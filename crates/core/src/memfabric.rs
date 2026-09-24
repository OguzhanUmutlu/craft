// crates/core/src/memfabric.rs
//
// Core data models, virtual page tables, CXL/NVRAM descriptors,
// and memory fabric registry for Autonomous Distributed Inter-Server Memory Fabric.
// Strictly zero emojis.

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Memory tier within the distributed cluster hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    /// Local CPU DDR4/DDR5 DRAM
    LocalDram,
    /// Compute Express Link (CXL) pooled / persistent memory
    CxlPmem,
    /// Remote server DRAM accessible via sub-microsecond RDMA
    RemoteRdmaDram,
    /// Remote clustered Non-Volatile RAM (NVRAM) pool
    RemoteNvram,
}

impl MemoryTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LocalDram => "local_dram",
            Self::CxlPmem => "cxl_pmem",
            Self::RemoteRdmaDram => "remote_rdma_dram",
            Self::RemoteNvram => "remote_nvram",
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::RemoteRdmaDram | Self::RemoteNvram)
    }
}

impl fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for MemoryTier {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" | "dram" | "local_dram" => Ok(Self::LocalDram),
            "cxl" | "pmem" | "cxl_pmem" => Ok(Self::CxlPmem),
            "remote" | "rdma" | "remote_rdma" | "remote_rdma_dram" => Ok(Self::RemoteRdmaDram),
            "nvram" | "remote_nvram" => Ok(Self::RemoteNvram),
            _ => Err(CraftError::Config(format!("Unknown memory tier: {}", s))),
        }
    }
}

/// Page memory protection permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageProtection {
    Read,
    ReadWrite,
    ReadWriteExec,
}

impl PageProtection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "r--",
            Self::ReadWrite => "rw-",
            Self::ReadWriteExec => "rwx",
        }
    }
}

impl fmt::Display for PageProtection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for PageProtection {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "r" | "read" | "r--" => Ok(Self::Read),
            "rw" | "readwrite" | "rw-" => Ok(Self::ReadWrite),
            "rwx" | "readwriteexec" => Ok(Self::ReadWriteExec),
            _ => Err(CraftError::Config(format!("Unknown page protection: {}", s))),
        }
    }
}

/// Descriptor representing an allocated memory page in the fabric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePageDescriptor {
    pub page_id: String,
    pub virtual_addr: u64,
    pub page_size: usize,
    pub tier: MemoryTier,
    pub node_id: String,
    pub remote_mr_id: Option<String>,
    pub remote_offset: u64,
    pub protection: PageProtection,
    pub is_dirty: bool,
    pub access_count: u64,
    pub dimension: Option<String>,
    pub created_at_secs: u64,
    pub last_access_secs: u64,
}

/// Clustered compute or memory expansion node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemFabricNodeInfo {
    pub node_id: String,
    pub address: String,
    pub dram_total_bytes: u64,
    pub dram_allocated_bytes: u64,
    pub nvram_total_bytes: u64,
    pub nvram_allocated_bytes: u64,
    pub cxl_enabled: bool,
    pub interconnect_latency_nanos: u64,
}

/// Cumulative telemetry and health metrics for the memory fabric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemFabricStatusSummary {
    pub active_nodes: usize,
    pub total_dram_bytes: u64,
    pub allocated_dram_bytes: u64,
    pub total_nvram_bytes: u64,
    pub allocated_nvram_bytes: u64,
    pub dram_utilization_percent: f64,
    pub nvram_utilization_percent: f64,
    pub total_pages_managed: usize,
    pub remote_pages_count: usize,
    pub page_faults_total: u64,
    pub avg_page_fault_latency_nanos: u64,
    pub remote_evictions_total: u64,
}

impl Default for MemFabricStatusSummary {
    fn default() -> Self {
        Self {
            active_nodes: 1,
            total_dram_bytes: 68_719_476_736,       // 64 GB
            allocated_dram_bytes: 4_294_967_296,    // 4 GB
            total_nvram_bytes: 274_877_906_944,     // 256 GB
            allocated_nvram_bytes: 0,
            dram_utilization_percent: 6.25,
            nvram_utilization_percent: 0.0,
            total_pages_managed: 0,
            remote_pages_count: 0,
            page_faults_total: 0,
            avg_page_fault_latency_nanos: 420,
            remote_evictions_total: 0,
        }
    }
}

/// Metrics gathered during memory fabric benchmarking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemFabricBenchmarkMetrics {
    pub pages_processed: usize,
    pub throughput_pages_sec: f64,
    pub bandwidth_gbps: f64,
    pub avg_latency_nanos: u64,
    pub cache_hit_rate_percent: f64,
    pub dirty_evictions: u64,
}

/// Persistent registry holding memory fabric state, nodes, and page tables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemFabricRegistry {
    pub nodes: Vec<MemFabricNodeInfo>,
    pub pages: Vec<RemotePageDescriptor>,
    pub status: MemFabricStatusSummary,
}

impl Default for MemFabricRegistry {
    fn default() -> Self {
        let local_node = MemFabricNodeInfo {
            node_id: "local-node".to_string(),
            address: "127.0.0.1:4791".to_string(),
            dram_total_bytes: 68_719_476_736,
            dram_allocated_bytes: 4_294_967_296,
            nvram_total_bytes: 274_877_906_944,
            nvram_allocated_bytes: 0,
            cxl_enabled: true,
            interconnect_latency_nanos: 85,
        };

        Self {
            nodes: vec![local_node],
            pages: Vec::new(),
            status: MemFabricStatusSummary::default(),
        }
    }
}

impl MemFabricRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let file = &paths.memfabric_registry_file;
        if !file.exists() {
            let default_reg = Self::default();
            default_reg.save(paths)?;
            return Ok(default_reg);
        }

        let content = fs::read_to_string(file).map_err(|e| {
            CraftError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read memory fabric registry: {}", e),
            ))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            CraftError::Config(format!(
                "Failed to parse memory fabric registry JSON: {}",
                e
            ))
        })
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file = &paths.memfabric_registry_file;
        if let Some(parent) = file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!(
                "Failed to serialize memory fabric registry: {}",
                e
            ))
        })?;

        fs::write(file, content)?;
        Ok(())
    }

    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.memfabric_lock.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.memfabric_lock)
            .map_err(|e| CraftError::Other(format!("Failed to open memfabric lock file: {}", e)))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire memfabric exclusive lock: {}", e)))?;

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

    pub fn allocate_page(
        &mut self,
        page_id: String,
        size: usize,
        tier: MemoryTier,
        dimension: Option<String>,
    ) -> Result<RemotePageDescriptor> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let vaddr = 0x7fff_0000_0000u64 + (self.pages.len() as u64 * size as u64);
        let node_id = if tier.is_remote() {
            "remote-fabric-node-1".to_string()
        } else {
            "local-node".to_string()
        };

        let page = RemotePageDescriptor {
            page_id,
            virtual_addr: vaddr,
            page_size: size,
            tier,
            node_id,
            remote_mr_id: if tier.is_remote() {
                Some("mr-fabric-0x01".to_string())
            } else {
                None
            },
            remote_offset: (self.pages.len() as u64) * size as u64,
            protection: PageProtection::ReadWrite,
            is_dirty: false,
            access_count: 1,
            dimension,
            created_at_secs: now,
            last_access_secs: now,
        };

        self.pages.push(page.clone());
        self.status.total_pages_managed = self.pages.len();
        if tier.is_remote() {
            self.status.remote_pages_count += 1;
            self.status.allocated_nvram_bytes += size as u64;
        } else {
            self.status.allocated_dram_bytes += size as u64;
        }

        if self.status.total_dram_bytes > 0 {
            self.status.dram_utilization_percent = (self.status.allocated_dram_bytes as f64
                / self.status.total_dram_bytes as f64)
                * 100.0;
        }
        if self.status.total_nvram_bytes > 0 {
            self.status.nvram_utilization_percent = (self.status.allocated_nvram_bytes as f64
                / self.status.total_nvram_bytes as f64)
                * 100.0;
        }

        Ok(page)
    }

    pub fn evict_dimension(
        &mut self,
        dimension: &str,
        target_node: Option<&str>,
    ) -> Result<(usize, u64)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut count = 0usize;
        let mut bytes = 0u64;
        let target = target_node.unwrap_or("remote-fabric-node-1");

        for page in self.pages.iter_mut() {
            if let Some(ref dim) = page.dimension {
                if dim.eq_ignore_ascii_case(dimension) && !page.tier.is_remote() {
                    page.tier = MemoryTier::RemoteRdmaDram;
                    page.node_id = target.to_string();
                    page.remote_mr_id = Some("mr-evicted-dim".to_string());
                    page.last_access_secs = now;
                    count += 1;
                    bytes += page.page_size as u64;
                }
            }
        }

        self.status.remote_evictions_total += count as u64;
        self.status.remote_pages_count += count;
        if count > 0 {
            self.status.allocated_dram_bytes = self.status.allocated_dram_bytes.saturating_sub(bytes);
            self.status.allocated_nvram_bytes += bytes;
        }

        Ok((count, bytes))
    }

    pub fn reset_metrics(&mut self) {
        self.status.page_faults_total = 0;
        self.status.remote_evictions_total = 0;
        for p in self.pages.iter_mut() {
            p.access_count = 0;
            p.is_dirty = false;
        }
    }
}

/// Plain-text formatting for memory fabric status
pub fn render_memfabric_status_text(
    summary: &MemFabricStatusSummary,
    nodes: &[MemFabricNodeInfo],
) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("     CRAFT DISTRIBUTED INTER-SERVER MEMORY FABRIC & NVRAM POOL STATUS           \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Active Nodes:           {}\n", summary.active_nodes));
    out.push_str(&format!(
        "  Total DRAM:             {:.2} GB (Allocated: {:.2} GB, {:.1}%)\n",
        summary.total_dram_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        summary.allocated_dram_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        summary.dram_utilization_percent
    ));
    out.push_str(&format!(
        "  Total NVRAM:            {:.2} GB (Allocated: {:.2} GB, {:.1}%)\n",
        summary.total_nvram_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        summary.allocated_nvram_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        summary.nvram_utilization_percent
    ));
    out.push_str(&format!(
        "  Pages Managed:          {} (Remote Paged: {})\n",
        summary.total_pages_managed, summary.remote_pages_count
    ));
    out.push_str(&format!("  Page Faults Resolved:   {}\n", summary.page_faults_total));
    out.push_str(&format!(
        "  Avg Paging Latency:     {} ns ({:.3} us)\n",
        summary.avg_page_fault_latency_nanos,
        summary.avg_page_fault_latency_nanos as f64 / 1000.0
    ));
    out.push_str(&format!("  Remote Evictions:       {}\n\n", summary.remote_evictions_total));

    out.push_str("--- Clustered Memory Fabric Nodes ---\n");
    if nodes.is_empty() {
        out.push_str("  (No fabric nodes registered)\n");
    } else {
        out.push_str(&format!(
            "  {:<16} {:<20} {:<12} {:<12} {:<6} {:<10}\n",
            "NODE ID", "ADDRESS", "DRAM (GB)", "NVRAM (GB)", "CXL", "LATENCY"
        ));
        for n in nodes {
            out.push_str(&format!(
                "  {:<16} {:<20} {:<12.1} {:<12.1} {:<6} {:<10}\n",
                n.node_id,
                n.address,
                n.dram_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                n.nvram_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                if n.cxl_enabled { "YES" } else { "NO" },
                format!("{}ns", n.interconnect_latency_nanos)
            ));
        }
    }

    out
}

/// Plain-text formatting for allocated fabric pages
pub fn render_memfabric_pages_text(pages: &[RemotePageDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("            DISTRIBUTED FABRIC VIRTUAL PAGE TABLE ENTRIES                       \n");
    out.push_str("================================================================================\n\n");

    if pages.is_empty() {
        out.push_str("  (No active fabric pages allocated)\n");
    } else {
        out.push_str(&format!(
            "  {:<20} {:<16} {:<10} {:<16} {:<12} {:<8}\n",
            "PAGE ID", "VIRTUAL ADDR", "SIZE", "TIER", "NODE", "HITS"
        ));
        for p in pages {
            out.push_str(&format!(
                "  {:<20} 0x{:012x}  {:<10} {:<16} {:<12} {:<8}\n",
                p.page_id,
                p.virtual_addr,
                format!("{}KB", p.page_size / 1024),
                p.tier.as_str(),
                p.node_id,
                p.access_count
            ));
        }
    }

    out
}

/// Plain-text formatting for memory fabric benchmark metrics
pub fn render_memfabric_bench_text(metrics: &MemFabricBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("          MEMORY FABRIC ZERO-COPY PAGING BENCHMARK RESULTS                      \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Pages Processed:     {}\n", metrics.pages_processed));
    out.push_str(&format!("  Throughput:          {:.2} pages/sec\n", metrics.throughput_pages_sec));
    out.push_str(&format!("  Bandwidth:           {:.2} Gbps\n", metrics.bandwidth_gbps));
    out.push_str(&format!("  Average Latency:     {} ns ({:.3} us)\n", metrics.avg_latency_nanos, metrics.avg_latency_nanos as f64 / 1000.0));
    out.push_str(&format!("  Cache Hit Rate:      {:.2}%\n", metrics.cache_hit_rate_percent));
    out.push_str(&format!("  Dirty Evictions:     {}\n", metrics.dirty_evictions));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tier_parsing_and_str() {
        assert_eq!(MemoryTier::from_str("local").unwrap(), MemoryTier::LocalDram);
        assert_eq!(MemoryTier::from_str("cxl").unwrap(), MemoryTier::CxlPmem);
        assert_eq!(MemoryTier::from_str("remote_rdma").unwrap(), MemoryTier::RemoteRdmaDram);
        assert_eq!(MemoryTier::from_str("nvram").unwrap(), MemoryTier::RemoteNvram);
        assert!(MemoryTier::RemoteRdmaDram.is_remote());
        assert!(!MemoryTier::LocalDram.is_remote());
    }

    #[test]
    fn test_page_protection_parsing() {
        assert_eq!(PageProtection::from_str("r").unwrap(), PageProtection::Read);
        assert_eq!(PageProtection::from_str("rw").unwrap(), PageProtection::ReadWrite);
        assert_eq!(PageProtection::from_str("rwx").unwrap(), PageProtection::ReadWriteExec);
    }

    #[test]
    fn test_registry_page_allocation_and_eviction() {
        let mut reg = MemFabricRegistry::default();
        let page = reg
            .allocate_page(
                "page-dim-nether-1".to_string(),
                2_097_152, // 2MB THP
                MemoryTier::LocalDram,
                Some("the_nether".to_string()),
            )
            .unwrap();

        assert_eq!(page.page_id, "page-dim-nether-1");
        assert_eq!(reg.pages.len(), 1);
        assert_eq!(reg.status.total_pages_managed, 1);

        // Evict nether to remote fabric node
        let (evicted_count, evicted_bytes) = reg.evict_dimension("the_nether", None).unwrap();
        assert_eq!(evicted_count, 1);
        assert_eq!(evicted_bytes, 2_097_152);
        assert_eq!(reg.pages[0].tier, MemoryTier::RemoteRdmaDram);
        assert_eq!(reg.status.remote_evictions_total, 1);
    }
}
