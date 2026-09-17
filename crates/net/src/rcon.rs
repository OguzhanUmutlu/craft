use craft_core::{CraftError, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SERVERDATA_AUTH: i32 = 3;
const SERVERDATA_EXECCOMMAND: i32 = 2;
const SERVERDATA_RESPONSE_VALUE: i32 = 0;

pub struct RconClient {
    stream: TcpStream,
    req_id: i32,
}

impl RconClient {
    pub async fn connect(host: &str, port: u16, password: &str) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| CraftError::Other(format!("Failed to connect to RCON {}: {}", addr, e)))?;

        let auth_id = 1;
        send_rcon_packet(&mut stream, auth_id, SERVERDATA_AUTH, password).await?;

        // Minecraft sends an empty response (type 0 or 2), then the auth result
        let (res_id, res_type, _) = read_rcon_packet(&mut stream).await?;
        if res_type == SERVERDATA_RESPONSE_VALUE {
            // Read next packet
            let (res_id2, _, _) = read_rcon_packet(&mut stream).await?;
            if res_id2 == -1 {
                return Err(CraftError::Other(
                    "RCON Authentication failed: Invalid password".to_string(),
                ));
            }
        } else if res_id == -1 {
            return Err(CraftError::Other(
                "RCON Authentication failed: Invalid password".to_string(),
            ));
        }

        Ok(Self { stream, req_id: 2 })
    }

    pub async fn send_command(&mut self, command: &str) -> Result<String> {
        let id = self.req_id;
        self.req_id += 1;

        send_rcon_packet(&mut self.stream, id, SERVERDATA_EXECCOMMAND, command).await?;
        let (res_id, _, payload) = read_rcon_packet(&mut self.stream).await?;

        if res_id != id {
            return Err(CraftError::Other("RCON response ID mismatch".to_string()));
        }

        Ok(payload)
    }
}

async fn send_rcon_packet(
    stream: &mut TcpStream,
    req_id: i32,
    packet_type: i32,
    payload: &str,
) -> Result<()> {
    let payload_bytes = payload.as_bytes();
    let length = (4 + 4 + payload_bytes.len() + 2) as i32;

    let mut buf = Vec::with_capacity((length + 4) as usize);
    buf.extend_from_slice(&length.to_le_bytes());
    buf.extend_from_slice(&req_id.to_le_bytes());
    buf.extend_from_slice(&packet_type.to_le_bytes());
    buf.extend_from_slice(payload_bytes);
    buf.push(0x00);
    buf.push(0x00);

    stream.write_all(&buf).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_rcon_packet(stream: &mut TcpStream) -> Result<(i32, i32, String)> {
    let length = stream.read_i32_le().await?;
    if !(10..=4096).contains(&length) {
        return Err(CraftError::Other("Invalid RCON packet length".to_string()));
    }

    let req_id = stream.read_i32_le().await?;
    let packet_type = stream.read_i32_le().await?;

    let payload_len = (length - 8 - 2) as usize;
    let mut payload_bytes = vec![0u8; payload_len];
    stream.read_exact(&mut payload_bytes).await?;

    // Read 2-byte null terminator
    let mut pad = [0u8; 2];
    stream.read_exact(&mut pad).await?;

    let payload = String::from_utf8_lossy(&payload_bytes).to_string();
    Ok((req_id, packet_type, payload))
}
