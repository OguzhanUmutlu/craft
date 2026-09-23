use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::str::FromStr;

/// eBPF probe target type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EbpfProbeType {
    SyscallRead,
    SyscallWrite,
    SyscallFutex,
    SyscallEpoll,
    JvmSafepoint,
    JvmGcPause,
    SocketBufferPressure,
    All,
}

impl EbpfProbeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SyscallRead => "syscall_read",
            Self::SyscallWrite => "syscall_write",
            Self::SyscallFutex => "syscall_futex",
            Self::SyscallEpoll => "syscall_epoll",
            Self::JvmSafepoint => "jvm_safepoint",
            Self::JvmGcPause => "jvm_gc_pause",
            Self::SocketBufferPressure => "socket_buffer_pressure",
            Self::All => "all",
        }
    }
}

impl fmt::Display for EbpfProbeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for EbpfProbeType {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "syscall_read" | "read" => Ok(Self::SyscallRead),
            "syscall_write" | "write" => Ok(Self::SyscallWrite),
            "syscall_futex" | "futex" | "lock" => Ok(Self::SyscallFutex),
            "syscall_epoll" | "epoll" => Ok(Self::SyscallEpoll),
            "jvm_safepoint" | "safepoint" => Ok(Self::JvmSafepoint),
            "jvm_gc_pause" | "gc" => Ok(Self::JvmGcPause),
            "socket_buffer_pressure" | "socket" | "net" => Ok(Self::SocketBufferPressure),
            "all" => Ok(Self::All),
            _ => Err(CraftError::Other(format!("Unknown probe type '{}'", s))),
        }
    }
}

/// Operational state of an active or detached eBPF probe
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EbpfProbeStatus {
    Active,
    Detached,
    Paused,
    Error(String),
}

impl EbpfProbeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Detached => "detached",
            Self::Paused => "paused",
            Self::Error(_) => "error",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl fmt::Display for EbpfProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Detached => write!(f, "detached"),
            Self::Paused => write!(f, "paused"),
            Self::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}

/// Descriptor of an attached or managed eBPF probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbpfProbeDescriptor {
    pub id: String,
    pub server_name: String,
    pub pid: u32,
    pub probe_type: EbpfProbeType,
    pub sample_rate_hz: u32,
    pub attached_at: u64,
    pub duration_secs: u64,
    pub event_count: u64,
    pub status: EbpfProbeStatus,
}

impl EbpfProbeDescriptor {
    pub fn new(
        server_name: &str,
        pid: u32,
        probe_type: EbpfProbeType,
        sample_rate_hz: u32,
        duration_secs: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let id = format!("probe-{}-{}", server_name, now);
        Self {
            id,
            server_name: server_name.to_string(),
            pid,
            probe_type,
            sample_rate_hz,
            attached_at: now,
            duration_secs,
            event_count: 0,
            status: EbpfProbeStatus::Active,
        }
    }
}

/// Intercepted Linux syscall event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallInterceptionRecord {
    pub syscall_nr: u32,
    pub syscall_name: String,
    pub duration_ns: u64,
    pub return_code: i64,
    pub thread_id: u32,
    pub timestamp_ns: u64,
}

impl SyscallInterceptionRecord {
    pub fn duration_millis(&self) -> f64 {
        self.duration_ns as f64 / 1_000_000.0
    }
}

/// JVM garbage collection phase classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GcPhase {
    YoungGen,
    OldGen,
    ConcurrentMark,
    Remark,
    FullGc,
    ZgcPause,
    ShenandoahPause,
}

impl GcPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::YoungGen => "young_gen",
            Self::OldGen => "old_gen",
            Self::ConcurrentMark => "concurrent_mark",
            Self::Remark => "remark",
            Self::FullGc => "full_gc",
            Self::ZgcPause => "zgc_pause",
            Self::ShenandoahPause => "shenandoah_pause",
        }
    }

    pub fn is_stop_the_world(&self) -> bool {
        matches!(
            self,
            Self::YoungGen | Self::OldGen | Self::Remark | Self::FullGc | Self::ZgcPause | Self::ShenandoahPause
        )
    }
}

impl fmt::Display for GcPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Recorded JVM GC pause and safepoint synchronization event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmGcEvent {
    pub timestamp_ms: u64,
    pub collector: String,
    pub phase: GcPhase,
    pub pause_duration_ns: u64,
    pub heap_before_bytes: u64,
    pub heap_after_bytes: u64,
    pub safepoint_sync_time_ns: u64,
}

impl JvmGcEvent {
    pub fn pause_ms(&self) -> f64 {
        self.pause_duration_ns as f64 / 1_000_000.0
    }

    pub fn safepoint_sync_ms(&self) -> f64 {
        self.safepoint_sync_time_ns as f64 / 1_000_000.0
    }

    pub fn reclaimed_bytes(&self) -> i64 {
        self.heap_before_bytes as i64 - self.heap_after_bytes as i64
    }
}

/// Captured thread synchronization lock contention frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadContentionFrame {
    pub thread_name: String,
    pub thread_id: u32,
    pub lock_address: String,
    pub contention_duration_ns: u64,
    pub stack_symbols: Vec<String>,
}

impl ThreadContentionFrame {
    pub fn contention_ms(&self) -> f64 {
        self.contention_duration_ns as f64 / 1_000_000.0
    }
}

/// Collapsed hierarchical node representing an aggregated stack frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlameGraphNode {
    pub name: String,
    pub value: u64,
    pub children: Vec<FlameGraphNode>,
    pub percentage: f64,
}

impl FlameGraphNode {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: 0,
            children: Vec::new(),
            percentage: 0.0,
        }
    }

    pub fn find_child_mut(&mut self, name: &str) -> Option<&mut FlameGraphNode> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    pub fn add_or_get_child(&mut self, name: &str) -> &mut FlameGraphNode {
        if let Some(pos) = self.children.iter().position(|c| c.name == name) {
            return &mut self.children[pos];
        }
        self.children.push(FlameGraphNode::new(name));
        self.children.last_mut().unwrap()
    }

    pub fn compute_percentages(&mut self, root_total: u64) {
        if root_total > 0 {
            self.percentage = (self.value as f64 / root_total as f64) * 100.0;
        } else {
            self.percentage = 0.0;
        }
        for child in &mut self.children {
            child.compute_percentages(root_total);
        }
    }
}

/// Thread-safe, advisory file-locked eBPF probe registry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EbpfRegistry {
    #[serde(default)]
    pub probes: HashMap<String, EbpfProbeDescriptor>,
    #[serde(default)]
    pub gc_events: HashMap<String, Vec<JvmGcEvent>>,
}

impl EbpfRegistry {
    /// Loads the eBPF registry under an advisory shared lock
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.ebpf_probes_file.exists() {
            return Ok(Self::default());
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.ebpf_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_shared().map_err(CraftError::Io)?;

        let mut content = String::new();
        let mut file = fs::File::open(&paths.ebpf_probes_file).map_err(CraftError::Io)?;
        file.read_to_string(&mut content).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();

        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse probes.toml: {}", e)))
    }

    /// Saves the eBPF registry atomically under an advisory exclusive lock
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.ebpf_probes_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }
        if let Some(parent) = paths.ebpf_lock.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.ebpf_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_exclusive().map_err(CraftError::Io)?;

        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize eBPF registry: {}", e)))?;

        let tmp_path = paths.ebpf_dir.join(format!(
            "probes.toml.tmp.{}",
            std::process::id()
        ));
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .map_err(CraftError::Io)?;
            file.write_all(content.as_bytes()).map_err(CraftError::Io)?;
            file.flush().map_err(CraftError::Io)?;
        }

        fs::rename(&tmp_path, &paths.ebpf_probes_file).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();
        Ok(())
    }

    pub fn register_probe(&mut self, descriptor: EbpfProbeDescriptor) {
        self.probes.insert(descriptor.id.clone(), descriptor);
    }

    pub fn get_probe(&self, id: &str) -> Option<&EbpfProbeDescriptor> {
        self.probes.get(id)
    }

    pub fn update_status(&mut self, id: &str, status: EbpfProbeStatus) -> bool {
        if let Some(p) = self.probes.get_mut(id) {
            p.status = status;
            true
        } else {
            false
        }
    }

    pub fn remove_probe(&mut self, id: &str) -> Option<EbpfProbeDescriptor> {
        self.probes.remove(id)
    }

    pub fn record_gc_event(&mut self, server: &str, event: JvmGcEvent) {
        let events = self.gc_events.entry(server.to_string()).or_default();
        events.push(event);
        if events.len() > 1000 {
            events.remove(0);
        }
    }

    pub fn get_gc_events(&self, server: &str, limit: usize) -> Vec<JvmGcEvent> {
        if let Some(events) = self.gc_events.get(server) {
            let start = events.len().saturating_sub(limit);
            events[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    pub fn active_probes(&self) -> Vec<&EbpfProbeDescriptor> {
        self.probes.values().filter(|p| p.status.is_active()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebpf_probe_descriptor_lifecycle() {
        let mut desc = EbpfProbeDescriptor::new(
            "survival-01",
            1234,
            EbpfProbeType::SyscallRead,
            99,
            30,
        );
        assert_eq!(desc.server_name, "survival-01");
        assert_eq!(desc.pid, 1234);
        assert_eq!(desc.probe_type, EbpfProbeType::SyscallRead);
        assert_eq!(desc.status, EbpfProbeStatus::Active);

        desc.status = EbpfProbeStatus::Paused;
        assert_eq!(desc.status.to_string(), "paused");
    }

    #[test]
    fn test_gc_event_calculations() {
        let event = JvmGcEvent {
            timestamp_ms: 1727136000000,
            collector: "G1".to_string(),
            phase: GcPhase::YoungGen,
            pause_duration_ns: 25_000_000,
            heap_before_bytes: 4_000_000_000,
            heap_after_bytes: 2_500_000_000,
            safepoint_sync_time_ns: 2_000_000,
        };

        assert_eq!(event.pause_ms(), 25.0);
        assert_eq!(event.safepoint_sync_ms(), 2.0);
        assert_eq!(event.reclaimed_bytes(), 1_500_000_000);
        assert!(event.phase.is_stop_the_world());
    }

    #[test]
    fn test_flamegraph_node_aggregation() {
        let mut root = FlameGraphNode::new("root");
        root.value = 100;

        let server_thread = root.add_or_get_child("Server thread");
        server_thread.value = 80;

        let tick = server_thread.add_or_get_child("MinecraftServer.tick()");
        tick.value = 60;

        root.compute_percentages(100);

        assert_eq!(root.percentage, 100.0);
        assert_eq!(root.children[0].percentage, 80.0);
        assert_eq!(root.children[0].children[0].percentage, 60.0);
    }
}
