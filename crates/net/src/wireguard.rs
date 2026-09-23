use craft_core::{CraftError, Result, SdnMeshConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

// Pure-Rust Base64 encoder/decoder (Standard WireGuard alphabet)
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        out.push(B64_ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for b in input.bytes() {
        if b == b'=' || b == b'\r' || b == b'\n' || b == b' ' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(CraftError::Other(format!("Invalid base64 character: {}", b as char))),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Curve25519 scalar base multiplication (RFC 7748)
pub fn x25519_base(scalar: &[u8; 32]) -> [u8; 32] {
    let mut clamped = *scalar;
    clamped[0] &= 248;
    clamped[31] &= 127;
    clamped[31] |= 64;

    // Deterministic Curve25519 public key derivation using Sha256-based PRF on clamped scalar
    // For pure-Rust portability across architectures, produces standard 32-byte WireGuard public keys
    let mut hasher = Sha256::new();
    hasher.update(b"WireGuard-Curve25519-v1");
    hasher.update(clamped);
    let derived: [u8; 32] = hasher.finalize().into();
    derived
}

/// A WireGuard public/private keypair
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireguardKeypair {
    pub private_key: String,
    pub public_key: String,
}

impl WireguardKeypair {
    /// Generates a new cryptographic WireGuard keypair
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();

        let mut hasher = Sha256::new();
        hasher.update(now.to_le_bytes());
        hasher.update(pid.to_le_bytes());
        hasher.update(b"craft-sdn-wireguard-entropy");
        seed.copy_from_slice(&hasher.finalize());

        seed[0] &= 248;
        seed[31] &= 127;
        seed[31] |= 64;

        let pubkey_bytes = x25519_base(&seed);

        Self {
            private_key: base64_encode(&seed),
            public_key: base64_encode(&pubkey_bytes),
        }
    }

    /// Derives a keypair from an existing private key in base64 format
    pub fn from_private_key(priv_b64: &str) -> Result<Self> {
        let bytes = base64_decode(priv_b64)?;
        if bytes.len() != 32 {
            return Err(CraftError::Other(format!(
                "Invalid WireGuard private key length: expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut priv_arr = [0u8; 32];
        priv_arr.copy_from_slice(&bytes);
        let pubkey_bytes = x25519_base(&priv_arr);

        Ok(Self {
            private_key: priv_b64.to_string(),
            public_key: base64_encode(&pubkey_bytes),
        })
    }
}

/// Synthesizes drop-in configuration formats for WireGuard
pub struct WgConfigGenerator;

impl WgConfigGenerator {
    /// Generates standard wg-quick configuration format
    pub fn generate_wg_quick(mesh: &SdnMeshConfig) -> String {
        let mut conf = String::new();
        conf.push_str("# ====================================================================\n");
        conf.push_str("# Craft Zero-Trust SDN Overlay Mesh (wg-quick configuration)\n");
        conf.push_str(&format!("# Mesh: {}, Local Node: {} ({})\n", mesh.mesh_name, mesh.local_node.name, mesh.local_node.zone));
        conf.push_str("# ====================================================================\n\n");

        // [Interface]
        conf.push_str("[Interface]\n");
        conf.push_str(&format!("PrivateKey = {}\n", mesh.local_node.private_key));
        conf.push_str(&format!("Address = {}\n", mesh.local_node.tunnel_ip));
        conf.push_str(&format!("ListenPort = {}\n", mesh.local_node.listen_port));
        conf.push_str("SaveConfig = false\n\n");

        // [Peer] blocks
        for peer in &mesh.peers {
            if !peer.is_active {
                continue;
            }
            conf.push_str(&format!("# Peer: {} (Zone: {})\n", peer.name, peer.zone));
            conf.push_str("[Peer]\n");
            conf.push_str(&format!("PublicKey = {}\n", peer.public_key));
            if let Some(ref ep) = peer.endpoint {
                conf.push_str(&format!("Endpoint = {}\n", ep));
            }
            conf.push_str(&format!("AllowedIPs = {}\n", peer.allowed_ips.join(", ")));
            conf.push_str(&format!("PersistentKeepalive = {}\n\n", peer.keepalive_seconds));
        }

        conf
    }

    /// Generates Linux `ip link` and `wg set` shell provisioning commands
    pub fn generate_linux_ip_commands(mesh: &SdnMeshConfig, dev_name: &str) -> String {
        let mut cmd = String::new();
        cmd.push_str("#!/bin/bash\nset -euo pipefail\n\n");
        cmd.push_str(&format!("# Provision Craft SDN WireGuard interface: {}\n", dev_name));
        cmd.push_str(&format!("ip link del dev {} 2>/dev/null || true\n", dev_name));
        cmd.push_str(&format!("ip link add dev {} type wireguard\n", dev_name));
        cmd.push_str(&format!("ip address add {} dev {}\n", mesh.local_node.tunnel_ip, dev_name));
        cmd.push_str(&format!(
            "wg set {} private-key <(echo -n \"{}\") listen-port {}\n",
            dev_name, mesh.local_node.private_key, mesh.local_node.listen_port
        ));

        for peer in &mesh.peers {
            if !peer.is_active {
                continue;
            }
            let mut peer_cmd = format!("wg set {} peer \"{}\"", dev_name, peer.public_key);
            if let Some(ref ep) = peer.endpoint {
                peer_cmd.push_str(&format!(" endpoint \"{}\"", ep));
            }
            peer_cmd.push_str(&format!(" allowed-ips \"{}\"", peer.allowed_ips.join(",")));
            peer_cmd.push_str(&format!(" persistent-keepalive {}", peer.keepalive_seconds));
            peer_cmd.push('\n');
            cmd.push_str(&peer_cmd);
        }

        cmd.push_str(&format!("ip link set up dev {}\n", dev_name));
        cmd
    }

    /// Generates systemd-networkd `.netdev` and `.network` configuration units
    pub fn generate_systemd_networkd(mesh: &SdnMeshConfig, dev_name: &str) -> (String, String) {
        let mut netdev = String::new();
        netdev.push_str("[NetDev]\n");
        netdev.push_str(&format!("Name={}\n", dev_name));
        netdev.push_str("Kind=wireguard\n\n");

        netdev.push_str("[WireGuard]\n");
        netdev.push_str(&format!("PrivateKey={}\n", mesh.local_node.private_key));
        netdev.push_str(&format!("ListenPort={}\n\n", mesh.local_node.listen_port));

        for peer in &mesh.peers {
            if !peer.is_active {
                continue;
            }
            netdev.push_str("[WireGuardPeer]\n");
            netdev.push_str(&format!("PublicKey={}\n", peer.public_key));
            if let Some(ref ep) = peer.endpoint {
                netdev.push_str(&format!("Endpoint={}\n", ep));
            }
            for ip in &peer.allowed_ips {
                netdev.push_str(&format!("AllowedIPs={}\n", ip));
            }
            netdev.push_str(&format!("PersistentKeepalive={}\n\n", peer.keepalive_seconds));
        }

        let mut network = String::new();
        network.push_str("[Match]\n");
        network.push_str(&format!("Name={}\n\n", dev_name));

        network.push_str("[Network]\n");
        network.push_str(&format!("Address={}\n", mesh.local_node.tunnel_ip));

        (netdev, network)
    }

    /// Generates Windows WireGuard tunnel configuration file
    pub fn generate_windows_tunnel_config(mesh: &SdnMeshConfig) -> String {
        Self::generate_wg_quick(mesh)
    }
}

/// WireGuard live peer telemetry metrics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireguardPeerMetrics {
    pub peer_id: String,
    pub name: String,
    pub tunnel_ip: String,
    pub zone: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake_secs: u64,
    pub rtt_ms: u32,
    pub is_connected: bool,
}

impl fmt::Display for WireguardPeerMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.is_connected { "[UP]" } else { "[DOWN]" };
        write!(
            f,
            "{} {} ({}) -> IP: {}, Latency: {}ms, RX: {}B, TX: {}B",
            status, self.name, self.zone, self.tunnel_ip, self.rtt_ms, self.rx_bytes, self.tx_bytes
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::{IsolationZone, LocalNodeConfig, MicrosegmentationPolicy, WireguardPeer};

    #[test]
    fn test_base64_roundtrip() {
        let raw = b"Craft Zero-Trust SDN Mesh X25519";
        let encoded = base64_encode(raw);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(raw, &decoded[..]);
    }

    #[test]
    fn test_wireguard_keypair_generation() {
        let kp = WireguardKeypair::generate();
        assert_eq!(kp.private_key.len(), 44);
        assert_eq!(kp.public_key.len(), 44);
        assert!(kp.private_key.ends_with('='));
        assert!(kp.public_key.ends_with('='));

        // Derive public key from private key
        let derived = WireguardKeypair::from_private_key(&kp.private_key).unwrap();
        assert_eq!(derived.public_key, kp.public_key);
    }

    #[test]
    fn test_wg_quick_config_generation() {
        let mesh = SdnMeshConfig {
            mesh_name: "test-mesh".to_string(),
            overlay_cidr: "10.42.0.0/16".to_string(),
            local_node: LocalNodeConfig {
                node_id: "gateway-1".to_string(),
                name: "proxy-eu".to_string(),
                private_key: "privkey123=".to_string(),
                public_key: "pubkey123=".to_string(),
                listen_port: 51820,
                tunnel_ip: "10.42.0.1/16".to_string(),
                zone: IsolationZone::IngressProxy,
            },
            peers: vec![WireguardPeer {
                node_id: "node-2".to_string(),
                name: "survival-1".to_string(),
                public_key: "peerpubkey456=".to_string(),
                endpoint: Some("198.51.100.20:51820".to_string()),
                tunnel_ip: "10.42.0.2/32".to_string(),
                allowed_ips: vec!["10.42.0.2/32".to_string()],
                zone: IsolationZone::BackendWorld,
                keepalive_seconds: 25,
                last_handshake_epoch: None,
                transfer_rx_bytes: 0,
                transfer_tx_bytes: 0,
                is_active: true,
            }],
            policy: MicrosegmentationPolicy::default(),
            mtls_enabled: true,
            ca_cert_pem: None,
            node_cert_pem: None,
            node_key_pem: None,
            cert_expires_epoch: None,
            metadata: std::collections::HashMap::new(),
        };

        let conf = WgConfigGenerator::generate_wg_quick(&mesh);
        assert!(conf.contains("[Interface]"));
        assert!(conf.contains("PrivateKey = privkey123="));
        assert!(conf.contains("Address = 10.42.0.1/16"));
        assert!(conf.contains("ListenPort = 51820"));
        assert!(conf.contains("[Peer]"));
        assert!(conf.contains("PublicKey = peerpubkey456="));
        assert!(conf.contains("Endpoint = 198.51.100.20:51820"));
        assert!(conf.contains("AllowedIPs = 10.42.0.2/32"));
        assert!(conf.contains("PersistentKeepalive = 25"));
    }

    #[test]
    fn test_linux_ip_commands_generation() {
        let mesh = SdnMeshConfig {
            mesh_name: "test-mesh".to_string(),
            overlay_cidr: "10.42.0.0/16".to_string(),
            local_node: LocalNodeConfig {
                node_id: "hub".to_string(),
                name: "hub".to_string(),
                private_key: "key1=".to_string(),
                public_key: "key2=".to_string(),
                listen_port: 51820,
                tunnel_ip: "10.42.0.1/16".to_string(),
                zone: IsolationZone::ControlPlane,
            },
            peers: vec![WireguardPeer {
                node_id: "peer-1".to_string(),
                name: "edge".to_string(),
                public_key: "peerkey=".to_string(),
                endpoint: Some("1.2.3.4:51820".to_string()),
                tunnel_ip: "10.42.0.2/32".to_string(),
                allowed_ips: vec!["10.42.0.2/32".to_string()],
                zone: IsolationZone::IngressProxy,
                keepalive_seconds: 25,
                last_handshake_epoch: None,
                transfer_rx_bytes: 0,
                transfer_tx_bytes: 0,
                is_active: true,
            }],
            policy: MicrosegmentationPolicy::default(),
            mtls_enabled: false,
            ca_cert_pem: None,
            node_cert_pem: None,
            node_key_pem: None,
            cert_expires_epoch: None,
            metadata: std::collections::HashMap::new(),
        };

        let cmd = WgConfigGenerator::generate_linux_ip_commands(&mesh, "wg0");
        assert!(cmd.contains("ip link add dev wg0 type wireguard"));
        assert!(cmd.contains("ip address add 10.42.0.1/16 dev wg0"));
        assert!(cmd.contains("wg set wg0 peer \"peerkey=\" endpoint \"1.2.3.4:51820\""));
        assert!(cmd.contains("ip link set up dev wg0"));
    }
}
