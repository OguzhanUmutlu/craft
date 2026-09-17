use craft_core::{CraftError, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;

const A2S_REQUEST: &[u8] = b"\xFF\xFF\xFF\xFF\x54Source Engine Query\0";
const A2S_RESPONSE_INFO: u8 = 0x49; // 'I'
const A2S_CHALLENGE_HEADER: u8 = 0x41; // 'A'
const TIMEOUT_DURATION: Duration = Duration::from_millis(3500);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct A2sPingStatus {
    pub server_name: String,
    pub map_name: String,
    pub game_folder: String,
    pub game_name: String,
    pub app_id: u16,
    pub online_players: u8,
    pub max_players: u8,
    pub bots: u8,
    pub server_type: String,
    pub environment: String,
    pub password_protected: bool,
    pub vac_secured: bool,
    pub latency_ms: u128,
}

pub async fn ping_a2s_server(host: &str, port: u16) -> Result<A2sPingStatus> {
    let addr_str = format!("{}:{}", host, port);
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| CraftError::Other(format!("Failed to bind local UDP socket: {}", e)))?;

    let start_time = Instant::now();

    // Resolve target
    let target: SocketAddr = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| {
            CraftError::Other(format!(
                "Failed to resolve target address '{}': {}",
                addr_str, e
            ))
        })?
        .next()
        .ok_or_else(|| CraftError::Other(format!("No address found for '{}'", addr_str)))?;

    // Send initial query
    socket
        .send_to(A2S_REQUEST, target)
        .await
        .map_err(|e| CraftError::Other(format!("Failed to send A2S query packet: {}", e)))?;

    let mut buf = [0u8; 4096];

    let (len, _) = timeout(TIMEOUT_DURATION, socket.recv_from(&mut buf))
        .await
        .map_err(|_| {
            CraftError::Other(format!(
                "A2S ping to {} timed out after {}s",
                addr_str,
                TIMEOUT_DURATION.as_secs()
            ))
        })?
        .map_err(|e| CraftError::Other(format!("Failed to receive UDP response: {}", e)))?;

    let latency = start_time.elapsed().as_millis();

    if len < 5 {
        return Err(CraftError::Other("A2S response too short".to_string()));
    }

    // Check challenge
    if buf[0..4] == [0xFF, 0xFF, 0xFF, 0xFF] && buf[4] == A2S_CHALLENGE_HEADER {
        if len < 9 {
            return Err(CraftError::Other(
                "A2S challenge packet malformed".to_string(),
            ));
        }
        let challenge = &buf[5..9];
        let mut request_with_challenge = Vec::with_capacity(A2S_REQUEST.len() + 4);
        request_with_challenge.extend_from_slice(A2S_REQUEST);
        request_with_challenge.extend_from_slice(challenge);

        socket
            .send_to(&request_with_challenge, target)
            .await
            .map_err(|e| CraftError::Other(format!("Failed to send challenge response: {}", e)))?;

        let (resp_len, _) = timeout(TIMEOUT_DURATION, socket.recv_from(&mut buf))
            .await
            .map_err(|_| {
                CraftError::Other(format!("A2S challenge response timed out for {}", addr_str))
            })?
            .map_err(|e| {
                CraftError::Other(format!("Failed to receive challenge response: {}", e))
            })?;

        return parse_a2s_info(&buf[..resp_len], latency);
    }

    parse_a2s_info(&buf[..len], latency)
}

pub fn parse_a2s_info(buf: &[u8], latency: u128) -> Result<A2sPingStatus> {
    if buf.len() < 5 {
        return Err(CraftError::Other(
            "A2S response payload too short".to_string(),
        ));
    }

    if buf[0..4] != [0xFF, 0xFF, 0xFF, 0xFF] {
        return Err(CraftError::Other("Invalid A2S packet header".to_string()));
    }

    if buf[4] != A2S_RESPONSE_INFO {
        return Err(CraftError::Other(format!(
            "Unexpected A2S response header: 0x{:02X}",
            buf[4]
        )));
    }

    let mut cursor = 5;

    // Protocol version byte
    if cursor >= buf.len() {
        return Err(CraftError::Other(
            "Truncated A2S packet at protocol byte".to_string(),
        ));
    }
    cursor += 1;

    let server_name = read_null_term_string(buf, &mut cursor)?;
    let map_name = read_null_term_string(buf, &mut cursor)?;
    let game_folder = read_null_term_string(buf, &mut cursor)?;
    let game_name = read_null_term_string(buf, &mut cursor)?;

    if cursor + 2 > buf.len() {
        return Err(CraftError::Other(
            "Truncated A2S packet at app_id".to_string(),
        ));
    }
    let app_id = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]);
    cursor += 2;

    if cursor + 7 > buf.len() {
        return Err(CraftError::Other(
            "Truncated A2S packet at players metadata".to_string(),
        ));
    }
    let online_players = buf[cursor];
    cursor += 1;
    let max_players = buf[cursor];
    cursor += 1;
    let bots = buf[cursor];
    cursor += 1;

    let server_type_byte = buf[cursor];
    cursor += 1;
    let server_type = match server_type_byte {
        b'd' => "Dedicated Server".to_string(),
        b'l' => "Non-Dedicated / Listen Server".to_string(),
        b'p' => "SourceTV / Proxy Relay".to_string(),
        c => format!("Server ({})", c as char),
    };

    let env_byte = buf[cursor];
    cursor += 1;
    let environment = match env_byte {
        b'l' => "Linux".to_string(),
        b'w' => "Windows".to_string(),
        b'm' | b'o' => "macOS".to_string(),
        c => format!("OS ({})", c as char),
    };

    let visibility = buf[cursor];
    cursor += 1;
    let password_protected = visibility != 0;

    let vac = if cursor < buf.len() {
        buf[cursor] != 0
    } else {
        false
    };

    Ok(A2sPingStatus {
        server_name,
        map_name,
        game_folder,
        game_name,
        app_id,
        online_players,
        max_players,
        bots,
        server_type,
        environment,
        password_protected,
        vac_secured: vac,
        latency_ms: latency,
    })
}

fn read_null_term_string(buf: &[u8], cursor: &mut usize) -> Result<String> {
    let start = *cursor;
    while *cursor < buf.len() && buf[*cursor] != 0 {
        *cursor += 1;
    }
    if *cursor >= buf.len() {
        return Err(CraftError::Other(
            "Unterminated null string in A2S response".to_string(),
        ));
    }
    let slice = &buf[start..*cursor];
    *cursor += 1; // skip null byte
    Ok(String::from_utf8_lossy(slice).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_a2s_info_valid() {
        // Construct mock A2S_INFO payload
        let mut mock = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x49, 17]; // Header + Protocol 17
        mock.extend_from_slice(b"Craft Palworld Official\0");
        mock.extend_from_slice(b"PalMap\0");
        mock.extend_from_slice(b"Pal\0");
        mock.extend_from_slice(b"Palworld\0");
        mock.extend_from_slice(&((2394010u32 & 0xFFFF) as u16).to_le_bytes()); // App ID (standard u16)
        mock.push(12); // 12 players
        mock.push(32); // 32 max players
        mock.push(0); // 0 bots
        mock.push(b'd'); // dedicated
        mock.push(b'l'); // linux
        mock.push(0); // public (not passworded)
        mock.push(1); // vac secured

        let res = parse_a2s_info(&mock, 25).expect("Should parse valid A2S packet");
        assert_eq!(res.server_name, "Craft Palworld Official");
        assert_eq!(res.map_name, "PalMap");
        assert_eq!(res.game_folder, "Pal");
        assert_eq!(res.game_name, "Palworld");
        assert_eq!(res.online_players, 12);
        assert_eq!(res.max_players, 32);
        assert_eq!(res.server_type, "Dedicated Server");
        assert_eq!(res.environment, "Linux");
        assert!(!res.password_protected);
        assert!(res.vac_secured);
        assert_eq!(res.latency_ms, 25);
    }
}
