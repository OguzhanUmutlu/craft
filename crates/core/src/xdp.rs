use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::net::Ipv4Addr;
use std::path::Path;

/// XDP action return codes matching the Linux kernel xdp_action enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XdpAction {
    Aborted = 0,
    Drop = 1,
    Pass = 2,
    Tx = 3,
    Redirect = 4,
}

impl fmt::Display for XdpAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aborted => write!(f, "XDP_ABORTED"),
            Self::Drop => write!(f, "XDP_DROP"),
            Self::Pass => write!(f, "XDP_PASS"),
            Self::Tx => write!(f, "XDP_TX"),
            Self::Redirect => write!(f, "XDP_REDIRECT"),
        }
    }
}

impl std::str::FromStr for XdpAction {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "drop" | "xdp_drop" => Ok(Self::Drop),
            "pass" | "xdp_pass" => Ok(Self::Pass),
            "tx" | "xdp_tx" => Ok(Self::Tx),
            "redirect" | "xdp_redirect" => Ok(Self::Redirect),
            "aborted" | "xdp_aborted" => Ok(Self::Aborted),
            other => Err(format!("Unknown XDP action: {}", other)),
        }
    }
}

/// Attachment mode of the XDP program to network driver interface
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XdpAttachMode {
    Native,
    Generic,
    Offloaded,
    Detached,
}

impl fmt::Display for XdpAttachMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native => write!(f, "native (driver)"),
            Self::Generic => write!(f, "generic (skb)"),
            Self::Offloaded => write!(f, "offloaded (hardware)"),
            Self::Detached => write!(f, "detached"),
        }
    }
}

impl std::str::FromStr for XdpAttachMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "native" | "driver" => Ok(Self::Native),
            "generic" | "skb" => Ok(Self::Generic),
            "offload" | "offloaded" | "hardware" => Ok(Self::Offloaded),
            "detached" | "detach" => Ok(Self::Detached),
            other => Err(format!("Unknown XDP attach mode: {}", other)),
        }
    }
}

/// Network transport protocol for XDP flow tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XdpProtocol {
    Tcp,
    Udp,
    Icmp,
    Any,
}

impl fmt::Display for XdpProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp => write!(f, "TCP"),
            Self::Udp => write!(f, "UDP"),
            Self::Icmp => write!(f, "ICMP"),
            Self::Any => write!(f, "ANY"),
        }
    }
}

impl std::str::FromStr for XdpProtocol {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            "icmp" => Ok(Self::Icmp),
            "any" | "*" => Ok(Self::Any),
            other => Err(format!("Unknown transport protocol: {}", other)),
        }
    }
}

/// Filter rule evaluated in-kernel or by simulated XDP hook
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XdpFilterRule {
    pub id: String,
    pub cidr: String,
    pub port: Option<u16>,
    pub protocol: XdpProtocol,
    pub rate_limit_pps: Option<u64>,
    pub burst_tokens: Option<u64>,
    pub action: XdpAction,
    pub priority: u32,
    pub ban_ttl_seconds: Option<u64>,
    pub created_at: u64,
}

/// 5-tuple flow key identifying a bidirectional connection or packet stream
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct XdpFlowKey {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: XdpProtocol,
}

/// State of an active flow in the in-kernel BPF LRU map
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XdpFlowState {
    New,
    Established,
    FinWait,
    SynFloodBlocked,
    RakNetValidated,
    VolumetricBanned,
}

impl fmt::Display for XdpFlowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New => write!(f, "NEW"),
            Self::Established => write!(f, "ESTABLISHED"),
            Self::FinWait => write!(f, "FIN_WAIT"),
            Self::SynFloodBlocked => write!(f, "SYN_FLOOD_BLOCKED"),
            Self::RakNetValidated => write!(f, "RAKNET_VALIDATED"),
            Self::VolumetricBanned => write!(f, "VOLUMETRIC_BANNED"),
        }
    }
}

/// Entry stored in the BPF connection tracking table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XdpFlowEntry {
    pub key: XdpFlowKey,
    pub packets: u64,
    pub bytes: u64,
    pub tokens: f64,
    pub last_seen_epoch_secs: u64,
    pub state: XdpFlowState,
    pub banned_until_epoch_secs: Option<u64>,
}

/// BPF map memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XdpMapConfig {
    pub max_entries: usize,
    pub memory_budget_mb: usize,
}

impl Default for XdpMapConfig {
    fn default() -> Self {
        Self {
            max_entries: 500_000,
            memory_budget_mb: 64,
        }
    }
}

/// Instantaneous and cumulative XDP metrics summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XdpMetricsSummary {
    pub total_rx_packets: u64,
    pub total_rx_bytes: u64,
    pub passed_packets: u64,
    pub dropped_packets: u64,
    pub syn_flood_drops: u64,
    pub udp_flood_drops: u64,
    pub raknet_flood_drops: u64,
    pub current_active_flows: usize,
    pub drop_rate_pps: f64,
    pub bandwidth_absorbed_gbps: f64,
}

/// Full interface status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XdpInterfaceStatus {
    pub interface_name: String,
    pub mode: XdpAttachMode,
    pub attached: bool,
    pub bpf_prog_id: Option<u32>,
    pub rules_count: usize,
    pub active_flows: usize,
    pub metrics: XdpMetricsSummary,
}

/// Pure-Rust CIDR matcher without third-party dependencies
pub fn matches_cidr(ip_str: &str, cidr: &str) -> bool {
    let trimmed = cidr.trim();
    if trimmed == "any" || trimmed == "*" || trimmed == "0.0.0.0/0" || trimmed.is_empty() {
        return true;
    }
    if ip_str == trimmed {
        return true;
    }
    if let (Some((prefix_str, mask_str)), Ok(ip)) = (trimmed.split_once('/'), ip_str.parse::<Ipv4Addr>()) {
        if let (Ok(net_ip), Ok(prefix_len)) = (prefix_str.parse::<Ipv4Addr>(), mask_str.parse::<u32>()) {
            if prefix_len > 32 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let mask = !0u32 << (32 - prefix_len);
            let ip_u32 = u32::from(ip);
            let net_u32 = u32::from(net_ip);
            return (ip_u32 & mask) == (net_u32 & mask);
        }
    }
    false
}

/// Core XDP filtering, state tracking, and rate limiting engine
#[derive(Debug, Clone)]
pub struct XdpEngine {
    pub config: XdpMapConfig,
    pub rules: Vec<XdpFilterRule>,
    pub flows: HashMap<XdpFlowKey, XdpFlowEntry>,
    pub metrics: XdpMetricsSummary,
    pub banned_ips: HashMap<String, u64>,
}

impl XdpEngine {
    pub fn new(config: XdpMapConfig) -> Self {
        Self {
            config,
            rules: Vec::new(),
            flows: HashMap::new(),
            metrics: XdpMetricsSummary::default(),
            banned_ips: HashMap::new(),
        }
    }

    /// Add a filtering rule sorted by descending priority
    pub fn add_rule(&mut self, rule: XdpFilterRule) {
        self.rules.retain(|r| r.id != rule.id);
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove a rule by ID
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let initial_len = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() < initial_len
    }

    /// List active rules
    pub fn list_rules(&self) -> &[XdpFilterRule] {
        &self.rules
    }

    /// Clear all rules
    pub fn clear_rules(&mut self) {
        self.rules.clear();
    }

    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = XdpMetricsSummary::default();
        self.metrics.current_active_flows = self.flows.len();
    }

    /// Evaluate an ingress packet against active rules, flow tables, and rate limiters
    pub fn evaluate_packet(
        &mut self,
        key: &XdpFlowKey,
        packet_len: usize,
        is_syn: bool,
        is_raknet: bool,
        now_secs: u64,
    ) -> (XdpAction, Option<&'static str>) {
        self.metrics.total_rx_packets += 1;
        self.metrics.total_rx_bytes += packet_len as u64;

        // 1. Check temporary IP bans
        if let Some(&banned_until) = self.banned_ips.get(&key.src_ip) {
            if now_secs < banned_until {
                self.record_drop(is_syn, is_raknet, key.protocol);
                return (XdpAction::Drop, Some("ip_volumetric_banned"));
            } else {
                self.banned_ips.remove(&key.src_ip);
            }
        }

        // 2. Evaluate filter rules in priority order
        let mut matched_rule: Option<XdpFilterRule> = None;
        for rule in &self.rules {
            if rule.protocol != XdpProtocol::Any && rule.protocol != key.protocol {
                continue;
            }
            if let Some(port) = rule.port {
                if port != key.dst_port {
                    continue;
                }
            }
            if matches_cidr(&key.src_ip, &rule.cidr) {
                matched_rule = Some(rule.clone());
                break;
            }
        }

        // 3. Flow tracking and token bucket rate limiting
        let (action_opt, reason_opt, ban_opt) = {
            let entry = self.flows.entry(key.clone()).or_insert_with(|| XdpFlowEntry {
                key: key.clone(),
                packets: 0,
                bytes: 0,
                tokens: matched_rule
                    .as_ref()
                    .and_then(|r| r.burst_tokens)
                    .unwrap_or(1000) as f64,
                last_seen_epoch_secs: now_secs,
                state: if is_raknet {
                    XdpFlowState::RakNetValidated
                } else if is_syn {
                    XdpFlowState::New
                } else {
                    XdpFlowState::Established
                },
                banned_until_epoch_secs: None,
            });

            entry.packets += 1;
            entry.bytes += packet_len as u64;

            if let Some(ref rule) = matched_rule {
                if rule.action == XdpAction::Drop {
                    let ban = rule.ban_ttl_seconds.map(|ban_ttl| {
                        let until = now_secs + ban_ttl;
                        entry.state = XdpFlowState::VolumetricBanned;
                        entry.banned_until_epoch_secs = Some(until);
                        (key.src_ip.clone(), until)
                    });
                    (Some(XdpAction::Drop), Some("rule_match_drop"), ban)
                } else if let Some(limit_pps) = rule.rate_limit_pps {
                    let burst = rule.burst_tokens.unwrap_or(limit_pps * 2) as f64;
                    let elapsed = if now_secs >= entry.last_seen_epoch_secs {
                        (now_secs - entry.last_seen_epoch_secs) as f64
                    } else {
                        0.0
                    };
                    entry.last_seen_epoch_secs = now_secs;
                    entry.tokens = (entry.tokens + elapsed * limit_pps as f64).min(burst);

                    if entry.tokens >= 1.0 {
                        entry.tokens -= 1.0;
                        (None, None, None)
                    } else {
                        let ban = rule.ban_ttl_seconds.map(|ban_ttl| {
                            let until = now_secs + ban_ttl;
                            entry.state = XdpFlowState::VolumetricBanned;
                            entry.banned_until_epoch_secs = Some(until);
                            (key.src_ip.clone(), until)
                        });
                        (Some(XdpAction::Drop), Some("rate_limit_exceeded"), ban)
                    }
                } else {
                    (None, None, None)
                }
            } else {
                (None, None, None)
            }
        };

        if let Some((ip, until)) = ban_opt {
            self.banned_ips.insert(ip, until);
        }

        if let Some(action) = action_opt {
            self.record_drop(is_syn, is_raknet, key.protocol);
            return (action, reason_opt);
        }

        self.metrics.passed_packets += 1;
        self.metrics.current_active_flows = self.flows.len();
        (XdpAction::Pass, None)
    }

    fn record_drop(&mut self, is_syn: bool, is_raknet: bool, proto: XdpProtocol) {
        self.metrics.dropped_packets += 1;
        if is_syn {
            self.metrics.syn_flood_drops += 1;
        }
        if is_raknet {
            self.metrics.raknet_flood_drops += 1;
        }
        if proto == XdpProtocol::Udp {
            self.metrics.udp_flood_drops += 1;
        }
        self.metrics.current_active_flows = self.flows.len();
    }

    /// Prune inactive or expired flows when table reaches capacity
    pub fn prune_flows(&mut self, now_secs: u64, ttl_secs: u64) {
        self.flows.retain(|_, entry| {
            if now_secs >= entry.last_seen_epoch_secs {
                (now_secs - entry.last_seen_epoch_secs) <= ttl_secs
            } else {
                true
            }
        });
        self.banned_ips.retain(|_, &mut until| now_secs < until);
        self.metrics.current_active_flows = self.flows.len();
    }

    /// Retrieve copy of metrics summary
    pub fn get_metrics(&self) -> XdpMetricsSummary {
        self.metrics.clone()
    }

    /// Format plain-text status summary table (zero emojis)
    pub fn render_plain_status(&self, iface: &str, mode: XdpAttachMode) -> String {
        let mut out = String::new();
        out.push_str("=== Autonomous eBPF XDP Firewall Status ===\n");
        out.push_str(&format!("Interface:      {}\n", iface));
        out.push_str(&format!("Attachment:     {}\n", mode));
        out.push_str(&format!("Active Rules:   {}\n", self.rules.len()));
        out.push_str(&format!("Active Flows:   {}\n", self.flows.len()));
        out.push_str(&format!("Banned IPs:     {}\n", self.banned_ips.len()));
        out.push_str(&format!("Total Packets:  {}\n", self.metrics.total_rx_packets));
        out.push_str(&format!("Passed Packets: {}\n", self.metrics.passed_packets));
        out.push_str(&format!("Dropped Packets:{}\n", self.metrics.dropped_packets));
        out.push_str(&format!("SYN Drops:      {}\n", self.metrics.syn_flood_drops));
        out.push_str(&format!("UDP Drops:      {}\n", self.metrics.udp_flood_drops));
        out.push_str(&format!("RakNet Drops:   {}\n", self.metrics.raknet_flood_drops));
        out.push_str(&format!("Drop Rate:      {:.2} pps\n", self.metrics.drop_rate_pps));
        out.push_str(&format!("Absorbed BW:    {:.4} Gbps\n", self.metrics.bandwidth_absorbed_gbps));
        if !self.rules.is_empty() {
            out.push_str("\n--- Active Filter Rules ---\n");
            out.push_str(&format!("{:<16} {:<18} {:<8} {:<8} {:<12} {:<10}\n", "RULE ID", "CIDR", "PORT", "PROTO", "ACTION", "RATE (PPS)"));
            for r in &self.rules {
                let port_str = r.port.map(|p| p.to_string()).unwrap_or_else(|| "*".to_string());
                let rate_str = r.rate_limit_pps.map(|p| p.to_string()).unwrap_or_else(|| "unlimited".to_string());
                out.push_str(&format!("{:<16} {:<18} {:<8} {:<8} {:<12} {:<10}\n", r.id, r.cidr, port_str, r.protocol.to_string(), r.action.to_string(), rate_str));
            }
        }
        out
    }
}

/// Registry holding persistent XDP rules and state under ~/.craft/xdp/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XdpRegistry {
    pub rules: Vec<XdpFilterRule>,
    pub interface_name: String,
    pub mode: XdpAttachMode,
    pub config: XdpMapConfig,
}

impl Default for XdpRegistry {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            interface_name: "eth0".to_string(),
            mode: XdpAttachMode::Generic,
            config: XdpMapConfig::default(),
        }
    }
}

impl XdpRegistry {
    /// Load registry from file with advisory file locking
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let lock_path = &paths.xdp_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        let file_path = &paths.xdp_rules_file;
        if !file_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(file_path)?;
        let reg: Self = serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse XDP registry: {}", e)))?;
        Ok(reg)
    }

    /// Save registry to file with advisory file locking
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_path = &paths.xdp_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        if let Some(parent) = paths.xdp_rules_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize XDP registry: {}", e)))?;
        fs::write(&paths.xdp_rules_file, json)?;
        Ok(())
    }

    /// Add a rule and save
    pub fn add_rule(&mut self, paths: &CraftPaths, rule: XdpFilterRule) -> Result<()> {
        self.rules.retain(|r| r.id != rule.id);
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        self.save(paths)
    }

    /// Remove a rule by ID and save
    pub fn remove_rule(&mut self, paths: &CraftPaths, rule_id: &str) -> Result<bool> {
        let initial_len = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        let removed = self.rules.len() < initial_len;
        if removed {
            self.save(paths)?;
        }
        Ok(removed)
    }

    /// Record live interface state into state.json
    pub fn record_state(&self, paths: &CraftPaths, status: &XdpInterfaceStatus) -> Result<()> {
        let lock_path = &paths.xdp_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        if let Some(parent) = paths.xdp_state_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(status)
            .map_err(|e| CraftError::Config(format!("Failed to serialize XDP state: {}", e)))?;
        fs::write(&paths.xdp_state_file, json)?;
        Ok(())
    }
}

/// Advisory lock guard for XDP operations
struct LockGuard {
    _file: File,
}

impl LockGuard {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire xdp.lock: {}", e)))?;
        Ok(Self { _file: file })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_cidr() {
        assert!(matches_cidr("192.168.1.50", "192.168.1.0/24"));
        assert!(!matches_cidr("192.168.2.50", "192.168.1.0/24"));
        assert!(matches_cidr("10.0.0.1", "10.0.0.1/32"));
        assert!(!matches_cidr("10.0.0.2", "10.0.0.1/32"));
        assert!(matches_cidr("172.16.5.99", "any"));
        assert!(matches_cidr("172.16.5.99", "*"));
    }

    #[test]
    fn test_xdp_engine_rate_limiting() {
        let mut engine = XdpEngine::new(XdpMapConfig::default());
        let rule = XdpFilterRule {
            id: "limit-udp".to_string(),
            cidr: "any".to_string(),
            port: Some(25565),
            protocol: XdpProtocol::Udp,
            rate_limit_pps: Some(5),
            burst_tokens: Some(5),
            action: XdpAction::Pass,
            priority: 10,
            ban_ttl_seconds: Some(60),
            created_at: 1000,
        };
        engine.add_rule(rule);

        let key = XdpFlowKey {
            src_ip: "1.2.3.4".to_string(),
            dst_ip: "10.0.0.1".to_string(),
            src_port: 43210,
            dst_port: 25565,
            protocol: XdpProtocol::Udp,
        };

        // First 5 packets should pass (burst = 5)
        for _ in 0..5 {
            let (action, _) = engine.evaluate_packet(&key, 1000, false, false, 1000);
            assert_eq!(action, XdpAction::Pass);
        }

        // 6th packet exceeds rate limit and triggers temporary ban
        let (action, reason) = engine.evaluate_packet(&key, 1000, false, false, 1000);
        assert_eq!(action, XdpAction::Drop);
        assert_eq!(reason, Some("rate_limit_exceeded"));

        // Subsequent packets from same IP are dropped due to ban
        let (action, reason) = engine.evaluate_packet(&key, 1000, false, false, 1010);
        assert_eq!(action, XdpAction::Drop);
        assert_eq!(reason, Some("ip_volumetric_banned"));
    }
}
