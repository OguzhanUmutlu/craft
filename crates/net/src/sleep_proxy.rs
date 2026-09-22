use bytes::{Buf, BufMut, BytesMut};
use craft_core::{CraftError, Result};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct SleepProxyConfig {
    pub bind_addr: SocketAddr,
    pub server_name: String,
    pub motd: String,
    pub version_name: String,
    pub protocol_version: i32,
    pub wake_kick_message: String,
}

impl Default for SleepProxyConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:25565".parse().unwrap(),
            server_name: "default".to_string(),
            motd: "[Craft] Server is sleeping. Connect to wake up!".to_string(),
            version_name: "Craft SleepProxy".to_string(),
            protocol_version: 765, // 1.20.4 compatible
            wake_kick_message: "[Craft] Server is starting up! Please reconnect in 15 seconds.".to_string(),
        }
    }
}

pub struct SleepProxyHandle {
    shutdown_tx: watch::Sender<bool>,
    is_running: Arc<AtomicBool>,
}

impl SleepProxyHandle {
    pub fn shutdown(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(true);
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

pub struct SleepProxy;

impl SleepProxy {
    pub async fn start(
        config: SleepProxyConfig,
        wake_sender: mpsc::Sender<String>,
    ) -> Result<SleepProxyHandle> {
        let listener = TcpListener::bind(config.bind_addr).await.map_err(|e| {
            CraftError::Other(format!(
                "Failed to bind SleepProxy to {}: {}",
                config.bind_addr, e
            ))
        })?;

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = Arc::clone(&is_running);

        let config = Arc::new(config);

        tokio::spawn(async move {
            info!(
                server = %config.server_name,
                addr = %config.bind_addr,
                "SleepProxy listening for SLP and player connection wakeups"
            );

            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, peer_addr)) => {
                                debug!(peer = %peer_addr, "SleepProxy accepted connection");
                                let cfg = Arc::clone(&config);
                                let wake_tx = wake_sender.clone();
                                tokio::spawn(async move {
                                    if let Err(err) = handle_client(stream, cfg, wake_tx).await {
                                        debug!(peer = %peer_addr, error = %err, "SleepProxy client session closed");
                                    }
                                });
                            }
                            Err(e) => {
                                warn!(error = %e, "SleepProxy accept error");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!(server = %config.server_name, "SleepProxy shutting down listener");
                            break;
                        }
                    }
                }
            }

            is_running_clone.store(false, Ordering::SeqCst);
        });

        Ok(SleepProxyHandle {
            shutdown_tx,
            is_running,
        })
    }
}

async fn handle_client(
    mut stream: TcpStream,
    config: Arc<SleepProxyConfig>,
    wake_sender: mpsc::Sender<String>,
) -> Result<()> {
    // 1. Read Handshake packet
    let packet_data = read_packet(&mut stream).await?;
    let mut buf = &packet_data[..];

    let packet_id = read_varint(&mut buf)?;
    if packet_id != 0x00 {
        return Err(CraftError::Other(format!(
            "Invalid handshake packet ID: {}",
            packet_id
        )));
    }

    let _protocol_version = read_varint(&mut buf)?;
    let _server_address = read_string(&mut buf)?;
    if buf.remaining() < 2 {
        return Err(CraftError::Other("Truncated handshake port".to_string()));
    }
    let _server_port = buf.get_u16();
    let next_state = read_varint(&mut buf)?;

    match next_state {
        1 => {
            // Status state
            handle_status_session(&mut stream, &config).await?;
        }
        2 => {
            // Login state: Trigger server wake-up and disconnect player with informative kick message
            info!(
                server = %config.server_name,
                "Player login detected on sleeping server! Triggering wake-up event."
            );
            let _ = wake_sender.send(config.server_name.clone()).await;

            // Try reading login start packet if available, but disconnect immediately
            let _ = read_packet(&mut stream).await;

            // Send Login Disconnect packet: Packet ID 0x00 with chat component
            let disconnect_payload = json!({
                "text": config.wake_kick_message
            })
            .to_string();

            let mut disconnect_packet = BytesMut::new();
            write_varint(&mut disconnect_packet, 0x00); // Disconnect packet ID in login
            write_string(&mut disconnect_packet, &disconnect_payload);

            send_packet(&mut stream, &disconnect_packet).await?;
            let _ = stream.flush().await;
        }
        _ => {
            return Err(CraftError::Other(format!(
                "Unknown next_state: {}",
                next_state
            )));
        }
    }

    Ok(())
}

async fn handle_status_session(stream: &mut TcpStream, config: &SleepProxyConfig) -> Result<()> {
    loop {
        let packet_data = match read_packet(stream).await {
            Ok(p) => p,
            Err(_) => return Ok(()), // Client disconnected normally
        };

        let mut buf = &packet_data[..];
        let packet_id = read_varint(&mut buf)?;

        match packet_id {
            0x00 => {
                // Status Request -> respond with Status Response (0x00)
                let response_json = json!({
                    "version": {
                        "name": config.version_name,
                        "protocol": config.protocol_version
                    },
                    "players": {
                        "max": 0,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": config.motd
                    }
                })
                .to_string();

                let mut resp_packet = BytesMut::new();
                write_varint(&mut resp_packet, 0x00);
                write_string(&mut resp_packet, &response_json);
                send_packet(stream, &resp_packet).await?;
            }
            0x01 => {
                // Ping Request (payload is 8 bytes timestamp) -> Pong Response (0x01)
                let payload = buf.to_vec();
                let mut resp_packet = BytesMut::new();
                write_varint(&mut resp_packet, 0x01);
                resp_packet.put_slice(&payload);
                send_packet(stream, &resp_packet).await?;
                // After pong, Minecraft client terminates connection
                return Ok(());
            }
            _ => {
                debug!(packet_id, "Unknown packet in status state");
                return Ok(());
            }
        }
    }
}

pub fn write_varint(buf: &mut BytesMut, mut value: i32) {
    loop {
        if (value & !0x7F) == 0 {
            buf.put_u8(value as u8);
            return;
        } else {
            buf.put_u8(((value & 0x7F) | 0x80) as u8);
            value = (value as u32 >> 7) as i32;
        }
    }
}

pub fn write_string(buf: &mut BytesMut, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.put_slice(s.as_bytes());
}

pub fn read_varint(buf: &mut &[u8]) -> Result<i32> {
    let mut value = 0i32;
    let mut position = 0;

    while position < 35 {
        if !buf.has_remaining() {
            return Err(CraftError::Other(
                "Unexpected EOF while reading VarInt".to_string(),
            ));
        }
        let byte = buf.get_u8();
        value |= ((byte & 0x7F) as i32) << position;
        if (byte & 0x80) == 0 {
            return Ok(value);
        }
        position += 7;
    }

    Err(CraftError::Other("VarInt is too large".to_string()))
}

pub fn read_string(buf: &mut &[u8]) -> Result<String> {
    let len = read_varint(buf)? as usize;
    if buf.remaining() < len {
        return Err(CraftError::Other(
            "Not enough bytes to read string".to_string(),
        ));
    }
    let s = String::from_utf8_lossy(&buf[..len]).to_string();
    buf.advance(len);
    Ok(s)
}

pub async fn send_packet(stream: &mut TcpStream, packet: &[u8]) -> Result<()> {
    let mut frame = BytesMut::new();
    write_varint(&mut frame, packet.len() as i32);
    frame.put_slice(packet);
    stream.write_all(&frame).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_varint_from_stream(stream).await? as usize;
    let mut buf = vec![0u8; length];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

pub async fn read_varint_from_stream(stream: &mut TcpStream) -> Result<i32> {
    let mut value = 0i32;
    let mut position = 0;

    while position < 35 {
        let byte = stream.read_u8().await?;
        value |= ((byte & 0x7F) as i32) << position;
        if (byte & 0x80) == 0 {
            return Ok(value);
        }
        position += 7;
    }

    Err(CraftError::Other("VarInt is too large".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_varint_roundtrip() {
        let test_values = vec![0, 1, 127, 128, 255, 25565, 2097151, -1, -2147483648];
        for val in test_values {
            let mut buf = BytesMut::new();
            write_varint(&mut buf, val);
            let mut slice = &buf[..];
            let decoded = read_varint(&mut slice).expect("decode varint");
            assert_eq!(val, decoded);
            assert_eq!(slice.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_string_roundtrip() {
        let test_str = "hello world from craft sleep proxy! [Online]";
        let mut buf = BytesMut::new();
        write_string(&mut buf, test_str);
        let mut slice = &buf[..];
        let decoded = read_string(&mut slice).expect("decode string");
        assert_eq!(test_str, decoded);
        assert_eq!(slice.len(), 0);
    }

    #[tokio::test]
    async fn test_sleep_proxy_status_query() {
        let (wake_tx, mut wake_rx) = mpsc::channel(10);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config = SleepProxyConfig {
            bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
            server_name: "test_srv".to_string(),
            motd: "Sleeping Server MOTD".to_string(),
            version_name: "Craft Sleep 1.20".to_string(),
            protocol_version: 765,
            wake_kick_message: "Waking up server!".to_string(),
        };

        let proxy = SleepProxy::start(config, wake_tx).await.unwrap();

        // Connect client
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Send handshake next_state = 1
        let mut handshake = BytesMut::new();
        write_varint(&mut handshake, 0x00);
        write_varint(&mut handshake, 765);
        write_string(&mut handshake, "127.0.0.1");
        handshake.put_u16(port);
        write_varint(&mut handshake, 1);
        send_packet(&mut client, &handshake).await.unwrap();

        // Send status request
        let mut status_req = BytesMut::new();
        write_varint(&mut status_req, 0x00);
        send_packet(&mut client, &status_req).await.unwrap();

        // Read status response
        let resp_bytes = read_packet(&mut client).await.unwrap();
        let mut resp_slice = &resp_bytes[..];
        let p_id = read_varint(&mut resp_slice).unwrap();
        assert_eq!(p_id, 0x00);
        let json_str = read_string(&mut resp_slice).unwrap();
        assert!(json_str.contains("Sleeping Server MOTD"));
        assert!(json_str.contains("Craft Sleep 1.20"));

        // Wake channel should NOT have triggered
        assert!(wake_rx.try_recv().is_err());

        proxy.shutdown();
    }

    #[tokio::test]
    async fn test_sleep_proxy_login_wake() {
        let (wake_tx, mut wake_rx) = mpsc::channel(10);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config = SleepProxyConfig {
            bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
            server_name: "lobby".to_string(),
            motd: "Sleeping Lobby".to_string(),
            version_name: "Craft Sleep 1.20".to_string(),
            protocol_version: 765,
            wake_kick_message: "Waking up lobby! Please reconnect in 15 seconds.".to_string(),
        };

        let proxy = SleepProxy::start(config, wake_tx).await.unwrap();

        // Connect client
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Send handshake next_state = 2 (Login)
        let mut handshake = BytesMut::new();
        write_varint(&mut handshake, 0x00);
        write_varint(&mut handshake, 765);
        write_string(&mut handshake, "127.0.0.1");
        handshake.put_u16(port);
        write_varint(&mut handshake, 2);
        send_packet(&mut client, &handshake).await.unwrap();

        // Send login start
        let mut login_start = BytesMut::new();
        write_varint(&mut login_start, 0x00);
        write_string(&mut login_start, "Steve");
        send_packet(&mut client, &login_start).await.unwrap();

        // Read login disconnect packet
        let resp_bytes = read_packet(&mut client).await.unwrap();
        let mut resp_slice = &resp_bytes[..];
        let p_id = read_varint(&mut resp_slice).unwrap();
        assert_eq!(p_id, 0x00);
        let kick_json = read_string(&mut resp_slice).unwrap();
        assert!(kick_json.contains("Waking up lobby!"));

        // Check wake channel
        let woken_server = wake_rx.recv().await.unwrap();
        assert_eq!(woken_server, "lobby");

        proxy.shutdown();
    }
}
