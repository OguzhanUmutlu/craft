use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::net::Ipv4Addr;
use std::str::FromStr;

/// Network security isolation zone
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IsolationZone {
    /// Public entry points (e.g. Velocity, BungeeCord, HAProxy)
    IngressProxy,
    /// Protected game runtime instances (e.g. Paper, Fabric, Purpur)
    BackendWorld,
    /// Storage and backup mesh synchronization nodes
    StorageMesh,
    /// Management supervisor, daemon IPC, telemetry, and SSH federation
    ControlPlane,
}

impl IsolationZone {
    pub fn as_str(&self) -> &'static str {
        match self {
            IsolationZone::IngressProxy => "ingress_proxy",
            IsolationZone::BackendWorld => "backend_world",
            IsolationZone::StorageMesh => "storage_mesh",
            IsolationZone::ControlPlane => "control_plane",
        }
    }
}

impl fmt::Display for IsolationZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for IsolationZone {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('-', "_").as_str() {
            "ingress" | "proxy" | "ingress_proxy" | "ingressproxy" => Ok(IsolationZone::IngressProxy),
            "backend" | "world" | "backend_world" | "backendworld" | "server" => Ok(IsolationZone::BackendWorld),
            "storage" | "storage_mesh" | "storagemesh" | "backup" => Ok(IsolationZone::StorageMesh),
            "control" | "control_plane" | "controlplane" | "daemon" | "admin" => Ok(IsolationZone::ControlPlane),
            _ => Err(format!(
                "Unknown isolation zone '{}'. Valid zones: ingress_proxy, backend_world, storage_mesh, control_plane",
                s
            )),
        }
    }
}

/// Network transport protocol for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilterProtocol {
    Tcp,
    Udp,
    Both,
    Any,
}

impl FilterProtocol {
    pub fn matches(&self, other: FilterProtocol) -> bool {
        match (*self, other) {
            (FilterProtocol::Any, _) | (_, FilterProtocol::Any) => true,
            (FilterProtocol::Both, _) | (_, FilterProtocol::Both) => true,
            (a, b) => a == b,
        }
    }
}

impl fmt::Display for FilterProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterProtocol::Tcp => write!(f, "TCP"),
            FilterProtocol::Udp => write!(f, "UDP"),
            FilterProtocol::Both => write!(f, "TCP/UDP"),
            FilterProtocol::Any => write!(f, "ANY"),
        }
    }
}

impl FromStr for FilterProtocol {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "tcp" => Ok(FilterProtocol::Tcp),
            "udp" => Ok(FilterProtocol::Udp),
            "both" | "tcp/udp" | "all" => Ok(FilterProtocol::Both),
            "any" | "*" => Ok(FilterProtocol::Any),
            _ => Err(format!("Unknown protocol '{}'. Valid: tcp, udp, both, any", s)),
        }
    }
}

/// Packet verdict action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilterAction {
    Pass,
    Drop,
    Reject,
}

impl fmt::Display for FilterAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterAction::Pass => write!(f, "PASS"),
            FilterAction::Drop => write!(f, "DROP"),
            FilterAction::Reject => write!(f, "REJECT"),
        }
    }
}

impl FromStr for FilterAction {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "pass" | "allow" | "accept" => Ok(FilterAction::Pass),
            "drop" | "deny" => Ok(FilterAction::Drop),
            "reject" => Ok(FilterAction::Reject),
            _ => Err(format!("Unknown filter action '{}'. Valid: pass, drop, reject", s)),
        }
    }
}

/// Granular L4/L7 microsegmentation packet filter rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrosegmentationRule {
    pub rule_id: String,
    pub description: String,
    pub source_zone: IsolationZone,
    pub target_zone: IsolationZone,
    pub protocol: FilterProtocol,
    pub ports: Vec<u16>,
    #[serde(default)]
    pub rate_limit_pps: Option<u32>,
    pub action: FilterAction,
}

/// Comprehensive microsegmentation security policy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrosegmentationPolicy {
    pub name: String,
    pub default_action: FilterAction,
    pub rules: Vec<MicrosegmentationRule>,
}

impl Default for MicrosegmentationPolicy {
    fn default() -> Self {
        Self::default_game_cluster_policy()
    }
}

impl MicrosegmentationPolicy {
    /// Creates a hardened zero-trust baseline policy for distributed game server clusters
    pub fn default_game_cluster_policy() -> Self {
        Self {
            name: "zero-trust-cluster-baseline".to_string(),
            default_action: FilterAction::Drop,
            rules: vec![
                MicrosegmentationRule {
                    rule_id: "rule-proxy-to-backend-java".to_string(),
                    description: "Allow proxy routing to backend Java game servers".to_string(),
                    source_zone: IsolationZone::IngressProxy,
                    target_zone: IsolationZone::BackendWorld,
                    protocol: FilterProtocol::Tcp,
                    ports: vec![25565, 25566, 25567, 25568, 25569, 25570],
                    rate_limit_pps: Some(50000),
                    action: FilterAction::Pass,
                },
                MicrosegmentationRule {
                    rule_id: "rule-proxy-to-backend-bedrock".to_string(),
                    description: "Allow proxy routing to backend Bedrock instances".to_string(),
                    source_zone: IsolationZone::IngressProxy,
                    target_zone: IsolationZone::BackendWorld,
                    protocol: FilterProtocol::Udp,
                    ports: vec![19132, 19133, 19134, 19135],
                    rate_limit_pps: Some(50000),
                    action: FilterAction::Pass,
                },
                MicrosegmentationRule {
                    rule_id: "rule-control-to-all".to_string(),
                    description: "Allow control plane daemon IPC and administrative discovery".to_string(),
                    source_zone: IsolationZone::ControlPlane,
                    target_zone: IsolationZone::BackendWorld,
                    protocol: FilterProtocol::Both,
                    ports: vec![25575, 8000, 9090], // RCON, Metrics, HTTP
                    rate_limit_pps: None,
                    action: FilterAction::Pass,
                },
                MicrosegmentationRule {
                    rule_id: "rule-control-to-proxy".to_string(),
                    description: "Allow control plane to manage ingress proxy nodes".to_string(),
                    source_zone: IsolationZone::ControlPlane,
                    target_zone: IsolationZone::IngressProxy,
                    protocol: FilterProtocol::Tcp,
                    ports: vec![8000, 9090],
                    rate_limit_pps: None,
                    action: FilterAction::Pass,
                },
                MicrosegmentationRule {
                    rule_id: "rule-storage-mesh".to_string(),
                    description: "Allow storage mesh cluster synchronization".to_string(),
                    source_zone: IsolationZone::StorageMesh,
                    target_zone: IsolationZone::StorageMesh,
                    protocol: FilterProtocol::Tcp,
                    ports: vec![9000, 9001],
                    rate_limit_pps: None,
                    action: FilterAction::Pass,
                },
                MicrosegmentationRule {
                    rule_id: "rule-backend-to-storage".to_string(),
                    description: "Allow backend worlds to ship backups to storage mesh".to_string(),
                    source_zone: IsolationZone::BackendWorld,
                    target_zone: IsolationZone::StorageMesh,
                    protocol: FilterProtocol::Tcp,
                    ports: vec![9000, 9001],
                    rate_limit_pps: None,
                    action: FilterAction::Pass,
                },
            ],
        }
    }

    /// Evaluates a packet flow through the microsegmentation policy
    pub fn evaluate_flow(
        &self,
        src_zone: IsolationZone,
        dst_zone: IsolationZone,
        proto: FilterProtocol,
        dst_port: u16,
    ) -> (FilterAction, Option<String>) {
        for rule in &self.rules {
            if rule.source_zone == src_zone
                && rule.target_zone == dst_zone
                && rule.protocol.matches(proto)
                && (rule.ports.is_empty() || rule.ports.contains(&dst_port))
            {
                return (rule.action, Some(rule.rule_id.clone()));
            }
        }
        (self.default_action, None)
    }
}

/// WireGuard overlay network peer node descriptor
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireguardPeer {
    pub node_id: String,
    pub name: String,
    pub public_key: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub tunnel_ip: String,
    pub allowed_ips: Vec<String>,
    pub zone: IsolationZone,
    #[serde(default = "default_keepalive")]
    pub keepalive_seconds: u16,
    #[serde(default)]
    pub last_handshake_epoch: Option<u64>,
    #[serde(default)]
    pub transfer_rx_bytes: u64,
    #[serde(default)]
    pub transfer_tx_bytes: u64,
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_keepalive() -> u16 {
    25
}

fn default_active() -> bool {
    true
}

/// Local node WireGuard cryptographic and network interface identity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNodeConfig {
    pub node_id: String,
    pub name: String,
    pub private_key: String,
    pub public_key: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    pub tunnel_ip: String,
    pub zone: IsolationZone,
}

fn default_listen_port() -> u16 {
    51820
}

/// Top-level SDN overlay mesh and microsegmentation configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdnMeshConfig {
    pub mesh_name: String,
    #[serde(default = "default_overlay_cidr")]
    pub overlay_cidr: String,
    pub local_node: LocalNodeConfig,
    #[serde(default)]
    pub peers: Vec<WireguardPeer>,
    #[serde(default)]
    pub policy: MicrosegmentationPolicy,
    #[serde(default = "default_true")]
    pub mtls_enabled: bool,
    #[serde(default)]
    pub ca_cert_pem: Option<String>,
    #[serde(default)]
    pub node_cert_pem: Option<String>,
    #[serde(default)]
    pub node_key_pem: Option<String>,
    #[serde(default)]
    pub cert_expires_epoch: Option<u64>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_overlay_cidr() -> String {
    "10.42.0.0/16".to_string()
}

fn default_true() -> bool {
    true
}

/// Thread-safe and inter-process file-locked registry managing SDN overlay configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdnRegistry {
    pub mesh: SdnMeshConfig,
}

impl SdnRegistry {
    pub fn new(local_name: &str, zone: IsolationZone) -> Self {
        // Deterministic fallback initial keys
        let dummy_priv = "wN4W3x9sL9E3j7r1Q8Z2v5Y4u1T0s8P7o6I5u4Y3t2E=";
        let dummy_pub = "xT6V5z1uN1G5l9t3S0B4x7A6w3V2u0R9q8K7w6A5v4G=";

        Self {
            mesh: SdnMeshConfig {
                mesh_name: "craft-sdn-mesh".to_string(),
                overlay_cidr: "10.42.0.0/16".to_string(),
                local_node: LocalNodeConfig {
                    node_id: "local-node".to_string(),
                    name: local_name.to_string(),
                    private_key: dummy_priv.to_string(),
                    public_key: dummy_pub.to_string(),
                    listen_port: 51820,
                    tunnel_ip: "10.42.0.1/16".to_string(),
                    zone,
                },
                peers: Vec::new(),
                policy: MicrosegmentationPolicy::default(),
                mtls_enabled: true,
                ca_cert_pem: None,
                node_cert_pem: None,
                node_key_pem: None,
                cert_expires_epoch: None,
                metadata: HashMap::new(),
            },
        }
    }

    /// Loads the SDN registry from disk with advisory file locking
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if let Some(parent) = paths.sdn_lock.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.sdn_lock)?;
        lock_file.lock_exclusive()?;

        let registry = if paths.sdn_mesh_file.exists() {
            let contents = fs::read_to_string(&paths.sdn_mesh_file)?;
            toml::from_str(&contents).map_err(|e| CraftError::Config(format!("Failed to parse sdn mesh.toml: {}", e)))?
        } else {
            Self::new("gateway-1", IsolationZone::ControlPlane)
        };

        let _ = lock_file.unlock();
        Ok(registry)
    }

    /// Saves the SDN registry atomically to disk with advisory file locking
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.sdn_lock.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.sdn_lock)?;
        lock_file.lock_exclusive()?;

        if let Some(parent) = paths.sdn_mesh_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize sdn mesh.toml: {}", e)))?;

        let temp_file = paths.sdn_dir.join("mesh.toml.tmp");
        fs::write(&temp_file, toml_str)?;
        fs::rename(temp_file, &paths.sdn_mesh_file)?;

        let _ = lock_file.unlock();
        Ok(())
    }

    /// Registers or updates a WireGuard peer
    pub fn add_peer(&mut self, peer: WireguardPeer) {
        if let Some(existing) = self.mesh.peers.iter_mut().find(|p| p.node_id == peer.node_id) {
            *existing = peer;
        } else {
            self.mesh.peers.push(peer);
        }
    }

    /// Removes a WireGuard peer by node_id
    pub fn remove_peer(&mut self, node_id: &str) -> bool {
        let initial_len = self.mesh.peers.len();
        self.mesh.peers.retain(|p| p.node_id != node_id);
        self.mesh.peers.len() < initial_len
    }

    /// Retrieves a peer reference by node_id
    pub fn get_peer(&self, node_id: &str) -> Option<&WireguardPeer> {
        self.mesh.peers.iter().find(|p| p.node_id == node_id)
    }

    /// Retrieves all peers in a given isolation zone
    pub fn get_peers_in_zone(&self, zone: IsolationZone) -> Vec<&WireguardPeer> {
        self.mesh.peers.iter().filter(|p| p.zone == zone).collect()
    }

    /// Allocates the next available sequential IPv4 address in the overlay CIDR (e.g. 10.42.0.2/32, 10.42.0.3/32)
    pub fn allocate_next_tunnel_ip(&self) -> Result<String> {
        let base_ip_str = self
            .mesh
            .overlay_cidr
            .split('/')
            .next()
            .ok_or_else(|| CraftError::Config("Invalid overlay CIDR format".to_string()))?;
        let base_ip: Ipv4Addr = base_ip_str
            .parse()
            .map_err(|e| CraftError::Config(format!("Invalid IPv4 in overlay CIDR: {}", e)))?;
        let base_u32 = u32::from(base_ip);

        // Collect all currently assigned IPs
        let mut used_ips = Vec::new();
        if let Some(local_ip) = self.mesh.local_node.tunnel_ip.split('/').next() {
            if let Ok(parsed) = local_ip.parse::<Ipv4Addr>() {
                used_ips.push(u32::from(parsed));
            }
        }
        for peer in &self.mesh.peers {
            if let Some(peer_ip) = peer.tunnel_ip.split('/').next() {
                if let Ok(parsed) = peer_ip.parse::<Ipv4Addr>() {
                    used_ips.push(u32::from(parsed));
                }
            }
        }

        // Search for next unassigned address starting from .2
        for offset in 2..65534u32 {
            let candidate_u32 = base_u32 + offset;
            if !used_ips.contains(&candidate_u32) {
                let candidate_ip = Ipv4Addr::from(candidate_u32);
                return Ok(format!("{}/32", candidate_ip));
            }
        }

        Err(CraftError::Other("Overlay IP space exhausted in CIDR".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_zone_parsing() {
        assert_eq!(IsolationZone::from_str("ingress").unwrap(), IsolationZone::IngressProxy);
        assert_eq!(IsolationZone::from_str("backend").unwrap(), IsolationZone::BackendWorld);
        assert_eq!(IsolationZone::from_str("storage").unwrap(), IsolationZone::StorageMesh);
        assert_eq!(IsolationZone::from_str("control").unwrap(), IsolationZone::ControlPlane);
        assert!(IsolationZone::from_str("unknown_zone").is_err());
    }

    #[test]
    fn test_microsegmentation_policy_evaluation() {
        let policy = MicrosegmentationPolicy::default_game_cluster_policy();

        // Ingress proxy to backend on port 25565 -> PASS
        let (action, rule) = policy.evaluate_flow(
            IsolationZone::IngressProxy,
            IsolationZone::BackendWorld,
            FilterProtocol::Tcp,
            25565,
        );
        assert_eq!(action, FilterAction::Pass);
        assert_eq!(rule.as_deref(), Some("rule-proxy-to-backend-java"));

        // Backend to backend direct flow -> DROP (zero-trust lateral movement block)
        let (action, rule) = policy.evaluate_flow(
            IsolationZone::BackendWorld,
            IsolationZone::BackendWorld,
            FilterProtocol::Tcp,
            25565,
        );
        assert_eq!(action, FilterAction::Drop);
        assert!(rule.is_none());

        // Ingress proxy to backend on unauthorized port (e.g. 22 SSH) -> DROP
        let (action, _) = policy.evaluate_flow(
            IsolationZone::IngressProxy,
            IsolationZone::BackendWorld,
            FilterProtocol::Tcp,
            22,
        );
        assert_eq!(action, FilterAction::Drop);
    }

    #[test]
    fn test_tunnel_ip_allocation() {
        let mut registry = SdnRegistry::new("hub-1", IsolationZone::ControlPlane);
        registry.mesh.local_node.tunnel_ip = "10.42.0.1/16".to_string();

        let ip1 = registry.allocate_next_tunnel_ip().unwrap();
        assert_eq!(ip1, "10.42.0.2/32");

        registry.add_peer(WireguardPeer {
            node_id: "peer-1".to_string(),
            name: "lobby-1".to_string(),
            public_key: "abc".to_string(),
            endpoint: Some("192.168.1.50:51820".to_string()),
            tunnel_ip: ip1,
            allowed_ips: vec!["10.42.0.2/32".to_string()],
            zone: IsolationZone::BackendWorld,
            keepalive_seconds: 25,
            last_handshake_epoch: None,
            transfer_rx_bytes: 0,
            transfer_tx_bytes: 0,
            is_active: true,
        });

        let ip2 = registry.allocate_next_tunnel_ip().unwrap();
        assert_eq!(ip2, "10.42.0.3/32");
    }

    #[test]
    fn test_sdn_registry_save_load_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());

        let mut registry = SdnRegistry::new("edge-node-1", IsolationZone::IngressProxy);
        registry.add_peer(WireguardPeer {
            node_id: "node-backend-1".to_string(),
            name: "survival-main".to_string(),
            public_key: "test_key_base64".to_string(),
            endpoint: Some("10.0.0.5:51820".to_string()),
            tunnel_ip: "10.42.0.2/32".to_string(),
            allowed_ips: vec!["10.42.0.2/32".to_string()],
            zone: IsolationZone::BackendWorld,
            keepalive_seconds: 25,
            last_handshake_epoch: Some(1700000000),
            transfer_rx_bytes: 1024,
            transfer_tx_bytes: 2048,
            is_active: true,
        });

        registry.save(&paths).unwrap();

        let loaded = SdnRegistry::load(&paths).unwrap();
        assert_eq!(loaded.mesh.mesh_name, "craft-sdn-mesh");
        assert_eq!(loaded.mesh.local_node.zone, IsolationZone::IngressProxy);
        assert_eq!(loaded.mesh.peers.len(), 1);
        assert_eq!(loaded.mesh.peers[0].name, "survival-main");
    }
}
