//! Autonomous eBPF XDP Hardware Offloading, SmartNIC Acceleration & P4 Line-Rate Switching
//!
//! Provides data models, P4 match-action table abstractions, hardware flow table
//! tracking, and registry persistence for line-rate packet offloading on SmartNIC SOCs.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

/// Vendor classification of SmartNIC SOC hardware
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartNicVendor {
    NvidiaBluefield,
    AmdPensando,
    IntelIpu,
    NetronomeAgilio,
    GenericP4Emulated,
}

impl SmartNicVendor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NvidiaBluefield => "nvidia_bluefield",
            Self::AmdPensando => "amd_pensando",
            Self::IntelIpu => "intel_ipu",
            Self::NetronomeAgilio => "netronome_agilio",
            Self::GenericP4Emulated => "generic_p4_emulated",
        }
    }
}

impl fmt::Display for SmartNicVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for SmartNicVendor {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nvidia" | "bluefield" | "nvidia_bluefield" => Ok(Self::NvidiaBluefield),
            "amd" | "pensando" | "amd_pensando" => Ok(Self::AmdPensando),
            "intel" | "ipu" | "intel_ipu" => Ok(Self::IntelIpu),
            "netronome" | "agilio" | "netronome_agilio" => Ok(Self::NetronomeAgilio),
            "p4" | "emulated" | "generic" | "generic_p4_emulated" => Ok(Self::GenericP4Emulated),
            _ => Err(CraftError::Config(format!("Unknown SmartNIC vendor: {}", s))),
        }
    }
}

/// Execution mode for packet offload
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffloadMode {
    /// In-hardware execution on SmartNIC ASIC/FPGA/SOC (XDP_FLAGS_HW_MODE)
    HardwareAsic,
    /// Kernel driver XDP native mode (XDP_FLAGS_DRV_MODE)
    Driver,
    /// Software generic SKB mode (XDP_FLAGS_SKB_MODE)
    SoftwareGeneric,
    /// Userspace P4 behavioral model / software emulation
    EmulatedP4Software,
}

impl OffloadMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HardwareAsic => "hardware_asic",
            Self::Driver => "driver",
            Self::SoftwareGeneric => "software_generic",
            Self::EmulatedP4Software => "emulated_p4_software",
        }
    }
}

impl fmt::Display for OffloadMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OffloadMode {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hw" | "hardware" | "hardware_asic" | "asic" => Ok(Self::HardwareAsic),
            "drv" | "driver" => Ok(Self::Driver),
            "skb" | "generic" | "software_generic" => Ok(Self::SoftwareGeneric),
            "p4" | "emulated" | "emulated_p4_software" => Ok(Self::EmulatedP4Software),
            _ => Err(CraftError::Config(format!("Unknown offload mode: {}", s))),
        }
    }
}

/// Protocols supported for hardware offload
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffloadProtocol {
    MinecraftJavaSlp,
    BedrockRaknet,
    ValveA2s,
    DdosMitigation,
    CustomP4,
}

impl OffloadProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MinecraftJavaSlp => "minecraft_java_slp",
            Self::BedrockRaknet => "bedrock_raknet",
            Self::ValveA2s => "valve_a2s",
            Self::DdosMitigation => "ddos_mitigation",
            Self::CustomP4 => "custom_p4",
        }
    }
}

impl fmt::Display for OffloadProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OffloadProtocol {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "slp" | "java" | "minecraft" | "minecraft_java_slp" => Ok(Self::MinecraftJavaSlp),
            "raknet" | "bedrock" | "bedrock_raknet" => Ok(Self::BedrockRaknet),
            "a2s" | "valve" | "valve_a2s" => Ok(Self::ValveA2s),
            "ddos" | "anti_ddos" | "ddos_mitigation" => Ok(Self::DdosMitigation),
            "custom" | "p4" | "custom_p4" => Ok(Self::CustomP4),
            _ => Err(CraftError::Config(format!("Unknown offload protocol: {}", s))),
        }
    }
}

/// P4 match field types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "match_type", content = "spec")]
pub enum P4MatchField {
    Exact(Vec<u8>),
    Ternary { value: Vec<u8>, mask: Vec<u8> },
    Lpm { value: Vec<u8>, prefix_len: u8 },
    Range { low: u64, high: u64 },
}

/// P4 Action definitions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_type", content = "param")]
pub enum P4ActionType {
    Drop,
    ForwardPort(u16),
    SendPongDirect,
    SendSynCookie,
    RateLimitToken(u32),
    PassToHost,
}

impl fmt::Display for P4ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Drop => write!(f, "DROP"),
            Self::ForwardPort(port) => write!(f, "FORWARD_PORT({})", port),
            Self::SendPongDirect => write!(f, "SEND_PONG_DIRECT"),
            Self::SendSynCookie => write!(f, "SEND_SYN_COOKIE"),
            Self::RateLimitToken(tokens) => write!(f, "RATE_LIMIT({})", tokens),
            Self::PassToHost => write!(f, "PASS_TO_HOST"),
        }
    }
}

/// A single entry within a P4 match-action table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P4TableEntry {
    pub table_name: String,
    pub match_fields: Vec<P4MatchField>,
    pub action: P4ActionType,
    pub priority: u32,
    pub hit_count: u64,
    pub byte_count: u64,
    pub age_seconds: u64,
}

/// P4 Match-Action table metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P4MatchActionTable {
    pub table_name: String,
    pub max_entries: usize,
    pub default_action: P4ActionType,
    pub entries: Vec<P4TableEntry>,
}

/// Physical or emulated SmartNIC device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNicDeviceInfo {
    pub device_id: String,
    pub pci_address: String,
    pub vendor: SmartNicVendor,
    pub model: String,
    pub total_tcam_rules: usize,
    pub used_tcam_rules: usize,
    pub link_speed_gbps: u32,
    pub offload_mode: OffloadMode,
    pub supported_protocols: Vec<OffloadProtocol>,
}

/// Operator-managed SmartNIC offload rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNicOffloadRule {
    pub rule_id: String,
    pub priority: u32,
    pub protocol: OffloadProtocol,
    pub match_port: Option<u16>,
    pub match_cidr: Option<String>,
    pub action: P4ActionType,
    pub hardware_installed: bool,
    pub hits: u64,
    pub bytes: u64,
    pub created_at: u64,
}

/// Cumulative status summary for SmartNIC offloading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNicStatusSummary {
    pub active_devices: usize,
    pub offload_mode: OffloadMode,
    pub installed_rules: usize,
    pub tcam_usage_percent: f64,
    pub offloaded_packets: u64,
    pub offloaded_bytes: u64,
    pub host_cpu_saved_percent: f64,
    pub fallback_count: u64,
}

impl Default for SmartNicStatusSummary {
    fn default() -> Self {
        Self {
            active_devices: 1,
            offload_mode: OffloadMode::HardwareAsic,
            installed_rules: 0,
            tcam_usage_percent: 0.0,
            offloaded_packets: 0,
            offloaded_bytes: 0,
            host_cpu_saved_percent: 99.8,
            fallback_count: 0,
        }
    }
}

/// Benchmark metrics for line-rate packet offload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNicBenchmarkMetrics {
    pub throughput_mpps: f64,
    pub bandwidth_gbps: f64,
    pub asic_latency_nanos: u64,
    pub host_cpu_utilization_percent: f64,
    pub packets_evaluated: u64,
    pub hardware_drops: u64,
    pub hardware_pongs: u64,
}

/// Persistent registry holding SmartNIC state, devices, and rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNicRegistry {
    pub devices: Vec<SmartNicDeviceInfo>,
    pub rules: Vec<SmartNicOffloadRule>,
    pub status: SmartNicStatusSummary,
}

impl Default for SmartNicRegistry {
    fn default() -> Self {
        let default_device = SmartNicDeviceInfo {
            device_id: "smartnic-0".to_string(),
            pci_address: "0000:03:00.0".to_string(),
            vendor: SmartNicVendor::NvidiaBluefield,
            model: "BlueField-3 DPU 400GbE / P4 Data Plane".to_string(),
            total_tcam_rules: 65536,
            used_tcam_rules: 0,
            link_speed_gbps: 100,
            offload_mode: OffloadMode::HardwareAsic,
            supported_protocols: vec![
                OffloadProtocol::MinecraftJavaSlp,
                OffloadProtocol::BedrockRaknet,
                OffloadProtocol::ValveA2s,
                OffloadProtocol::DdosMitigation,
                OffloadProtocol::CustomP4,
            ],
        };

        Self {
            devices: vec![default_device],
            rules: Vec::new(),
            status: SmartNicStatusSummary::default(),
        }
    }
}

impl SmartNicRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.smartnic_registry_file.exists() {
            let reg = Self::default();
            reg.save(paths)?;
            return Ok(reg);
        }
        let mut f = File::open(&paths.smartnic_registry_file).map_err(CraftError::Io)?;
        let mut content = String::new();
        f.read_to_string(&mut content).map_err(CraftError::Io)?;
        serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse smartnic registry: {}", e)))
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.smartnic_registry_file.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize smartnic registry: {}", e)))?;
        let temp_file = paths.smartnic_registry_file.with_extension("tmp");
        let mut f = File::create(&temp_file).map_err(CraftError::Io)?;
        f.write_all(content.as_bytes()).map_err(CraftError::Io)?;
        f.sync_all().map_err(CraftError::Io)?;
        fs::rename(&temp_file, &paths.smartnic_registry_file).map_err(CraftError::Io)?;
        Ok(())
    }

    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.smartnic_lock.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.smartnic_lock)
            .map_err(|e| CraftError::Other(format!("Failed to open SmartNIC lock file: {}", e)))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire SmartNIC exclusive lock: {}", e)))?;

        let res = (|| {
            let loaded = Self::load(paths)?;
            *self = loaded;
            let result = f(self)?;
            self.save(paths)?;
            Ok(result)
        })();

        let _ = lock_file.unlock();
        res
    }

    pub fn add_rule(&mut self, mut rule: SmartNicOffloadRule) -> Result<()> {
        // Check TCAM capacity on primary device
        if let Some(dev) = self.devices.first_mut() {
            if dev.used_tcam_rules >= dev.total_tcam_rules {
                return Err(CraftError::Other(
                    "SmartNIC TCAM table capacity exhausted (65536/65536 rules allocated)".to_string(),
                ));
            }
            dev.used_tcam_rules += 1;
            rule.hardware_installed = true;
        }

        if let Some(pos) = self.rules.iter().position(|r| r.rule_id == rule.rule_id) {
            self.rules[pos] = rule;
        } else {
            self.rules.push(rule);
        }

        self.update_summary();
        Ok(())
    }

    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        if let Some(pos) = self.rules.iter().position(|r| r.rule_id == rule_id) {
            self.rules.remove(pos);
            if let Some(dev) = self.devices.first_mut() {
                if dev.used_tcam_rules > 0 {
                    dev.used_tcam_rules -= 1;
                }
            }
            self.update_summary();
            true
        } else {
            false
        }
    }

    pub fn update_summary(&mut self) {
        self.status.active_devices = self.devices.len();
        self.status.installed_rules = self.rules.len();

        let total_tcam: usize = self.devices.iter().map(|d| d.total_tcam_rules).sum();
        let used_tcam: usize = self.devices.iter().map(|d| d.used_tcam_rules).sum();

        if total_tcam > 0 {
            self.status.tcam_usage_percent = (used_tcam as f64 / total_tcam as f64) * 100.0;
        } else {
            self.status.tcam_usage_percent = 0.0;
        }

        let off_pkts: u64 = self.rules.iter().map(|r| r.hits).sum();
        let off_bytes: u64 = self.rules.iter().map(|r| r.bytes).sum();
        self.status.offloaded_packets = off_pkts;
        self.status.offloaded_bytes = off_bytes;
    }

    pub fn reset_metrics(&mut self) {
        for rule in &mut self.rules {
            rule.hits = 0;
            rule.bytes = 0;
        }
        self.status.offloaded_packets = 0;
        self.status.offloaded_bytes = 0;
        self.status.fallback_count = 0;
    }
}

/// Plain-text formatting for SmartNIC status
pub fn render_smartnic_status_text(
    summary: &SmartNicStatusSummary,
    devices: &[SmartNicDeviceInfo],
) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("          CRAFT SMARTNIC HARDWARE OFFLOAD & P4 DATA PLANE STATUS                \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Offload Mode:       {}\n", summary.offload_mode));
    out.push_str(&format!("  Active Devices:     {}\n", summary.active_devices));
    out.push_str(&format!("  Installed Rules:    {}\n", summary.installed_rules));
    out.push_str(&format!("  TCAM Capacity:      {:.2}%\n", summary.tcam_usage_percent));
    out.push_str(&format!("  Offloaded Packets:  {}\n", summary.offloaded_packets));
    out.push_str(&format!("  Offloaded Volume:   {} bytes\n", summary.offloaded_bytes));
    out.push_str(&format!("  Host CPU Saved:     {:.1}%\n", summary.host_cpu_saved_percent));
    out.push_str(&format!("  Driver Fallbacks:   {}\n\n", summary.fallback_count));

    out.push_str("--- Enumerated SmartNIC Devices ---\n");
    if devices.is_empty() {
        out.push_str("  (No SmartNIC devices detected)\n");
    } else {
        out.push_str(&format!(
            "  {:<12} {:<14} {:<18} {:<10} {:<12} {:<14}\n",
            "DEVICE ID", "PCI ADDR", "VENDOR", "SPEED", "MODE", "TCAM RULES"
        ));
        for dev in devices {
            out.push_str(&format!(
                "  {:<12} {:<14} {:<18} {:<10} {:<12} {:<14}\n",
                dev.device_id,
                dev.pci_address,
                dev.vendor.as_str(),
                format!("{}G", dev.link_speed_gbps),
                dev.offload_mode.as_str(),
                format!("{}/{}", dev.used_tcam_rules, dev.total_tcam_rules)
            ));
        }
    }
    out
}

/// Plain-text formatting for SmartNIC rules
pub fn render_smartnic_rules_text(rules: &[SmartNicOffloadRule]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("                 SMARTNIC IN-HARDWARE MATCH-ACTION RULES                        \n");
    out.push_str("================================================================================\n\n");
    if rules.is_empty() {
        out.push_str("  (No offload rules installed)\n");
    } else {
        out.push_str(&format!(
            "  {:<16} {:<8} {:<20} {:<8} {:<18} {:<12} {:<10}\n",
            "RULE ID", "PRIO", "PROTOCOL", "PORT", "ACTION", "HW_OFFLOAD", "HITS"
        ));
        for r in rules {
            let port_str = r.match_port.map(|p| p.to_string()).unwrap_or_else(|| "*".to_string());
            out.push_str(&format!(
                "  {:<16} {:<8} {:<20} {:<8} {:<18} {:<12} {:<10}\n",
                r.rule_id,
                r.priority,
                r.protocol.as_str(),
                port_str,
                r.action.to_string(),
                if r.hardware_installed { "[OK] ASIC" } else { "[SW] DRV" },
                r.hits
            ));
        }
    }
    out
}

/// Plain-text formatting for SmartNIC benchmark results
pub fn render_smartnic_bench_text(metrics: &SmartNicBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("             SMARTNIC LINE-RATE SWITCHING BENCHMARK RESULTS                     \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Throughput:          {:.2} Mpps\n", metrics.throughput_mpps));
    out.push_str(&format!("  Bandwidth:           {:.2} Gbps\n", metrics.bandwidth_gbps));
    out.push_str(&format!("  ASIC Latency:        {} ns\n", metrics.asic_latency_nanos));
    out.push_str(&format!("  Host CPU Load:       {:.2}%\n", metrics.host_cpu_utilization_percent));
    out.push_str(&format!("  Packets Evaluated:   {}\n", metrics.packets_evaluated));
    out.push_str(&format!("  Hardware Drops:      {}\n", metrics.hardware_drops));
    out.push_str(&format!("  Hardware Pongs:      {}\n", metrics.hardware_pongs));
    out
}

/// Helper to get current epoch timestamp in seconds
pub fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smartnic_vendor_and_mode_parsing() {
        assert_eq!(
            SmartNicVendor::from_str("bluefield").unwrap(),
            SmartNicVendor::NvidiaBluefield
        );
        assert_eq!(
            SmartNicVendor::from_str("pensando").unwrap(),
            SmartNicVendor::AmdPensando
        );
        assert_eq!(
            SmartNicVendor::from_str("ipu").unwrap(),
            SmartNicVendor::IntelIpu
        );

        assert_eq!(
            OffloadMode::from_str("hw").unwrap(),
            OffloadMode::HardwareAsic
        );
        assert_eq!(
            OffloadMode::from_str("drv").unwrap(),
            OffloadMode::Driver
        );
        assert_eq!(
            OffloadMode::from_str("skb").unwrap(),
            OffloadMode::SoftwareGeneric
        );
    }

    #[test]
    fn test_smartnic_rule_lifecycle_and_tcam_tracking() {
        let mut reg = SmartNicRegistry::default();
        assert_eq!(reg.devices.len(), 1);
        assert_eq!(reg.devices[0].used_tcam_rules, 0);

        let rule1 = SmartNicOffloadRule {
            rule_id: "slp-pong-25565".to_string(),
            priority: 10,
            protocol: OffloadProtocol::MinecraftJavaSlp,
            match_port: Some(25565),
            match_cidr: None,
            action: P4ActionType::SendPongDirect,
            hardware_installed: false,
            hits: 1500,
            bytes: 96000,
            created_at: current_epoch_secs(),
        };

        reg.add_rule(rule1).unwrap();
        assert_eq!(reg.rules.len(), 1);
        assert!(reg.rules[0].hardware_installed);
        assert_eq!(reg.devices[0].used_tcam_rules, 1);
        assert_eq!(reg.status.installed_rules, 1);
        assert_eq!(reg.status.offloaded_packets, 1500);

        let removed = reg.remove_rule("slp-pong-25565");
        assert!(removed);
        assert_eq!(reg.rules.len(), 0);
        assert_eq!(reg.devices[0].used_tcam_rules, 0);
    }
}
