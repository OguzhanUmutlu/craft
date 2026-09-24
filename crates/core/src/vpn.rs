//! Core data models, registry, and plain-text renderers for Autonomous Quantum-Encrypted
//! Inter-Cluster VPN Mesh (WireGuard PQXDH & P4 Crypto Offloading).
//! Strictly zero emojis anywhere.

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 * 1024 {
        format!("{:.2} TB", bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Operational state of a WireGuard PQXDH VPN tunnel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VpnTunnelState {
    Active,
    Negotiating,
    Degraded,
    RotatingKeys,
    Down,
}

impl fmt::Display for VpnTunnelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "ACTIVE"),
            Self::Negotiating => write!(f, "NEGOTIATING"),
            Self::Degraded => write!(f, "DEGRADED"),
            Self::RotatingKeys => write!(f, "ROTATING_KEYS"),
            Self::Down => write!(f, "DOWN"),
        }
    }
}

impl FromStr for VpnTunnelState {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "negotiating" => Ok(Self::Negotiating),
            "degraded" => Ok(Self::Degraded),
            "rotating_keys" | "rotating" => Ok(Self::RotatingKeys),
            "down" => Ok(Self::Down),
            other => Err(CraftError::Config(format!("Unknown VPN tunnel state: {}", other))),
        }
    }
}

/// Cryptographic cipher suite and execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VpnCryptoMode {
    #[serde(rename = "hardware_p4", alias = "hardware_offload_p4")]
    HardwareOffloadP4,
    #[serde(rename = "hybrid_kyber_chacha", alias = "hybrid_kyber_cha_cha", alias = "hybrid_pqxdh")]
    HybridKyberChaCha,
    #[serde(rename = "software_kernel")]
    SoftwareKernel,
    #[serde(rename = "simulated_wireguard")]
    SimulatedWireGuard,
}

impl fmt::Display for VpnCryptoMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HardwareOffloadP4 => write!(f, "HARDWARE_P4_OFFLOAD"),
            Self::HybridKyberChaCha => write!(f, "HYBRID_KYBER1024_CHACHA"),
            Self::SoftwareKernel => write!(f, "SOFTWARE_KERNEL"),
            Self::SimulatedWireGuard => write!(f, "SIMULATED_WIREGUARD"),
        }
    }
}

impl FromStr for VpnCryptoMode {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hardware" | "p4" | "hardware_p4" | "hardware_offload_p4" => Ok(Self::HardwareOffloadP4),
            "hybrid" | "kyber" | "hybrid_kyber_chacha" | "hybrid_kyber_cha_cha" | "hybrid_pqxdh" | "pqxdh" => Ok(Self::HybridKyberChaCha),
            "kernel" | "software" | "software_kernel" => Ok(Self::SoftwareKernel),
            "simulated" | "simulated_wireguard" | "wireguard" => Ok(Self::SimulatedWireGuard),
            other => Err(CraftError::Config(format!("Unknown VPN crypto mode: {}", other))),
        }
    }
}

/// Automated key rotation policy thresholds
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnKeyRotationPolicy {
    pub timer_secs: u64,
    pub volume_bytes: u64,
    pub auto_rotate: bool,
}

impl Default for VpnKeyRotationPolicy {
    fn default() -> Self {
        Self {
            timer_secs: 120, // Re-key every 2 minutes
            volume_bytes: 1_073_741_824, // Re-key every 1 GB
            auto_rotate: true,
        }
    }
}

/// Handshake stage for PQXDH noise exchange
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PqxdhHandshakeStage {
    Initial,
    EphemeralGenerated,
    Encapsulated,
    KeysDerived,
    TransportEstablished,
}

impl fmt::Display for PqxdhHandshakeStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => write!(f, "INITIAL"),
            Self::EphemeralGenerated => write!(f, "EPHEMERAL_GENERATED"),
            Self::Encapsulated => write!(f, "ENCAPSULATED"),
            Self::KeysDerived => write!(f, "KEYS_DERIVED"),
            Self::TransportEstablished => write!(f, "TRANSPORT_ESTABLISHED"),
        }
    }
}

/// Active PQXDH noise handshake state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqxdhHandshakeState {
    pub stage: PqxdhHandshakeStage,
    pub local_ephemeral_pk: String,
    pub remote_ciphertext_len: usize,
    pub shared_secret_hash: String,
    pub established_at_secs: u64,
}

impl Default for PqxdhHandshakeState {
    fn default() -> Self {
        Self {
            stage: PqxdhHandshakeStage::Initial,
            local_ephemeral_pk: String::new(),
            remote_ciphertext_len: 0,
            shared_secret_hash: String::new(),
            established_at_secs: 0,
        }
    }
}

/// Peer configuration in VPN mesh
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnPeerConfig {
    pub peer_id: String,
    pub public_key: String,
    pub pqxdh_prekey: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive_secs: u16,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake_secs: u64,
    pub last_rotation_secs: u64,
    pub quantum_safe: bool,
}

/// WireGuard VPN tunnel interface descriptor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnTunnelDescriptor {
    pub tunnel_id: String,
    pub interface_name: String,
    pub local_address: String,
    pub listen_port: u16,
    pub crypto_mode: VpnCryptoMode,
    pub peers: Vec<VpnPeerConfig>,
    pub rotation_policy: VpnKeyRotationPolicy,
    pub mtu: u16,
    pub state: VpnTunnelState,
    pub created_at_secs: u64,
}

/// Cumulative status summary across all VPN tunnels and peers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnStatusSummary {
    pub active_tunnels: usize,
    pub active_peers: usize,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub throughput_gbps: f64,
    pub avg_latency_micros: f64,
    pub key_rotations_total: u64,
    pub quantum_defense_score: f64,
    pub hardware_offload_active: bool,
}

impl Default for VpnStatusSummary {
    fn default() -> Self {
        Self {
            active_tunnels: 1,
            active_peers: 0,
            total_rx_bytes: 0,
            total_tx_bytes: 0,
            throughput_gbps: 0.0,
            avg_latency_micros: 24.8,
            key_rotations_total: 0,
            quantum_defense_score: 100.0,
            hardware_offload_active: true,
        }
    }
}

/// Benchmark performance metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnBenchmarkMetrics {
    pub packets_processed: usize,
    pub packet_size: usize,
    pub throughput_gbps: f64,
    pub encryption_latency_nanos: u64,
    pub renegotiation_latency_micros: f64,
    pub packet_loss_percent: f64,
    pub key_rotations: u64,
}

/// Registry of VPN mesh tunnels and persistent state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnRegistry {
    pub tunnels: Vec<VpnTunnelDescriptor>,
    pub status: VpnStatusSummary,
}

impl Default for VpnRegistry {
    fn default() -> Self {
        let default_tunnel = VpnTunnelDescriptor {
            tunnel_id: "craft-wg0".to_string(),
            interface_name: "craft-wg0".to_string(),
            local_address: "10.42.0.1/24".to_string(),
            listen_port: 51820,
            crypto_mode: VpnCryptoMode::HardwareOffloadP4,
            peers: vec![
                VpnPeerConfig {
                    peer_id: "us-east-core".to_string(),
                    public_key: "k1024-pub-useast0001".to_string(),
                    pqxdh_prekey: "pq-prekey-useast0001".to_string(),
                    endpoint: "198.51.100.10:51820".to_string(),
                    allowed_ips: vec!["10.42.0.2/32".to_string(), "10.100.0.0/16".to_string()],
                    persistent_keepalive_secs: 25,
                    tx_bytes: 1024 * 1024 * 540,
                    rx_bytes: 1024 * 1024 * 610,
                    last_handshake_secs: 12,
                    last_rotation_secs: 48,
                    quantum_safe: true,
                },
                VpnPeerConfig {
                    peer_id: "eu-central-replica".to_string(),
                    public_key: "k1024-pub-eucentral0002".to_string(),
                    pqxdh_prekey: "pq-prekey-eucentral0002".to_string(),
                    endpoint: "203.0.113.25:51820".to_string(),
                    allowed_ips: vec!["10.42.0.3/32".to_string(), "10.200.0.0/16".to_string()],
                    persistent_keepalive_secs: 25,
                    tx_bytes: 1024 * 1024 * 320,
                    rx_bytes: 1024 * 1024 * 390,
                    last_handshake_secs: 22,
                    last_rotation_secs: 52,
                    quantum_safe: true,
                },
            ],
            rotation_policy: VpnKeyRotationPolicy::default(),
            mtu: 1420,
            state: VpnTunnelState::Active,
            created_at_secs: current_epoch_secs(),
        };

        Self {
            tunnels: vec![default_tunnel],
            status: VpnStatusSummary::default(),
        }
    }
}

impl VpnRegistry {
    /// Loads registry from disk or initializes default
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.vpn_dir.exists() {
            let _ = fs::create_dir_all(&paths.vpn_dir);
        }
        if !paths.locks_dir.exists() {
            let _ = fs::create_dir_all(&paths.locks_dir);
        }

        let file_path = &paths.vpn_registry_file;
        if !file_path.exists() {
            let reg = Self::default();
            reg.save(paths)?;
            return Ok(reg);
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.vpn_lock)?;
        lock_file.lock_shared()?;

        let mut file = OpenOptions::new().read(true).open(file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let _ = lock_file.unlock();

        if content.trim().is_empty() {
            let reg = Self::default();
            reg.save(paths)?;
            return Ok(reg);
        }

        let reg: Self = serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse VPN registry: {}", e)))?;
        Ok(reg)
    }

    /// Saves registry to disk with exclusive advisory locking
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if !paths.vpn_dir.exists() {
            let _ = fs::create_dir_all(&paths.vpn_dir);
        }
        if !paths.locks_dir.exists() {
            let _ = fs::create_dir_all(&paths.locks_dir);
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.vpn_lock)?;
        lock_file.lock_exclusive()?;

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize VPN registry: {}", e)))?;

        let tmp_path = paths.vpn_dir.join("registry.json.tmp");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.flush()?;
        }

        fs::rename(tmp_path, &paths.vpn_registry_file)?;
        let _ = lock_file.unlock();
        Ok(())
    }

    /// Modifies the registry transactionally
    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if !paths.vpn_dir.exists() {
            let _ = fs::create_dir_all(&paths.vpn_dir);
        }
        if !paths.locks_dir.exists() {
            let _ = fs::create_dir_all(&paths.locks_dir);
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.vpn_lock)?;
        lock_file.lock_exclusive()?;

        let res = f(self)?;
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize VPN registry: {}", e)))?;

        let tmp_path = paths.vpn_dir.join("registry.json.tmp");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.flush()?;
        }

        fs::rename(tmp_path, &paths.vpn_registry_file)?;
        let _ = lock_file.unlock();
        Ok(res)
    }
}

/// Plain-text status table renderer (strictly zero emojis)
pub fn render_vpn_status_text(summary: &VpnStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("   CRAFT QUANTUM-ENCRYPTED INTER-CLUSTER VPN MESH & WIREGUARD PQXDH STATUS     \n");
    out.push_str("================================================================================\n");
    out.push_str(&format!("{:<28} : {}\n", "Active Tunnels", summary.active_tunnels));
    out.push_str(&format!("{:<28} : {}\n", "Active Mesh Peers", summary.active_peers));
    out.push_str(&format!("{:<28} : {}\n", "Total Ingress Volume", format_bytes(summary.total_rx_bytes)));
    out.push_str(&format!("{:<28} : {}\n", "Total Egress Volume", format_bytes(summary.total_tx_bytes)));
    out.push_str(&format!("{:<28} : {:.2} Gbps\n", "Current Throughput", summary.throughput_gbps));
    out.push_str(&format!("{:<28} : {:.2} us\n", "Average Packet Latency", summary.avg_latency_micros));
    out.push_str(&format!("{:<28} : {}\n", "Key Rotations Completed", summary.key_rotations_total));
    out.push_str(&format!("{:<28} : {:.1}%\n", "Quantum Defense Grade", summary.quantum_defense_score));
    out.push_str(&format!("{:<28} : {}\n", "SmartNIC Crypto Offload", if summary.hardware_offload_active { "ACTIVE (P4 ASIC)" } else { "DISABLED" }));
    out.push_str("--------------------------------------------------------------------------------\n");
    out
}

/// Plain-text table renderer for configured tunnels (strictly zero emojis)
pub fn render_vpn_tunnels_text(tunnels: &[VpnTunnelDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("                 CONFIGURED WIREGUARD PQXDH VPN TUNNELS                         \n");
    out.push_str("================================================================================\n");
    if tunnels.is_empty() {
        out.push_str("  No tunnels configured. Use 'craft vpn tunnel-create' to initialize a tunnel.\n");
        out.push_str("--------------------------------------------------------------------------------\n");
        return out;
    }

    out.push_str(&format!(
        "{:<14} {:<12} {:<18} {:<8} {:<24} {:<8} {:<10}\n",
        "TUNNEL_ID", "INTERFACE", "LOCAL_ADDR", "PORT", "CRYPTO_MODE", "PEERS", "STATUS"
    ));
    out.push_str("--------------------------------------------------------------------------------\n");
    for t in tunnels {
        out.push_str(&format!(
            "{:<14} {:<12} {:<18} {:<8} {:<24} {:<8} {:<10}\n",
            t.tunnel_id,
            t.interface_name,
            t.local_address,
            t.listen_port,
            t.crypto_mode.to_string(),
            t.peers.len(),
            t.state.to_string(),
        ));
    }
    out.push_str("--------------------------------------------------------------------------------\n");
    out
}

/// Plain-text table renderer for configured peers (strictly zero emojis)
pub fn render_vpn_peers_text(peers: &[VpnPeerConfig]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("                    WIREGUARD PQXDH MESH PEERS & ROUTES                         \n");
    out.push_str("================================================================================\n");
    if peers.is_empty() {
        out.push_str("  No peers connected in tunnel. Use 'craft vpn peer-add' to establish peering.\n");
        out.push_str("--------------------------------------------------------------------------------\n");
        return out;
    }

    out.push_str(&format!(
        "{:<16} {:<22} {:<18} {:<12} {:<12} {:<6}\n",
        "PEER_ID", "ENDPOINT", "ALLOWED_IPS", "RX_VOLUME", "TX_VOLUME", "QUANTUM"
    ));
    out.push_str("--------------------------------------------------------------------------------\n");
    for p in peers {
        let ips = if p.allowed_ips.is_empty() {
            "-".to_string()
        } else {
            p.allowed_ips.join(",")
        };
        out.push_str(&format!(
            "{:<16} {:<22} {:<18} {:<12} {:<12} {:<6}\n",
            p.peer_id,
            p.endpoint,
            ips,
            format_bytes(p.rx_bytes),
            format_bytes(p.tx_bytes),
            if p.quantum_safe { "YES" } else { "NO" }
        ));
    }
    out.push_str("--------------------------------------------------------------------------------\n");
    out
}

/// Plain-text table renderer for VPN benchmark metrics (strictly zero emojis)
pub fn render_vpn_bench_text(bench: &VpnBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("       WIREGUARD PQXDH & P4 HARDWARE CRYPTO BENCHMARK RESULTS                   \n");
    out.push_str("================================================================================\n");
    out.push_str(&format!("{:<30} : {}\n", "Packets Processed", bench.packets_processed));
    out.push_str(&format!("{:<30} : {} bytes\n", "Packet Size", bench.packet_size));
    out.push_str(&format!("{:<30} : {:.2} Gbps\n", "Line-Rate Throughput", bench.throughput_gbps));
    out.push_str(&format!("{:<30} : {} ns\n", "Avg Encryption Latency", bench.encryption_latency_nanos));
    out.push_str(&format!("{:<30} : {:.2} us\n", "Key Renegotiation Latency", bench.renegotiation_latency_micros));
    out.push_str(&format!("{:<30} : {:.2}%\n", "Packet Loss During Re-key", bench.packet_loss_percent));
    out.push_str(&format!("{:<30} : {}\n", "Key Rotations Verified", bench.key_rotations));
    out.push_str("--------------------------------------------------------------------------------\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vpn_registry_lifecycle_and_locking() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        fs::create_dir_all(&paths.vpn_dir).unwrap();
        fs::create_dir_all(&paths.locks_dir).unwrap();

        let mut reg = VpnRegistry::load(&paths).unwrap();
        assert_eq!(reg.tunnels.len(), 1);
        assert_eq!(reg.tunnels[0].tunnel_id, "craft-wg0");
        assert_eq!(reg.tunnels[0].state, VpnTunnelState::Active);

        reg.modify(&paths, |r| {
            r.status.key_rotations_total += 5;
            r.status.throughput_gbps = 10.5;
            Ok(())
        }).unwrap();

        let loaded = VpnRegistry::load(&paths).unwrap();
        assert_eq!(loaded.status.key_rotations_total, 5);
        assert_eq!(loaded.status.throughput_gbps, 10.5);
    }

    #[test]
    fn test_vpn_tunnel_peer_management() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        fs::create_dir_all(&paths.vpn_dir).unwrap();
        fs::create_dir_all(&paths.locks_dir).unwrap();

        let mut reg = VpnRegistry::load(&paths).unwrap();
        let peer = VpnPeerConfig {
            peer_id: "cluster-node-2".to_string(),
            public_key: "curve25519-public-key-base64".to_string(),
            pqxdh_prekey: "kyber1024-prekey-base64".to_string(),
            endpoint: "10.200.0.2:51820".to_string(),
            allowed_ips: vec!["10.100.2.0/24".to_string()],
            persistent_keepalive_secs: 25,
            rx_bytes: 50_000_000,
            tx_bytes: 40_000_000,
            last_handshake_secs: current_epoch_secs(),
            last_rotation_secs: current_epoch_secs(),
            quantum_safe: true,
        };

        reg.modify(&paths, |r| {
            if let Some(t) = r.tunnels.iter_mut().find(|t| t.tunnel_id == "craft-wg0") {
                t.peers.push(peer.clone());
            }
            r.status.active_peers = 3;
            Ok(())
        }).unwrap();

        let loaded = VpnRegistry::load(&paths).unwrap();
        assert_eq!(loaded.tunnels[0].peers.len(), 3);
        assert_eq!(loaded.tunnels[0].peers[2].peer_id, "cluster-node-2");
        assert!(loaded.tunnels[0].peers[2].quantum_safe);
    }

    #[test]
    fn test_vpn_renderers_output() {
        let summary = VpnStatusSummary::default();
        let status_text = render_vpn_status_text(&summary);
        assert!(status_text.contains("CRAFT QUANTUM-ENCRYPTED INTER-CLUSTER VPN MESH & WIREGUARD PQXDH STATUS"));
        assert!(status_text.contains("Active Tunnels"));
        assert!(status_text.contains("Quantum Defense Grade"));

        let tunnels = vec![VpnTunnelDescriptor {
            tunnel_id: "wg-cluster".to_string(),
            interface_name: "craft-wg1".to_string(),
            local_address: "10.200.1.1/24".to_string(),
            listen_port: 51821,
            crypto_mode: VpnCryptoMode::HardwareOffloadP4,
            peers: Vec::new(),
            rotation_policy: VpnKeyRotationPolicy::default(),
            mtu: 1420,
            state: VpnTunnelState::Active,
            created_at_secs: current_epoch_secs(),
        }];
        let tunnels_text = render_vpn_tunnels_text(&tunnels);
        assert!(tunnels_text.contains("CONFIGURED WIREGUARD PQXDH VPN TUNNELS"));
        assert!(tunnels_text.contains("wg-cluster"));

        let bench = VpnBenchmarkMetrics {
            packets_processed: 500_000,
            packet_size: 1420,
            throughput_gbps: 11.2,
            encryption_latency_nanos: 42,
            renegotiation_latency_micros: 68.5,
            packet_loss_percent: 0.0,
            key_rotations: 3,
        };
        let bench_text = render_vpn_bench_text(&bench);
        assert!(bench_text.contains("WIREGUARD PQXDH & P4 HARDWARE CRYPTO BENCHMARK RESULTS"));
        assert!(bench_text.contains("Line-Rate Throughput"));
        assert!(bench_text.contains("Key Renegotiation Latency"));
    }
}
