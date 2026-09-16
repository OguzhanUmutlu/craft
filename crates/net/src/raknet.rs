use std::time::Instant;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use craft_core::{CraftError, Result};

pub const RAKNET_OFFLINE_MAGIC: &[u8] = &[
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockPingStatus {
    pub latency_ms: u64,
    pub server_name: String,
    pub protocol_version: String,
    pub version: String,
    pub online_players: u32,
    pub max_players: u32,
    pub world_name: String,
    pub game_mode: String,
}

pub async fn ping_bedrock_server(host: &str, port: u16) -> Result<BedrockPingStatus> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let target = format!("{}:{}", host, port);
    socket.connect(&target).await?;

    let mut packet = Vec::with_capacity(33);
    packet.push(0x01); // ID_UNCONNECTED_PING
    packet.extend_from_slice(&0u64.to_be_bytes()); // Ping time
    packet.extend_from_slice(RAKNET_OFFLINE_MAGIC);
    packet.extend_from_slice(&123456789u64.to_be_bytes()); // Client GUID

    let start_time = Instant::now();
    socket.send(&packet).await?;

    let mut recv_buf = [0u8; 1024];
    let len = timeout(Duration::from_secs(3), socket.recv(&mut recv_buf)).await
        .map_err(|_| CraftError::Other("Bedrock ping timed out".to_string()))??;

    let latency_ms = start_time.elapsed().as_millis() as u64;

    if len < 35 || recv_buf[0] != 0x1c {
        return Err(CraftError::Other("Invalid Bedrock Unconnected Pong packet".to_string()));
    }

    // Packet format: [1 byte ID (0x1c)] [8 bytes time] [8 bytes server GUID] [16 bytes magic] [2 bytes string len] [string]
    let str_len = u16::from_be_bytes([recv_buf[33], recv_buf[34]]) as usize;
    if len < 35 + str_len {
        return Err(CraftError::Other("Truncated Bedrock pong data".to_string()));
    }

    let payload = String::from_utf8_lossy(&recv_buf[35..35 + str_len]).to_string();
    Ok(parse_bedrock_pong_payload(&payload, latency_ms))
}

pub fn parse_bedrock_pong_payload(payload: &str, latency_ms: u64) -> BedrockPingStatus {
    let parts: Vec<&str> = payload.split(';').collect();
    // Format: MCPE;Server Name;Protocol;Version;Online;Max;ServerId;WorldName;GameMode
    BedrockPingStatus {
        latency_ms,
        server_name: parts.get(1).unwrap_or(&"").to_string(),
        protocol_version: parts.get(2).unwrap_or(&"").to_string(),
        version: parts.get(3).unwrap_or(&"").to_string(),
        online_players: parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0),
        max_players: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
        world_name: parts.get(7).unwrap_or(&"").to_string(),
        game_mode: parts.get(8).unwrap_or(&"").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bedrock_pong() {
        let payload = "MCPE;Dedicated Server;486;1.18.30;5;10;1234567890;Bedrock level;Survival";
        let status = parse_bedrock_pong_payload(payload, 25);
        assert_eq!(status.server_name, "Dedicated Server");
        assert_eq!(status.protocol_version, "486");
        assert_eq!(status.version, "1.18.30");
        assert_eq!(status.online_players, 5);
        assert_eq!(status.max_players, 10);
        assert_eq!(status.world_name, "Bedrock level");
        assert_eq!(status.game_mode, "Survival");
        assert_eq!(status.latency_ms, 25);
    }
}

