use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdmaTransportType {
    RoceV2,
    InfiniBand,
    SoftRoceFallback,
    EmulatedMemoryVerbs,
}

impl fmt::Display for RdmaTransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoceV2 => write!(f, "RoCE v2"),
            Self::InfiniBand => write!(f, "InfiniBand"),
            Self::SoftRoceFallback => write!(f, "Soft-RoCE Fallback"),
            Self::EmulatedMemoryVerbs => write!(f, "Emulated Memory Verbs"),
        }
    }
}

impl RdmaTransportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoceV2 => "RoCE v2",
            Self::InfiniBand => "InfiniBand",
            Self::SoftRoceFallback => "Soft-RoCE Fallback",
            Self::EmulatedMemoryVerbs => "Emulated Verbs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdmaQpType {
    ReliableConnected,
    UnreliableDatagram,
    ReliableDatagram,
    ExtendedReliableConnected,
}

impl fmt::Display for RdmaQpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReliableConnected => write!(f, "RC (Reliable Connected)"),
            Self::UnreliableDatagram => write!(f, "UD (Unreliable Datagram)"),
            Self::ReliableDatagram => write!(f, "RD (Reliable Datagram)"),
            Self::ExtendedReliableConnected => write!(f, "XRC (Extended RC)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdmaQpState {
    Reset,
    Init,
    ReadyToReceive,
    ReadyToSend,
    SendQueueDrain,
    SendQueueError,
    Error,
}

impl fmt::Display for RdmaQpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reset => write!(f, "RESET"),
            Self::Init => write!(f, "INIT"),
            Self::ReadyToReceive => write!(f, "RTR"),
            Self::ReadyToSend => write!(f, "RTS"),
            Self::SendQueueDrain => write!(f, "SQD"),
            Self::SendQueueError => write!(f, "SQE"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

impl RdmaQpState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reset => "RESET",
            Self::Init => "INIT",
            Self::ReadyToReceive => "RTR",
            Self::ReadyToSend => "RTS",
            Self::SendQueueDrain => "SQD",
            Self::SendQueueError => "SQE",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmaAccessFlags {
    pub local_write: bool,
    pub remote_write: bool,
    pub remote_read: bool,
    pub remote_atomic: bool,
    pub mw_bind: bool,
}

impl Default for RdmaAccessFlags {
    fn default() -> Self {
        Self {
            local_write: true,
            remote_write: true,
            remote_read: true,
            remote_atomic: false,
            mw_bind: false,
        }
    }
}

impl RdmaAccessFlags {
    pub const LOCAL_WRITE: Self = Self { local_write: true, remote_write: false, remote_read: false, remote_atomic: false, mw_bind: false };
    pub const REMOTE_WRITE: Self = Self { local_write: false, remote_write: true, remote_read: false, remote_atomic: false, mw_bind: false };
    pub const REMOTE_READ: Self = Self { local_write: false, remote_write: false, remote_read: true, remote_atomic: false, mw_bind: false };
    pub const REMOTE_ATOMIC: Self = Self { local_write: false, remote_write: false, remote_read: false, remote_atomic: true, mw_bind: false };
    pub const MW_BIND: Self = Self { local_write: false, remote_write: false, remote_read: false, remote_atomic: false, mw_bind: true };

    pub fn read_only() -> Self {
        Self {
            local_write: false,
            remote_write: false,
            remote_read: true,
            remote_atomic: false,
            mw_bind: false,
        }
    }

    pub fn as_str(&self) -> String {
        let mut flags = Vec::new();
        if self.local_write { flags.push("LOCAL_WRITE"); }
        if self.remote_write { flags.push("REMOTE_WRITE"); }
        if self.remote_read { flags.push("REMOTE_READ"); }
        if self.remote_atomic { flags.push("REMOTE_ATOMIC"); }
        if self.mw_bind { flags.push("MW_BIND"); }
        if flags.is_empty() { "NONE".to_string() } else { flags.join("|") }
    }
}

impl std::ops::BitOr for RdmaAccessFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            local_write: self.local_write || rhs.local_write,
            remote_write: self.remote_write || rhs.remote_write,
            remote_read: self.remote_read || rhs.remote_read,
            remote_atomic: self.remote_atomic || rhs.remote_atomic,
            mw_bind: self.mw_bind || rhs.mw_bind,
        }
    }
}

impl std::ops::BitOrAssign for RdmaAccessFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegionDescriptor {
    pub mr_id: String,
    pub lkey: u32,
    pub rkey: u32,
    pub addr: u64,
    pub length: usize,
    pub protection_domain_id: u32,
    pub access_flags: RdmaAccessFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuePairConfig {
    pub qp_id: u32,
    pub qp_type: RdmaQpType,
    pub protection_domain_id: u32,
    pub send_cq_id: u32,
    pub recv_cq_id: u32,
    pub max_send_wr: u32,
    pub max_recv_wr: u32,
    pub max_inline_data: u32,
    pub dest_qp_num: u32,
    pub dest_gid_or_ip: String,
    pub state: RdmaQpState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkCompletionStatus {
    Success,
    LocLenErr,
    LocProtErr,
    RemAccErr,
    RemOpErr,
    BadRespErr,
    RespTimeout,
}

impl fmt::Display for WorkCompletionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => write!(f, "SUCCESS"),
            Self::LocLenErr => write!(f, "LOC_LEN_ERR"),
            Self::LocProtErr => write!(f, "LOC_PROT_ERR"),
            Self::RemAccErr => write!(f, "REM_ACC_ERR"),
            Self::RemOpErr => write!(f, "REM_OP_ERR"),
            Self::BadRespErr => write!(f, "BAD_RESP_ERR"),
            Self::RespTimeout => write!(f, "RESP_TIMEOUT"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkOpcode {
    Send,
    RdmaWrite,
    RdmaRead,
    AtomicCmpSwap,
    AtomicFetchAdd,
    Recv,
}

impl fmt::Display for WorkOpcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send => write!(f, "SEND"),
            Self::RdmaWrite => write!(f, "RDMA_WRITE"),
            Self::RdmaRead => write!(f, "RDMA_READ"),
            Self::AtomicCmpSwap => write!(f, "ATOMIC_CMP_SWAP"),
            Self::AtomicFetchAdd => write!(f, "ATOMIC_FETCH_ADD"),
            Self::Recv => write!(f, "RECV"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCompletion {
    pub wr_id: u64,
    pub status: WorkCompletionStatus,
    pub opcode: WorkOpcode,
    pub bytes_transferred: usize,
    pub qp_num: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRequest {
    pub wr_id: u64,
    pub opcode: WorkOpcode,
    pub signaled: bool,
    pub inline_data: bool,
    pub fence: bool,
    pub local_addr: u64,
    pub length: usize,
    pub lkey: u32,
    pub remote_addr: u64,
    pub rkey: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdmaLinkStatus {
    Active,
    Degraded,
    Down,
    FallbackActive,
}

impl fmt::Display for RdmaLinkStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "ACTIVE"),
            Self::Degraded => write!(f, "DEGRADED"),
            Self::Down => write!(f, "DOWN"),
            Self::FallbackActive => write!(f, "FALLBACK_ACTIVE"),
        }
    }
}

impl RdmaLinkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Degraded => "DEGRADED",
            Self::Down => "DOWN",
            Self::FallbackActive => "FALLBACK_ACTIVE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaPeerEndpoint {
    pub node_id: String,
    pub server_name: String,
    pub transport: RdmaTransportType,
    pub gid_or_ip: String,
    pub qp_num: u32,
    pub rkey: u32,
    pub link_status: RdmaLinkStatus,
    pub rtt_nanos: u64,
    pub bandwidth_gbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaStatusSummary {
    pub active_qps: usize,
    pub registered_mrs: usize,
    pub total_registered_bytes: usize,
    pub link_status: RdmaLinkStatus,
    pub tx_bandwidth_gbps: f64,
    pub rx_bandwidth_gbps: f64,
    pub avg_latency_nanos: u64,
    pub fallback_to_tcp_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaBenchmarkMetrics {
    pub operations_completed: usize,
    pub bytes_transferred: usize,
    pub throughput_ops_per_sec: f64,
    pub bandwidth_gbps: f64,
    pub avg_latency_nanos: u64,
    pub p95_latency_nanos: u64,
    pub p99_latency_nanos: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RdmaRegistry {
    pub peers: Vec<RdmaPeerEndpoint>,
    pub memory_regions: Vec<MemoryRegionDescriptor>,
    pub queue_pairs: Vec<QueuePairConfig>,
}

impl RdmaRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.rdma_registry_file.exists() {
            return Ok(Self::default());
        }
        let mut file = File::open(&paths.rdma_registry_file)
            .map_err(|e| CraftError::Io(e))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| CraftError::Io(e))?;
        serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse RDMA registry: {}", e)))
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.rdma_registry_file.parent() {
            fs::create_dir_all(parent).map_err(|e| CraftError::Io(e))?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize RDMA registry: {}", e)))?;
        let temp_file = paths.rdma_registry_file.with_extension("tmp");
        let mut f = File::create(&temp_file).map_err(|e| CraftError::Io(e))?;
        f.write_all(content.as_bytes())
            .map_err(|e| CraftError::Io(e))?;
        f.sync_all().map_err(|e| CraftError::Io(e))?;
        fs::rename(&temp_file, &paths.rdma_registry_file).map_err(|e| CraftError::Io(e))?;
        Ok(())
    }

    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.rdma_lock.parent() {
            fs::create_dir_all(parent).map_err(|e| CraftError::Io(e))?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.rdma_lock)
            .map_err(|e| CraftError::Other(format!("Failed to open RDMA lock file: {}", e)))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire RDMA exclusive lock: {}", e)))?;

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
}

pub fn render_rdma_status_text(summary: &RdmaStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("          CRAFT RDMA ACCELERATION & SUB-MICROSECOND FABRIC STATUS               \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Link Status:              {}\n", summary.link_status));
    out.push_str(&format!("  Active Queue Pairs:       {}\n", summary.active_qps));
    out.push_str(&format!("  Registered MRs:           {}\n", summary.registered_mrs));
    out.push_str(&format!("  Total Registered Memory:  {} bytes ({:.2} MB)\n", 
        summary.total_registered_bytes, summary.total_registered_bytes as f64 / (1024.0 * 1024.0)));
    out.push_str(&format!("  Tx Bandwidth:             {:.2} Gbps\n", summary.tx_bandwidth_gbps));
    out.push_str(&format!("  Rx Bandwidth:             {:.2} Gbps\n", summary.rx_bandwidth_gbps));
    out.push_str(&format!("  Average Latency:          {} ns ({:.3} us)\n", 
        summary.avg_latency_nanos, summary.avg_latency_nanos as f64 / 1000.0));
    out.push_str(&format!("  TCP Fallback Active:      {}\n", 
        if summary.fallback_to_tcp_active { "[WARN] YES (Degraded Mode)" } else { "[OK] NO (Zero-Copy DMA)" }));
    out.push_str("================================================================================\n");
    out
}

pub fn render_rdma_bench_text(metrics: &RdmaBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("               RDMA FABRIC ZERO-COPY BENCHMARK RESULTS                          \n");
    out.push_str("================================================================================\n\n");
    out.push_str(&format!("  Operations Completed:     {}\n", metrics.operations_completed));
    out.push_str(&format!("  Total Bytes Transferred:  {} bytes ({:.2} MB)\n", 
        metrics.bytes_transferred, metrics.bytes_transferred as f64 / (1024.0 * 1024.0)));
    out.push_str(&format!("  Throughput:               {:.2} ops/sec\n", metrics.throughput_ops_per_sec));
    out.push_str(&format!("  Direct DMA Bandwidth:     {:.2} Gbps\n", metrics.bandwidth_gbps));
    out.push_str(&format!("  Average RDMA Latency:     {} ns ({:.3} us)\n", 
        metrics.avg_latency_nanos, metrics.avg_latency_nanos as f64 / 1000.0));
    out.push_str(&format!("  p95 Latency:              {} ns ({:.3} us)\n", 
        metrics.p95_latency_nanos, metrics.p95_latency_nanos as f64 / 1000.0));
    out.push_str(&format!("  p99 Latency:              {} ns ({:.3} us)\n", 
        metrics.p99_latency_nanos, metrics.p99_latency_nanos as f64 / 1000.0));
    out.push_str("================================================================================\n");
    out
}

pub fn render_rdma_peers_text(peers: &[RdmaPeerEndpoint]) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("               CONNECTED RDMA FABRIC PEER ENDPOINTS                             \n");
    out.push_str("================================================================================\n\n");
    if peers.is_empty() {
        out.push_str("  No active RDMA fabric peer endpoints registered.\n");
    } else {
        out.push_str(&format!("  {:<15} {:<15} {:<15} {:<8} {:<12} {:<10}\n", 
            "NODE ID", "SERVER", "ENDPOINT/GID", "QP NUM", "STATUS", "RTT (ns)"));
        out.push_str("  ----------------------------------------------------------------------------\n");
        for peer in peers {
            out.push_str(&format!("  {:<15} {:<15} {:<15} {:<8} {:<12} {:<10}\n",
                peer.node_id, peer.server_name, peer.gid_or_ip, peer.qp_num, format!("{}", peer.link_status), peer.rtt_nanos));
        }
    }
    out.push_str("================================================================================\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdma_models_and_access_flags() {
        let flags = RdmaAccessFlags::default();
        assert!(flags.local_write);
        assert!(flags.remote_write);
        assert!(flags.remote_read);
        assert!(!flags.remote_atomic);

        let ro = RdmaAccessFlags::read_only();
        assert!(!ro.local_write);
        assert!(!ro.remote_write);
        assert!(ro.remote_read);
    }

    #[test]
    fn test_rdma_qp_state_display() {
        assert_eq!(format!("{}", RdmaQpState::Reset), "RESET");
        assert_eq!(format!("{}", RdmaQpState::ReadyToSend), "RTS");
        assert_eq!(format!("{}", RdmaTransportType::RoceV2), "RoCE v2");
        assert_eq!(format!("{}", WorkCompletionStatus::Success), "SUCCESS");
    }

    #[test]
    fn test_render_rdma_text() {
        let summary = RdmaStatusSummary {
            active_qps: 2,
            registered_mrs: 4,
            total_registered_bytes: 67108864,
            link_status: RdmaLinkStatus::Active,
            tx_bandwidth_gbps: 41.5,
            rx_bandwidth_gbps: 39.8,
            avg_latency_nanos: 750,
            fallback_to_tcp_active: false,
        };
        let text = render_rdma_status_text(&summary);
        assert!(text.contains("CRAFT RDMA ACCELERATION"));
        assert!(text.contains("41.50 Gbps"));
        assert!(text.contains("750 ns"));
        assert!(text.contains("[OK] NO (Zero-Copy DMA)"));
    }
}
