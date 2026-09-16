use std::net::SocketAddr;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::time::timeout;
use craft_core::{CraftError, QueryProtocolKind, Result};

use crate::a2s::{ping_a2s_server, A2sPingStatus};
use crate::raknet::{ping_bedrock_server, BedrockPingStatus};
use crate::slp::{ping_java_server, ServerPingStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UniversalPingStatus {
    MinecraftJava(ServerPingStatus),
    MinecraftBedrock(BedrockPingStatus),
    ValveA2S(A2sPingStatus),
    PortProbe {
        host: String,
        port: u16,
        latency_ms: u128,
        transport: String,
    },
}

pub async fn ping_server_auto(
    host: &str,
    port: u16,
    protocol: Option<QueryProtocolKind>,
) -> Result<UniversalPingStatus> {
    match protocol {
        Some(QueryProtocolKind::MinecraftJavaSlp) => {
            let res = ping_java_server(host, port).await?;
            Ok(UniversalPingStatus::MinecraftJava(res))
        }
        Some(QueryProtocolKind::MinecraftBedrockRakNet) => {
            let res = ping_bedrock_server(host, port).await?;
            Ok(UniversalPingStatus::MinecraftBedrock(res))
        }
        Some(QueryProtocolKind::ValveA2S) => {
            let res = ping_a2s_server(host, port).await?;
            Ok(UniversalPingStatus::ValveA2S(res))
        }
        Some(QueryProtocolKind::GenericPortProbe) => {
            probe_tcp_port(host, port).await
        }
        None => {
            // Auto-detect based on port conventions or fast probe
            if port == 19132 || port == 19133 {
                if let Ok(res) = ping_bedrock_server(host, port).await {
                    return Ok(UniversalPingStatus::MinecraftBedrock(res));
                }
            } else if port == 27015 || port == 2457 || port == 28016 {
                if let Ok(res) = ping_a2s_server(host, port).await {
                    return Ok(UniversalPingStatus::ValveA2S(res));
                }
            } else if let Ok(res) = ping_java_server(host, port).await {
                return Ok(UniversalPingStatus::MinecraftJava(res));
            } else if let Ok(res) = ping_a2s_server(host, port).await {
                return Ok(UniversalPingStatus::ValveA2S(res));
            } else if let Ok(res) = ping_bedrock_server(host, port).await {
                return Ok(UniversalPingStatus::MinecraftBedrock(res));
            }

            // Fallback to socket probe
            probe_tcp_port(host, port).await
        }
    }
}

pub async fn probe_tcp_port(host: &str, port: u16) -> Result<UniversalPingStatus> {
    let addr_str = format!("{}:{}", host, port);
    let start = Instant::now();

    let target: SocketAddr = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| CraftError::Other(format!("Failed to resolve '{}': {}", addr_str, e)))?
        .next()
        .ok_or_else(|| CraftError::Other(format!("No address found for '{}'", addr_str)))?;

    match timeout(Duration::from_secs(3), TcpStream::connect(target)).await {
        Ok(Ok(_)) => {
            let latency = start.elapsed().as_millis();
            Ok(UniversalPingStatus::PortProbe {
                host: host.to_string(),
                port,
                latency_ms: latency,
                transport: "TCP".to_string(),
            })
        }
        Ok(Err(e)) => Err(CraftError::Other(format!("Connection refused to {}: {}", addr_str, e))),
        Err(_) => Err(CraftError::Other(format!("Connection to {} timed out after 3s", addr_str))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_ping_serialization() {
        let status = UniversalPingStatus::PortProbe {
            host: "127.0.0.1".to_string(),
            port: 8211,
            latency_ms: 15,
            transport: "TCP".to_string(),
        };
        let json = serde_json::to_string(&status).expect("serialization works");
        let deserialized: UniversalPingStatus = serde_json::from_str(&json).expect("deserialization works");
        match deserialized {
            UniversalPingStatus::PortProbe { port, latency_ms, .. } => {
                assert_eq!(port, 8211);
                assert_eq!(latency_ms, 15);
            }
            _ => panic!("Expected PortProbe variant"),
        }
    }
}
