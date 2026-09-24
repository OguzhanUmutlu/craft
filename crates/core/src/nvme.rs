// crates/core/src/nvme.rs
//
// Core data models, NVMe-oF subsystem descriptors, namespace allocations,
// and storage fabric registry for Autonomous Zero-Copy Storage Fabrics.
// Strictly zero emojis.

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Transport type for NVMe over Fabrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NvmeTransportType {
    /// Remote Direct Memory Access (RoCE v2 / InfiniBand / iWARP)
    Rdma,
    /// Pure NVMe over TCP (standard port 4420)
    Tcp,
    /// In-memory / kernel loopback PCIe emulation
    Loopback,
}

impl NvmeTransportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rdma => "rdma",
            Self::Tcp => "tcp",
            Self::Loopback => "loopback",
        }
    }

    pub fn default_port(&self) -> u16 {
        match self {
            Self::Rdma | Self::Tcp => 4420,
            Self::Loopback => 0,
        }
    }
}

impl fmt::Display for NvmeTransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for NvmeTransportType {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rdma" | "roce" | "rocev2" | "infiniband" => Ok(Self::Rdma),
            "tcp" | "nvme-tcp" | "nvmetcp" => Ok(Self::Tcp),
            "loopback" | "pcie" | "local" => Ok(Self::Loopback),
            _ => Err(CraftError::Config(format!("Unknown NVMe transport type: {}", s))),
        }
    }
}

/// Subsystem classification according to NVMe-oF specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NvmeSubsystemType {
    /// Well-known discovery subsystem (nqn.2014-08.org.nvmexpress.discovery)
    Discovery,
    /// Storage target NVM subsystem exposing namespaces
    Nvm,
}

impl NvmeSubsystemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::Nvm => "nvm",
        }
    }
}

impl fmt::Display for NvmeSubsystemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for NvmeSubsystemType {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "discovery" | "disc" => Ok(Self::Discovery),
            "nvm" | "storage" | "target" => Ok(Self::Nvm),
            _ => Err(CraftError::Config(format!("Unknown NVMe subsystem type: {}", s))),
        }
    }
}

/// Port descriptor binding NVMe-oF transport to network address
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NvmePort {
    pub port_id: u16,
    pub trtype: NvmeTransportType,
    pub traddr: String,
    pub trsvcid: u16,
    pub status: String,
}

/// Namespace descriptor representing an allocated flash block storage volume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvmeNamespaceDescriptor {
    pub nsid: u32,
    pub size_blocks: u64,
    pub block_size: u32,
    pub capacity_bytes: u64,
    pub allocated_bytes: u64,
    pub server_id: Option<String>,
    pub dimension: Option<String>,
    pub thin_provisioned: bool,
    pub read_only: bool,
    pub created_at_secs: u64,
}

/// Target subsystem descriptor with NQN and exposed namespaces
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvmeSubsystemDescriptor {
    pub nqn: String,
    pub subsys_type: NvmeSubsystemType,
    pub namespaces: Vec<u32>,
    pub ports: Vec<NvmePort>,
    pub controllers: usize,
    pub status: String,
}

/// Cumulative telemetry, capacity, and performance status summary
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvmeStatusSummary {
    pub active_subsystems: usize,
    pub active_namespaces: usize,
    pub total_pool_bytes: u64,
    pub allocated_pool_bytes: u64,
    pub pool_utilization_percent: f64,
    pub active_controllers: usize,
    pub iops_current: f64,
    pub avg_latency_micros: f64,
    pub multipath_failovers_total: u64,
}

impl Default for NvmeStatusSummary {
    fn default() -> Self {
        Self {
            active_subsystems: 1,
            active_namespaces: 0,
            total_pool_bytes: 1_099_511_627_776, // 1 TB Flash Pool
            allocated_pool_bytes: 0,
            pool_utilization_percent: 0.0,
            active_controllers: 1,
            iops_current: 0.0,
            avg_latency_micros: 14.5,
            multipath_failovers_total: 0,
        }
    }
}

/// Benchmark metrics gathered during 4KB flash block I/O profiling
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvmeBenchmarkMetrics {
    pub ops_processed: usize,
    pub iops: f64,
    pub bandwidth_gbps: f64,
    pub avg_latency_micros: f64,
    pub p99_latency_micros: f64,
    pub block_size: usize,
    pub multipath_failovers: u64,
}

/// Persistent registry holding NVMe-oF subsystems, namespaces, and pool state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeRegistry {
    pub subsystems: Vec<NvmeSubsystemDescriptor>,
    pub namespaces: Vec<NvmeNamespaceDescriptor>,
    pub status: NvmeStatusSummary,
}

impl Default for NvmeRegistry {
    fn default() -> Self {
        let default_ports = vec![
            NvmePort {
                port_id: 1,
                trtype: NvmeTransportType::Rdma,
                traddr: "127.0.0.1".to_string(),
                trsvcid: 4420,
                status: "live".to_string(),
            },
            NvmePort {
                port_id: 2,
                trtype: NvmeTransportType::Tcp,
                traddr: "127.0.0.1".to_string(),
                trsvcid: 4421,
                status: "standby".to_string(),
            },
        ];

        let default_subsystem = NvmeSubsystemDescriptor {
            nqn: "nqn.2026-09.com.craft:nvme:pool-primary".to_string(),
            subsys_type: NvmeSubsystemType::Nvm,
            namespaces: Vec::new(),
            ports: default_ports,
            controllers: 1,
            status: "listening".to_string(),
        };

        Self {
            subsystems: vec![default_subsystem],
            namespaces: Vec::new(),
            status: NvmeStatusSummary::default(),
        }
    }
}

impl NvmeRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let file = &paths.nvme_registry_file;
        if !file.exists() {
            let default_reg = Self::default();
            default_reg.save(paths)?;
            return Ok(default_reg);
        }

        let content = fs::read_to_string(file).map_err(|e| {
            CraftError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read NVMe registry: {}", e),
            ))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse NVMe registry JSON: {}", e))
        })
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file = &paths.nvme_registry_file;
        if let Some(parent) = file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize NVMe registry: {}", e))
        })?;

        fs::write(file, content)?;
        Ok(())
    }

    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.nvme_lock.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.nvme_lock)
            .map_err(CraftError::Io)?;

        lock_file.lock_exclusive().map_err(CraftError::Io)?;

        let mut current = Self::load(paths)?;
        let result = f(&mut current);
        if result.is_ok() {
            current.recompute_summary();
            current.save(paths)?;
            *self = current;
        }

        let _ = lock_file.unlock();
        result
    }

    pub fn find_namespace(&self, nsid: u32) -> Option<&NvmeNamespaceDescriptor> {
        self.namespaces.iter().find(|n| n.nsid == nsid)
    }

    pub fn find_subsystem(&self, nqn: &str) -> Option<&NvmeSubsystemDescriptor> {
        self.subsystems.iter().find(|s| s.nqn == nqn)
    }

    pub fn recompute_summary(&mut self) {
        self.status.active_subsystems = self.subsystems.len();
        self.status.active_namespaces = self.namespaces.len();

        let allocated: u64 = self.namespaces.iter().map(|n| n.allocated_bytes).sum();
        self.status.allocated_pool_bytes = allocated;

        if self.status.total_pool_bytes > 0 {
            self.status.pool_utilization_percent =
                (allocated as f64 / self.status.total_pool_bytes as f64) * 100.0;
        } else {
            self.status.pool_utilization_percent = 0.0;
        }

        self.status.active_controllers = self.subsystems.iter().map(|s| s.controllers).sum();
    }
}

/// Helper to get current epoch timestamp in seconds
pub fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Format bytes into human-readable string (KB, MB, GB, TB)
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Plain-text table renderer for NVMe-oF target and pool status (strictly zero emojis)
pub fn render_nvme_status_text(summary: &NvmeStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("================================================================================
");
    out.push_str("        CRAFT AUTONOMOUS ZERO-COPY STORAGE FABRICS & NVME-OF TARGET             
");
    out.push_str("================================================================================
");
    out.push_str(&format!("{:<28} : {}
", "Active Subsystems", summary.active_subsystems));
    out.push_str(&format!("{:<28} : {}
", "Active Namespaces", summary.active_namespaces));
    out.push_str(&format!("{:<28} : {}
", "Active Controllers", summary.active_controllers));
    out.push_str(&format!("{:<28} : {}
", "Total Flash Pool", format_bytes(summary.total_pool_bytes)));
    out.push_str(&format!("{:<28} : {}
", "Allocated Flash Pool", format_bytes(summary.allocated_pool_bytes)));
    out.push_str(&format!("{:<28} : {:.2}%
", "Pool Utilization", summary.pool_utilization_percent));
    out.push_str(&format!("{:<28} : {:.0} IOPS
", "Current Throughput", summary.iops_current));
    out.push_str(&format!("{:<28} : {:.2} us
", "Average Latency", summary.avg_latency_micros));
    out.push_str(&format!("{:<28} : {}
", "Multipath Failovers", summary.multipath_failovers_total));
    out.push_str("--------------------------------------------------------------------------------
");
    out
}

/// Plain-text table renderer for allocated NVMe namespaces (strictly zero emojis)
pub fn render_nvme_namespaces_text(namespaces: &[NvmeNamespaceDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================
");
    out.push_str("                  ALLOCATED NVME-OF STORAGE NAMESPACES                          
");
    out.push_str("================================================================================
");
    if namespaces.is_empty() {
        out.push_str("  No namespaces allocated in flash pool. Use 'craft nvme ns-create' to allocate.
");
        out.push_str("--------------------------------------------------------------------------------
");
        return out;
    }

    out.push_str(&format!(
        "{:<6} {:<12} {:<10} {:<12} {:<16} {:<12} {:<6}
",
        "NSID", "BLOCKS", "BLK_SIZE", "CAPACITY", "SERVER", "DIMENSION", "THIN"
    ));
    out.push_str("--------------------------------------------------------------------------------
");
    for ns in namespaces {
        let server_str = ns.server_id.as_deref().unwrap_or("-");
        let dim_str = ns.dimension.as_deref().unwrap_or("-");
        let thin_str = if ns.thin_provisioned { "YES" } else { "NO" };

        out.push_str(&format!(
            "{:<6} {:<12} {:<10} {:<12} {:<16} {:<12} {:<6}
",
            ns.nsid,
            ns.size_blocks,
            format!("{}B", ns.block_size),
            format_bytes(ns.capacity_bytes),
            server_str,
            dim_str,
            thin_str,
        ));
    }
    out.push_str("--------------------------------------------------------------------------------
");
    out
}

/// Plain-text table renderer for NVMe-oF target subsystems (strictly zero emojis)
pub fn render_nvme_subsystems_text(subsystems: &[NvmeSubsystemDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================
");
    out.push_str("                   NVME-OF TARGET SUBSYSTEMS & PORTS                            
");
    out.push_str("================================================================================
");
    if subsystems.is_empty() {
        out.push_str("  No subsystems configured.
");
        out.push_str("--------------------------------------------------------------------------------
");
        return out;
    }

    for sub in subsystems {
        out.push_str(&format!("NQN: {}
", sub.nqn));
        out.push_str(&format!("  Type: {:<12} Status: {:<10} Controllers: {}
", sub.subsys_type, sub.status, sub.controllers));
        let ns_str = if sub.namespaces.is_empty() {
            "None".to_string()
        } else {
            sub.namespaces.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        };
        out.push_str(&format!("  Bound Namespaces: [{}]
", ns_str));
        out.push_str("  Listening Transport Ports:
");
        for p in &sub.ports {
            out.push_str(&format!(
                "    [Port {}] {:<6} {}:{} ({})
",
                p.port_id, p.trtype, p.traddr, p.trsvcid, p.status
            ));
        }
        out.push_str("--------------------------------------------------------------------------------
");
    }
    out
}

/// Plain-text table renderer for 4KB flash block I/O benchmark results (strictly zero emojis)
pub fn render_nvme_bench_text(bench: &NvmeBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("================================================================================
");
    out.push_str("       NVME-OF ZERO-COPY FLASH FABRIC 4KB RANDOM I/O BENCHMARK RESULTS          
");
    out.push_str("================================================================================
");
    out.push_str(&format!("{:<28} : {}
", "I/O Operations Executed", bench.ops_processed));
    out.push_str(&format!("{:<28} : {} bytes
", "Block Transfer Size", bench.block_size));
    out.push_str(&format!("{:<28} : {:.0} IOPS
", "I/O Throughput", bench.iops));
    out.push_str(&format!("{:<28} : {:.2} Gbps
", "Bandwidth", bench.bandwidth_gbps));
    out.push_str(&format!("{:<28} : {:.2} us
", "Average I/O Latency", bench.avg_latency_micros));
    out.push_str(&format!("{:<28} : {:.2} us
", "P99 Tail Latency", bench.p99_latency_micros));
    out.push_str(&format!("{:<28} : {}
", "Multipath Failover Events", bench.multipath_failovers));
    out.push_str("--------------------------------------------------------------------------------
");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvme_transport_and_subsys_str() {
        assert_eq!(NvmeTransportType::from_str("rdma").unwrap(), NvmeTransportType::Rdma);
        assert_eq!(NvmeTransportType::from_str("tcp").unwrap(), NvmeTransportType::Tcp);
        assert_eq!(NvmeTransportType::from_str("loopback").unwrap(), NvmeTransportType::Loopback);
        assert_eq!(NvmeSubsystemType::from_str("discovery").unwrap(), NvmeSubsystemType::Discovery);
        assert_eq!(NvmeSubsystemType::from_str("nvm").unwrap(), NvmeSubsystemType::Nvm);
    }

    #[test]
    fn test_nvme_registry_default_and_modify() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());

        let mut reg = NvmeRegistry::load(&paths).unwrap();
        assert_eq!(reg.subsystems.len(), 1);
        assert_eq!(reg.namespaces.len(), 0);

        reg.modify(&paths, |r| {
            r.namespaces.push(NvmeNamespaceDescriptor {
                nsid: 1,
                size_blocks: 262144, // 1 GB at 4KB blocks
                block_size: 4096,
                capacity_bytes: 1_073_741_824,
                allocated_bytes: 1_073_741_824,
                server_id: Some("lobby".to_string()),
                dimension: Some("overworld".to_string()),
                thin_provisioned: false,
                read_only: false,
                created_at_secs: current_epoch_secs(),
            });
            r.subsystems[0].namespaces.push(1);
            Ok(())
        }).unwrap();

        assert_eq!(reg.namespaces.len(), 1);
        assert_eq!(reg.status.active_namespaces, 1);
        assert_eq!(reg.status.allocated_pool_bytes, 1_073_741_824);

        let reloaded = NvmeRegistry::load(&paths).unwrap();
        assert_eq!(reloaded.namespaces.len(), 1);
        assert_eq!(reloaded.namespaces[0].nsid, 1);
        assert_eq!(reloaded.namespaces[0].dimension.as_deref(), Some("overworld"));
    }

    #[test]
    fn test_nvme_text_renderers() {
        let summary = NvmeStatusSummary::default();
        let text = render_nvme_status_text(&summary);
        assert!(text.contains("CRAFT AUTONOMOUS ZERO-COPY STORAGE FABRICS"));
        assert!(text.contains("Active Subsystems"));

        let ns = vec![NvmeNamespaceDescriptor {
            nsid: 1,
            size_blocks: 1024,
            block_size: 4096,
            capacity_bytes: 4_194_304,
            allocated_bytes: 4_194_304,
            server_id: Some("survival".to_string()),
            dimension: Some("the_nether".to_string()),
            thin_provisioned: true,
            read_only: false,
            created_at_secs: 1000,
        }];
        let ns_text = render_nvme_namespaces_text(&ns);
        assert!(ns_text.contains("ALLOCATED NVME-OF STORAGE NAMESPACES"));
        assert!(ns_text.contains("the_nether"));

        let bench = NvmeBenchmarkMetrics {
            ops_processed: 10000,
            iops: 920000.0,
            bandwidth_gbps: 29.44,
            avg_latency_micros: 15.2,
            p99_latency_micros: 18.7,
            block_size: 4096,
            multipath_failovers: 1,
        };
        let b_text = render_nvme_bench_text(&bench);
        assert!(b_text.contains("920000 IOPS"));
    }
}
