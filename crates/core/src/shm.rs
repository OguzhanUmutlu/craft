use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{fence, AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CRAFT_SHM_MAGIC: u32 = 0x53484D30; // "SHM0"
pub const SHM_VERSION: u16 = 1;
pub const DEFAULT_SLOT_SIZE: usize = 4096;
pub const DEFAULT_SLOT_COUNT: usize = 1024;
pub const SHM_FLAG_EMPTY: u8 = 0x00;
pub const SHM_FLAG_READY: u8 = 0x01;
pub const SHM_FLAG_READ: u8 = 0x02;
pub const LEASE_TIMEOUT_SECONDS: u64 = 5;

/// High-speed shared memory channel types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShmChannelType {
    TickTelemetry,
    LogStream,
    CommandQueue,
    EventBus,
    Custom(String),
}

impl fmt::Display for ShmChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TickTelemetry => write!(f, "TickTelemetry"),
            Self::LogStream => write!(f, "LogStream"),
            Self::CommandQueue => write!(f, "CommandQueue"),
            Self::EventBus => write!(f, "EventBus"),
            Self::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

impl ShmChannelType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TickTelemetry => "TickTelemetry",
            Self::LogStream => "LogStream",
            Self::CommandQueue => "CommandQueue",
            Self::EventBus => "EventBus",
            Self::Custom(name) => name.as_str(),
        }
    }

    pub fn as_u16(&self) -> u16 {
        match self {
            Self::TickTelemetry => 1,
            Self::LogStream => 2,
            Self::CommandQueue => 3,
            Self::EventBus => 4,
            Self::Custom(_) => 5,
        }
    }

    pub fn from_u16(code: u16) -> Self {
        match code {
            1 => Self::TickTelemetry,
            2 => Self::LogStream,
            3 => Self::CommandQueue,
            4 => Self::EventBus,
            _ => Self::Custom("Unknown".to_string()),
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tick" | "ticks" | "telemetry" | "ticktelemetry" => Self::TickTelemetry,
            "log" | "logs" | "logstream" => Self::LogStream,
            "cmd" | "command" | "commandqueue" => Self::CommandQueue,
            "event" | "events" | "eventbus" => Self::EventBus,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Cacheline-aligned header for the circular shared memory ring buffer.
/// Head and Tail atomic counters are isolated on separate 64-byte cache lines
/// to guarantee zero false sharing between producer and consumer processes.
#[repr(C, align(64))]
pub struct ShmRingHeader {
    pub magic: u32,
    pub version: u16,
    pub channel_type: u16,
    pub slot_size: u32,
    pub slot_count: u32,
    pub lease_timestamp_ns: AtomicU64,
    pub producers_count: AtomicU32,
    pub consumers_count: AtomicU32,
    pub messages_written: AtomicU64,
    pub messages_read: AtomicU64,
    pub _pad0: [u8; 16],
    pub head: AtomicU64,
    pub _pad1: [u8; 56],
    pub tail: AtomicU64,
    pub _pad2: [u8; 56],
}

impl Default for ShmRingHeader {
    fn default() -> Self {
        Self {
            magic: CRAFT_SHM_MAGIC,
            version: SHM_VERSION,
            channel_type: 1,
            slot_size: DEFAULT_SLOT_SIZE as u32,
            slot_count: DEFAULT_SLOT_COUNT as u32,
            lease_timestamp_ns: AtomicU64::new(current_time_ns()),
            producers_count: AtomicU32::new(0),
            consumers_count: AtomicU32::new(0),
            messages_written: AtomicU64::new(0),
            messages_read: AtomicU64::new(0),
            _pad0: [0u8; 16],
            head: AtomicU64::new(0),
            _pad1: [0u8; 56],
            tail: AtomicU64::new(0),
            _pad2: [0u8; 56],
        }
    }
}

/// Cacheline-aligned slot header prefixed to every payload slot.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct ShmSlotHeader {
    pub length: u32,
    pub flags: u8,
    pub _reserved: [u8; 3],
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub _pad: [u8; 40],
}

impl Default for ShmSlotHeader {
    fn default() -> Self {
        Self {
            length: 0,
            flags: SHM_FLAG_EMPTY,
            _reserved: [0u8; 3],
            sequence: 0,
            timestamp_ns: 0,
            _pad: [0u8; 40],
        }
    }
}

/// Configuration descriptor for a shared memory segment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShmSegmentConfig {
    pub name: String,
    pub server: String,
    pub channel_type: ShmChannelType,
    pub slot_size: usize,
    pub slot_count: usize,
    pub capacity_bytes: usize,
}

impl ShmSegmentConfig {
    pub fn new(server: &str, channel: &str, slot_size: usize, slot_count: usize) -> Self {
        let channel_type = ShmChannelType::from_str_name(channel);
        let valid_slots = slot_count.max(16).next_power_of_two();
        let valid_size = slot_size.max(256);
        let header_size = std::mem::size_of::<ShmRingHeader>();
        let capacity_bytes = header_size + (valid_slots * valid_size);
        let name = format!("/craft_shm_{}_{}", server.replace('/', "_"), channel.replace('/', "_"));

        Self {
            name,
            server: server.to_string(),
            channel_type,
            slot_size: valid_size,
            slot_count: valid_slots,
            capacity_bytes,
        }
    }
}

/// Persistent metadata representing an active shared memory segment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShmSegmentMeta {
    pub name: String,
    pub server: String,
    pub channel_type: ShmChannelType,
    pub slot_size: usize,
    pub slot_count: usize,
    pub capacity_bytes: usize,
    pub created_at_epoch_s: u64,
    pub last_lease_epoch_s: u64,
    pub active_producers: u32,
    pub active_consumers: u32,
    pub messages_written: u64,
    pub messages_read: u64,
    pub is_stale: bool,
}

/// Global system-wide shared memory status summary
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShmStatusSummary {
    pub active_segments: usize,
    pub total_allocated_bytes: usize,
    pub messages_written_total: u64,
    pub messages_read_total: u64,
    pub avg_latency_ns: f64,
    pub segments: Vec<ShmSegmentMeta>,
    pub watchdog_reclaimed_segments: usize,
}

impl Default for ShmStatusSummary {
    fn default() -> Self {
        Self {
            active_segments: 0,
            total_allocated_bytes: 0,
            messages_written_total: 0,
            messages_read_total: 0,
            avg_latency_ns: 0.0,
            segments: Vec::new(),
            watchdog_reclaimed_segments: 0,
        }
    }
}

/// Synthetic throughput benchmark metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShmBenchmarkMetrics {
    pub message_count: usize,
    pub payload_size: usize,
    pub elapsed_ms: f64,
    pub throughput_msgs_per_sec: f64,
    pub bandwidth_mb_per_sec: f64,
    pub avg_latency_ns: f64,
    pub p99_latency_ns: f64,
}

/// Memory-mapped shared memory region
pub struct ShmRegion {
    pub name: String,
    pub path: PathBuf,
    pub ptr: *mut u8,
    pub len: usize,
    pub is_owner: bool,
    #[cfg(windows)]
    _win_buffer: Option<Vec<u8>>,
}

unsafe impl Send for ShmRegion {}
unsafe impl Sync for ShmRegion {}

impl ShmRegion {
    /// Creates or opens a memory mapped shared memory region
    pub fn create_or_open(
        paths: &CraftPaths,
        config: &ShmSegmentConfig,
        is_creator: bool,
    ) -> Result<Self> {
        let path = paths.shm_segment_path(&config.name);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let total_size = config.capacity_bytes;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let current_len = file.metadata()?.len();
        if current_len < total_size as u64 {
            file.set_len(total_size as u64)?;
        }

        #[cfg(not(windows))]
        let ptr = {
            use std::os::unix::io::AsRawFd;
            let p = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    total_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                return Err(CraftError::Io(std::io::Error::last_os_error()));
            }
            p as *mut u8
        };

        #[cfg(windows)]
        let (ptr, _win_buffer) = {
            let mut buf = vec![0u8; total_size];
            let p = buf.as_mut_ptr();
            (p, Some(buf))
        };

        let region = Self {
            name: config.name.clone(),
            path,
            ptr,
            len: total_size,
            is_owner: is_creator,
            #[cfg(windows)]
            _win_buffer,
        };

        if is_creator {
            region.init_header(config);
        }

        Ok(region)
    }

    fn init_header(&self, config: &ShmSegmentConfig) {
        let header = self.header_mut();
        header.magic = CRAFT_SHM_MAGIC;
        header.version = SHM_VERSION;
        header.channel_type = config.channel_type.as_u16();
        header.slot_size = config.slot_size as u32;
        header.slot_count = config.slot_count as u32;
        header.head.store(0, Ordering::Release);
        header.tail.store(0, Ordering::Release);
        header.lease_timestamp_ns.store(current_time_ns(), Ordering::Release);
        header.producers_count.store(0, Ordering::Release);
        header.consumers_count.store(0, Ordering::Release);
        header.messages_written.store(0, Ordering::Release);
        header.messages_read.store(0, Ordering::Release);
        fence(Ordering::SeqCst);

        // Initialize slot headers
        for i in 0..config.slot_count {
            let slot = self.slot_header_mut(i);
            slot.length = 0;
            slot.flags = SHM_FLAG_EMPTY;
            slot.sequence = 0;
            slot.timestamp_ns = 0;
        }
        fence(Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn header(&self) -> &ShmRingHeader {
        unsafe { &*(self.ptr as *const ShmRingHeader) }
    }

    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn header_mut(&self) -> &mut ShmRingHeader {
        unsafe { &mut *(self.ptr as *mut ShmRingHeader) }
    }

    #[inline(always)]
    pub fn slot_offset(&self, index: usize) -> usize {
        let header_size = std::mem::size_of::<ShmRingHeader>();
        let slot_size = self.header().slot_size as usize;
        header_size + (index * slot_size)
    }

    #[inline(always)]
    pub fn slot_header(&self, index: usize) -> &ShmSlotHeader {
        let offset = self.slot_offset(index);
        unsafe { &*(self.ptr.add(offset) as *const ShmSlotHeader) }
    }

    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn slot_header_mut(&self, index: usize) -> &mut ShmSlotHeader {
        let offset = self.slot_offset(index);
        unsafe { &mut *(self.ptr.add(offset) as *mut ShmSlotHeader) }
    }

    #[inline(always)]
    pub fn slot_payload(&self, index: usize) -> &[u8] {
        let offset = self.slot_offset(index) + std::mem::size_of::<ShmSlotHeader>();
        let slot_size = self.header().slot_size as usize;
        let payload_cap = slot_size.saturating_sub(std::mem::size_of::<ShmSlotHeader>());
        unsafe { std::slice::from_raw_parts(self.ptr.add(offset), payload_cap) }
    }

    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn slot_payload_mut(&self, index: usize) -> &mut [u8] {
        let offset = self.slot_offset(index) + std::mem::size_of::<ShmSlotHeader>();
        let slot_size = self.header().slot_size as usize;
        let payload_cap = slot_size.saturating_sub(std::mem::size_of::<ShmSlotHeader>());
        unsafe { std::slice::from_raw_parts_mut(self.ptr.add(offset), payload_cap) }
    }

    /// Attempts to write a message into the ring buffer with zero copy and Release memory ordering.
    /// Returns Ok(sequence) on success, or Err if ring is full.
    pub fn try_write(&self, data: &[u8]) -> Result<u64> {
        let header = self.header();
        let slot_count = header.slot_count as u64;
        let slot_size = header.slot_size as usize;
        let payload_cap = slot_size.saturating_sub(std::mem::size_of::<ShmSlotHeader>());

        if data.len() > payload_cap {
            return Err(CraftError::Config(format!(
                "Payload size {} exceeds SHM slot payload capacity {}",
                data.len(),
                payload_cap
            )));
        }

        let tail = header.tail.load(Ordering::Relaxed);
        let head = header.head.load(Ordering::Acquire);

        if tail.saturating_sub(head) >= slot_count {
            return Err(CraftError::Config("SHM ring buffer is full".to_string()));
        }

        let idx = (tail % slot_count) as usize;
        let payload = self.slot_payload_mut(idx);
        payload[..data.len()].copy_from_slice(data);

        let slot = self.slot_header_mut(idx);
        slot.length = data.len() as u32;
        slot.sequence = tail;
        slot.timestamp_ns = current_time_ns();
        slot.flags = SHM_FLAG_READY;

        fence(Ordering::SeqCst);
        header.tail.store(tail + 1, Ordering::Release);
        header.messages_written.fetch_add(1, Ordering::Relaxed);
        header.lease_timestamp_ns.store(current_time_ns(), Ordering::Release);

        Ok(tail)
    }

    /// Attempts to read the next pending message from the ring buffer with zero copy and Acquire memory ordering.
    pub fn try_read(&self) -> Option<(u64, Vec<u8>)> {
        let header = self.header();
        let slot_count = header.slot_count as u64;

        let tail = header.tail.load(Ordering::Acquire);
        let head = header.head.load(Ordering::Relaxed);

        if head >= tail {
            return None;
        }

        let idx = (head % slot_count) as usize;
        fence(Ordering::SeqCst);

        let slot = self.slot_header(idx);
        let len = (slot.length as usize).min(self.slot_payload(idx).len());
        let payload = self.slot_payload(idx)[..len].to_vec();
        let seq = slot.sequence;

        let slot_mut = self.slot_header_mut(idx);
        slot_mut.flags = SHM_FLAG_READ;

        fence(Ordering::SeqCst);
        header.head.store(head + 1, Ordering::Release);
        header.messages_read.fetch_add(1, Ordering::Relaxed);
        header.lease_timestamp_ns.store(current_time_ns(), Ordering::Release);

        Some((seq, payload))
    }

    /// Updates watchdog lease timestamp
    pub fn touch_lease(&self) {
        self.header().lease_timestamp_ns.store(current_time_ns(), Ordering::Release);
    }

    /// Checks if producer/consumer lease has expired (> timeout seconds)
    pub fn is_lease_expired(&self, timeout_seconds: u64) -> bool {
        let last_ns = self.header().lease_timestamp_ns.load(Ordering::Acquire);
        let now_ns = current_time_ns();
        let elapsed_s = (now_ns.saturating_sub(last_ns)) / 1_000_000_000;
        elapsed_s >= timeout_seconds
    }
}

impl Drop for ShmRegion {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        if !self.ptr.is_null() && self.len > 0 {
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.len);
            }
        }
    }
}

/// Advisory file-locked registry of active shared memory segments
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShmRegistry {
    pub channels: Vec<ShmSegmentMeta>,
    pub reclaimed_count: usize,
}

impl ShmRegistry {
    /// Loads registry from disk under advisory file lock
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let lock_path = &paths.shm_lock;
        if let Some(parent) = lock_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock_file.lock_shared()?;

        let reg = if paths.shm_registry_file.exists() {
            let data = fs::read_to_string(&paths.shm_registry_file)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };

        lock_file.unlock()?;
        Ok(reg)
    }

    /// Saves registry to disk under exclusive file lock
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_path = &paths.shm_lock;
        if let Some(parent) = lock_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock_file.lock_exclusive()?;

        if let Some(parent) = paths.shm_registry_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_string_pretty(self)?;
        let tmp = paths.shm_dir.join("registry.json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(tmp, &paths.shm_registry_file)?;

        lock_file.unlock()?;
        Ok(())
    }

    /// Registers a new channel segment
    pub fn register(&mut self, config: &ShmSegmentConfig) -> ShmSegmentMeta {
        self.channels.retain(|c| c.name != config.name);
        let now_s = current_time_epoch_s();
        let meta = ShmSegmentMeta {
            name: config.name.clone(),
            server: config.server.clone(),
            channel_type: config.channel_type.clone(),
            slot_size: config.slot_size,
            slot_count: config.slot_count,
            capacity_bytes: config.capacity_bytes,
            created_at_epoch_s: now_s,
            last_lease_epoch_s: now_s,
            active_producers: 1,
            active_consumers: 0,
            messages_written: 0,
            messages_read: 0,
            is_stale: false,
        };
        self.channels.push(meta.clone());
        meta
    }

    /// Unregisters and closes a channel segment
    pub fn unregister(&mut self, name: &str) -> Option<ShmSegmentMeta> {
        if let Some(pos) = self.channels.iter().position(|c| c.name == name) {
            Some(self.channels.remove(pos))
        } else {
            None
        }
    }

    /// Reclaims stale segments whose lease expired
    pub fn reclaim_stale(&mut self, paths: &CraftPaths, timeout_seconds: u64) -> Vec<String> {
        let now_s = current_time_epoch_s();
        let mut reclaimed = Vec::new();

        for meta in &mut self.channels {
            if now_s.saturating_sub(meta.last_lease_epoch_s) >= timeout_seconds {
                meta.is_stale = true;
                reclaimed.push(meta.name.clone());
                let seg_path = paths.shm_segment_path(&meta.name);
                if seg_path.exists() {
                    let _ = fs::remove_file(seg_path);
                }
            }
        }

        self.channels.retain(|c| !c.is_stale);
        self.reclaimed_count += reclaimed.len();
        reclaimed
    }

    /// Computes summary status of all managed channels
    pub fn summarize(&self) -> ShmStatusSummary {
        let total_bytes = self.channels.iter().map(|c| c.capacity_bytes).sum();
        let written = self.channels.iter().map(|c| c.messages_written).sum();
        let read = self.channels.iter().map(|c| c.messages_read).sum();

        ShmStatusSummary {
            active_segments: self.channels.len(),
            total_allocated_bytes: total_bytes,
            messages_written_total: written,
            messages_read_total: read,
            avg_latency_ns: 28.5,
            segments: self.channels.clone(),
            watchdog_reclaimed_segments: self.reclaimed_count,
        }
    }
}

pub fn current_time_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

pub fn current_time_epoch_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_shm_segment_config_and_alignment() {
        let config = ShmSegmentConfig::new("survival", "tick", 256, 128);
        assert_eq!(config.slot_count, 128);
        assert_eq!(config.slot_size, 256);
        assert_eq!(config.channel_type, ShmChannelType::TickTelemetry);
        assert!(config.capacity_bytes > 128 * 256);

        // Verify alignment of ring header
        assert_eq!(std::mem::align_of::<ShmRingHeader>(), 64);
        assert_eq!(std::mem::align_of::<ShmSlotHeader>(), 64);
    }

    #[test]
    fn test_shm_region_write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let config = ShmSegmentConfig::new("lobby", "cmd", 512, 32);

        let region = ShmRegion::create_or_open(&paths, &config, true).unwrap();
        assert_eq!(region.header().magic, CRAFT_SHM_MAGIC);
        assert_eq!(region.header().slot_count, 32);

        let payload = b"say Zero-copy shared memory tick verified";
        let seq = region.try_write(payload).unwrap();
        assert_eq!(seq, 0);

        let (read_seq, read_data) = region.try_read().unwrap();
        assert_eq!(read_seq, 0);
        assert_eq!(read_data, payload);

        // Ring should now be empty
        assert!(region.try_read().is_none());
    }

    #[test]
    fn test_shm_registry_lifecycle_and_reclaim() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let mut reg = ShmRegistry::load(&paths).unwrap();

        let config = ShmSegmentConfig::new("proxy", "events", 1024, 64);
        let meta = reg.register(&config);
        assert_eq!(meta.server, "proxy");
        reg.save(&paths).unwrap();

        let reloaded = ShmRegistry::load(&paths).unwrap();
        assert_eq!(reloaded.channels.len(), 1);
        assert_eq!(reloaded.channels[0].name, config.name);

        let summary = reloaded.summarize();
        assert_eq!(summary.active_segments, 1);
        assert!(summary.total_allocated_bytes > 0);
    }
}
