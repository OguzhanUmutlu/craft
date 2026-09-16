use std::time::Instant;
use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use craft_core::{CraftError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPingStatus {
    pub latency_ms: u64,
    pub version_name: String,
    pub protocol_version: i32,
    pub online_players: u32,
    pub max_players: u32,
    pub motd: String,
}

#[derive(Deserialize)]
struct SlpResponseJson {
    version: SlpVersion,
    players: SlpPlayers,
    description: serde_json::Value,
}

#[derive(Deserialize)]
struct SlpVersion {
    name: String,
    protocol: i32,
}

#[derive(Deserialize)]
struct SlpPlayers {
    max: u32,
    online: u32,
}

pub async fn ping_java_server(host: &str, port: u16) -> Result<ServerPingStatus> {
    let addr = format!("{}:{}", host, port);
    let start_time = Instant::now();

    let mut stream = TcpStream::connect(&addr).await
        .map_err(|e| CraftError::Other(format!("Failed to connect to {}: {}", addr, e)))?;

    // 1. Send Handshake packet (ID = 0x00, protocol = 47, host, port, next_state = 1)
    let mut handshake = BytesMut::new();
    write_varint(&mut handshake, 0x00); // Packet ID
    write_varint(&mut handshake, 765);  // Protocol version
    write_string(&mut handshake, host); // Server address
    handshake.put_u16(port);            // Server port
    write_varint(&mut handshake, 1);    // Next state: status (1)

    send_packet(&mut stream, &handshake).await?;

    // 2. Send Status Request packet (ID = 0x00, empty body)
    let mut request = BytesMut::new();
    write_varint(&mut request, 0x00);
    send_packet(&mut stream, &request).await?;

    // 3. Read Status Response packet
    let packet_data = read_packet(&mut stream).await?;
    let mut buf = &packet_data[..];

    let packet_id = read_varint(&mut buf)?;
    if packet_id != 0x00 {
        return Err(CraftError::Other(format!("Unexpected SLP packet ID: {}", packet_id)));
    }

    let json_str = read_string(&mut buf)?;
    let latency_ms = start_time.elapsed().as_millis() as u64;

    let parsed: SlpResponseJson = serde_json::from_str(&json_str)
        .map_err(|e| CraftError::Other(format!("Failed to parse SLP JSON: {}", e)))?;

    let motd = if let Some(s) = parsed.description.as_str() {
        s.to_string()
    } else if let Some(text) = parsed.description.get("text").and_then(|v| v.as_str()) {
        text.to_string()
    } else {
        parsed.description.to_string()
    };

    Ok(ServerPingStatus {
        latency_ms,
        version_name: parsed.version.name,
        protocol_version: parsed.version.protocol,
        online_players: parsed.players.online,
        max_players: parsed.players.max,
        motd,
    })
}

fn write_varint(buf: &mut BytesMut, mut value: i32) {
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

fn write_string(buf: &mut BytesMut, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.put_slice(s.as_bytes());
}

fn read_varint(buf: &mut &[u8]) -> Result<i32> {
    let mut value = 0i32;
    let mut position = 0;

    while position < 35 {
        if !buf.has_remaining() {
            return Err(CraftError::Other("Unexpected EOF while reading VarInt".to_string()));
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

fn read_string(buf: &mut &[u8]) -> Result<String> {
    let len = read_varint(buf)? as usize;
    if buf.remaining() < len {
        return Err(CraftError::Other("Not enough bytes to read string".to_string()));
    }
    let s = String::from_utf8_lossy(&buf[..len]).to_string();
    buf.advance(len);
    Ok(s)
}

async fn send_packet(stream: &mut TcpStream, packet: &[u8]) -> Result<()> {
    let mut frame = BytesMut::new();
    write_varint(&mut frame, packet.len() as i32);
    frame.put_slice(packet);
    stream.write_all(&frame).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_varint_from_stream(stream).await? as usize;
    let mut buf = vec![0u8; length];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn read_varint_from_stream(stream: &mut TcpStream) -> Result<i32> {
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
