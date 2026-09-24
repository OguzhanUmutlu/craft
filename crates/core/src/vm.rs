use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VMADDR_CID_HYPERVISOR: u32 = 0;
pub const VMADDR_CID_LOCAL: u32 = 1;
pub const VMADDR_CID_HOST: u32 = 2;
pub const VMADDR_CID_GUEST_MIN: u32 = 3;

pub const CVSK_MAGIC: [u8; 4] = [0x43, 0x56, 0x53, 0x4B]; // "CVSK"

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtioDeviceType {
    Net,
    Block,
    Vsock,
    Console,
}

impl fmt::Display for VirtioDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Net => write!(f, "Net"),
            Self::Block => write!(f, "Block"),
            Self::Vsock => write!(f, "Vsock"),
            Self::Console => write!(f, "Console"),
        }
    }
}

impl VirtioDeviceType {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "net" | "network" | "virtio-net" => Some(Self::Net),
            "block" | "blk" | "disk" | "virtio-block" => Some(Self::Block),
            "vsock" | "vhost-vsock" | "virtio-vsock" => Some(Self::Vsock),
            "console" | "serial" | "virtio-console" => Some(Self::Console),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicroVmState {
    Created,
    Booting,
    Running,
    Suspended,
    Terminated,
    Failed(String),
}

impl fmt::Display for MicroVmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Booting => write!(f, "Booting"),
            Self::Running => write!(f, "Running"),
            Self::Suspended => write!(f, "Suspended"),
            Self::Terminated => write!(f, "Terminated"),
            Self::Failed(err) => write!(f, "Failed({})", err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeccompLevel {
    Disabled,
    Basic,
    Strict,
}

impl fmt::Display for SeccompLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "Disabled"),
            Self::Basic => write!(f, "Basic"),
            Self::Strict => write!(f, "Strict"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KvmExitReason {
    Unknown = 0,
    Exception = 1,
    IoIn = 2,
    IoOut = 3,
    Hypercall = 4,
    MmioRead = 5,
    MmioWrite = 6,
    Shutdown = 7,
    SystemEvent = 8,
}

impl fmt::Display for KvmExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Exception => write!(f, "Exception"),
            Self::IoIn => write!(f, "IoIn"),
            Self::IoOut => write!(f, "IoOut"),
            Self::Hypercall => write!(f, "Hypercall"),
            Self::MmioRead => write!(f, "MmioRead"),
            Self::MmioWrite => write!(f, "MmioWrite"),
            Self::Shutdown => write!(f, "Shutdown"),
            Self::SystemEvent => write!(f, "SystemEvent"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvmUserMemoryRegion {
    pub slot: u32,
    pub flags: u32,
    pub guest_phys_addr: u64,
    pub memory_size: u64,
    pub userspace_addr: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KvmRegs {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KvmSregs {
    pub cr0: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub efer: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtioQueue {
    pub queue_size: u16,
    pub desc_table_addr: u64,
    pub avail_ring_addr: u64,
    pub used_ring_addr: u64,
    pub last_avail_idx: u16,
    pub last_used_idx: u16,
}

impl Default for VirtioQueue {
    fn default() -> Self {
        Self {
            queue_size: 256,
            desc_table_addr: 0x1000,
            avail_ring_addr: 0x2000,
            used_ring_addr: 0x3000,
            last_avail_idx: 0,
            last_used_idx: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VirtioDescriptor {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum VsockOp {
    Request = 1,
    Response = 2,
    Rst = 3,
    Shutdown = 4,
    StreamData = 5,
    CreditUpdate = 6,
    CreditRequest = 7,
}

impl fmt::Display for VsockOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request => write!(f, "Request"),
            Self::Response => write!(f, "Response"),
            Self::Rst => write!(f, "Rst"),
            Self::Shutdown => write!(f, "Shutdown"),
            Self::StreamData => write!(f, "StreamData"),
            Self::CreditUpdate => write!(f, "CreditUpdate"),
            Self::CreditRequest => write!(f, "CreditRequest"),
        }
    }
}

impl VsockOp {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            1 => Some(Self::Request),
            2 => Some(Self::Response),
            3 => Some(Self::Rst),
            4 => Some(Self::Shutdown),
            5 => Some(Self::StreamData),
            6 => Some(Self::CreditUpdate),
            7 => Some(Self::CreditRequest),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsockAddr {
    pub cid: u32,
    pub port: u32,
}

impl fmt::Display for VsockAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cid:{}:port:{}", self.cid, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsockPacketHeader {
    pub magic: [u8; 4],
    pub src_cid: u32,
    pub dst_cid: u32,
    pub src_port: u32,
    pub dst_port: u32,
    pub len: u32,
    pub op_type: VsockOp,
    pub flags: u32,
    pub buf_alloc: u32,
    pub fwd_cnt: u32,
}

impl VsockPacketHeader {
    pub fn new(
        src: VsockAddr,
        dst: VsockAddr,
        op_type: VsockOp,
        len: u32,
        buf_alloc: u32,
        fwd_cnt: u32,
    ) -> Self {
        Self {
            magic: CVSK_MAGIC,
            src_cid: src.cid,
            dst_cid: dst.cid,
            src_port: src.port,
            dst_port: dst.port,
            len,
            op_type,
            flags: 0,
            buf_alloc,
            fwd_cnt,
        }
    }

    pub fn encode(&self, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(36 + payload.len());
        buf.extend_from_slice(&self.magic);
        buf.extend_from_slice(&self.src_cid.to_le_bytes());
        buf.extend_from_slice(&self.dst_cid.to_le_bytes());
        buf.extend_from_slice(&self.src_port.to_le_bytes());
        buf.extend_from_slice(&self.dst_port.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.op_type as u16).to_le_bytes());
        buf.extend_from_slice(&[0u8; 2]); // reserved/alignment
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.buf_alloc.to_le_bytes());
        buf.extend_from_slice(&self.fwd_cnt.to_le_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, Vec<u8>)> {
        if bytes.len() < 36 {
            return Err(CraftError::Other(format!(
                "Invalid VSOCK packet header: expected >= 36 bytes, got {}",
                bytes.len()
            )));
        }
        if &bytes[0..4] != &CVSK_MAGIC {
            return Err(CraftError::Other(
                "Invalid VSOCK packet magic bytes (expected CVSK)".to_string(),
            ));
        }
        let src_cid = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let dst_cid = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let src_port = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let dst_port = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        let op_code = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
        let op_type = VsockOp::from_u16(op_code).ok_or_else(|| {
            CraftError::Other(format!("Unknown VSOCK op code: {}", op_code))
        })?;
        let flags = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let buf_alloc = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        let fwd_cnt = if bytes.len() >= 40 {
            u32::from_le_bytes(bytes[36..40].try_into().unwrap())
        } else {
            0
        };

        let header_size = if bytes.len() >= 40 { 40 } else { 36 };
        if bytes.len() < header_size + len {
            return Err(CraftError::Other(format!(
                "Truncated VSOCK payload: expected {} bytes, got {}",
                header_size + len,
                bytes.len()
            )));
        }

        let payload = bytes[header_size..header_size + len].to_vec();
        let header = Self {
            magic: CVSK_MAGIC,
            src_cid,
            dst_cid,
            src_port,
            dst_port,
            len: len as u32,
            op_type,
            flags,
            buf_alloc,
            fwd_cnt,
        };
        Ok((header, payload))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroVmConfig {
    pub vm_id: String,
    pub name: String,
    pub vcpus: u32,
    pub memory_mb: u64,
    pub kernel_path: Option<PathBuf>,
    pub rootfs_path: Option<PathBuf>,
    pub vsock_cid: u32,
    pub virtio_devices: Vec<VirtioDeviceType>,
    pub jailer_uid: Option<u32>,
    pub jailer_gid: Option<u32>,
    pub seccomp_level: SeccompLevel,
}

impl Default for MicroVmConfig {
    fn default() -> Self {
        Self {
            vm_id: "vm-default".to_string(),
            name: "default".to_string(),
            vcpus: 1,
            memory_mb: 256,
            kernel_path: None,
            rootfs_path: None,
            vsock_cid: VMADDR_CID_GUEST_MIN,
            virtio_devices: vec![VirtioDeviceType::Net, VirtioDeviceType::Vsock],
            jailer_uid: Some(1001),
            jailer_gid: Some(1001),
            seccomp_level: SeccompLevel::Basic,
        }
    }
}

impl MicroVmConfig {
    pub fn validate(&self) -> Result<()> {
        if self.vm_id.trim().is_empty() {
            return Err(CraftError::Other("MicroVM ID cannot be empty".to_string()));
        }
        if self.vcpus == 0 {
            return Err(CraftError::Other("MicroVM vCPUs must be at least 1".to_string()));
        }
        if self.memory_mb < 32 {
            return Err(CraftError::Other("MicroVM memory must be at least 32 MB".to_string()));
        }
        if self.vsock_cid < VMADDR_CID_GUEST_MIN {
            return Err(CraftError::Other(format!(
                "MicroVM VSOCK CID must be >= {} (got {})",
                VMADDR_CID_GUEST_MIN, self.vsock_cid
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicroVmDescriptor {
    pub config: MicroVmConfig,
    pub state: MicroVmState,
    pub pid: Option<u32>,
    pub boot_time_ms: f64,
    pub allocated_memory_bytes: u64,
    pub created_at: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MicroVmStatusSummary {
    pub active_vms: usize,
    pub total_vcpus: u32,
    pub total_memory_mb: u64,
    pub avg_boot_time_ms: f64,
    pub kvm_available: bool,
    pub kvm_api_version: u32,
    pub vms: Vec<MicroVmDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MicroVmBenchmarkMetrics {
    pub concurrency: usize,
    pub total_boots: usize,
    pub avg_cold_start_ms: f64,
    pub p50_cold_start_ms: f64,
    pub p95_cold_start_ms: f64,
    pub p99_cold_start_ms: f64,
    pub vsock_throughput_msgs_sec: f64,
    pub vsock_bandwidth_mb_sec: f64,
    pub vsock_latency_micros: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvmCapability {
    pub available: bool,
    pub api_version: u32,
    pub max_vcpus: u32,
    pub user_memory_region_supported: bool,
    pub is_mock: bool,
}

impl Default for KvmCapability {
    fn default() -> Self {
        Self::detect()
    }
}

impl KvmCapability {
    pub fn detect() -> Self {
        let kvm_dev = Path::new("/dev/kvm");
        if kvm_dev.exists() {
            Self {
                available: true,
                api_version: 12,
                max_vcpus: 128,
                user_memory_region_supported: true,
                is_mock: false,
            }
        } else {
            Self {
                available: false,
                api_version: 12,
                max_vcpus: 32,
                user_memory_region_supported: true,
                is_mock: true,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MicroVmRegistryData {
    pub vms: Vec<MicroVmDescriptor>,
    pub cumulative_spawned: u64,
    pub cumulative_terminated: u64,
}

pub struct MicroVmRegistry {
    registry_path: PathBuf,
    lock_path: PathBuf,
}

impl MicroVmRegistry {
    pub fn new(paths: &CraftPaths) -> Self {
        Self {
            registry_path: paths.vm_registry_file.clone(),
            lock_path: paths.vm_lock.clone(),
        }
    }

    pub fn with_paths(registry_path: PathBuf, lock_path: PathBuf) -> Self {
        Self {
            registry_path,
            lock_path,
        }
    }

    fn lock_file(&self) -> Result<File> {
        if let Some(parent) = self.lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)?;
        file.lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire VM lock: {}", e)))?;
        Ok(file)
    }

    pub fn load_data(&self) -> Result<MicroVmRegistryData> {
        let _guard = self.lock_file()?;
        if !self.registry_path.exists() {
            return Ok(MicroVmRegistryData::default());
        }
        let content = fs::read_to_string(&self.registry_path)?;
        if content.trim().is_empty() {
            return Ok(MicroVmRegistryData::default());
        }
        serde_json::from_str(&content)
            .map_err(|e| CraftError::Other(format!("Failed to parse VM registry: {}", e)))
    }

    pub fn save_data(&self, data: &MicroVmRegistryData) -> Result<()> {
        let _guard = self.lock_file()?;
        if let Some(parent) = self.registry_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| CraftError::Other(format!("Failed to serialize VM registry: {}", e)))?;
        let mut tmp_path = self.registry_path.clone();
        tmp_path.set_extension("tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &self.registry_path)?;
        Ok(())
    }

    pub fn upsert_vm(&self, desc: MicroVmDescriptor) -> Result<()> {
        let _guard = self.lock_file()?;
        let mut data = self.load_data_internal()?;
        if let Some(pos) = data.vms.iter().position(|v| v.config.vm_id == desc.config.vm_id) {
            data.vms[pos] = desc;
        } else {
            data.cumulative_spawned += 1;
            data.vms.push(desc);
        }
        self.save_data_internal(&data)
    }

    pub fn get_vm(&self, vm_id: &str) -> Result<Option<MicroVmDescriptor>> {
        let data = self.load_data()?;
        Ok(data.vms.into_iter().find(|v| v.config.vm_id == vm_id || v.config.name == vm_id))
    }

    pub fn remove_vm(&self, vm_id: &str) -> Result<bool> {
        let _guard = self.lock_file()?;
        let mut data = self.load_data_internal()?;
        let initial_len = data.vms.len();
        data.vms.retain(|v| v.config.vm_id != vm_id && v.config.name != vm_id);
        if data.vms.len() < initial_len {
            data.cumulative_terminated += 1;
            self.save_data_internal(&data)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn list_vms(&self) -> Result<Vec<MicroVmDescriptor>> {
        let data = self.load_data()?;
        Ok(data.vms)
    }

    pub fn get_status_summary(&self) -> Result<MicroVmStatusSummary> {
        let data = self.load_data()?;
        let kvm = KvmCapability::detect();
        let active_vms = data.vms.iter().filter(|v| v.state == MicroVmState::Running).count();
        let total_vcpus = data.vms.iter().filter(|v| v.state == MicroVmState::Running).map(|v| v.config.vcpus).sum();
        let total_memory_mb = data.vms.iter().filter(|v| v.state == MicroVmState::Running).map(|v| v.config.memory_mb).sum();
        let boot_times: Vec<f64> = data.vms.iter().map(|v| v.boot_time_ms).filter(|&t| t > 0.0).collect();
        let avg_boot_time_ms = if boot_times.is_empty() {
            0.0
        } else {
            boot_times.iter().sum::<f64>() / boot_times.len() as f64
        };

        Ok(MicroVmStatusSummary {
            active_vms,
            total_vcpus,
            total_memory_mb,
            avg_boot_time_ms,
            kvm_available: kvm.available,
            kvm_api_version: kvm.api_version,
            vms: data.vms,
        })
    }

    pub fn reset_metrics(&self, vm_id: Option<&str>) -> Result<bool> {
        let _guard = self.lock_file()?;
        let mut data = self.load_data_internal()?;
        if let Some(id) = vm_id {
            if let Some(vm) = data.vms.iter_mut().find(|v| v.config.vm_id == id || v.config.name == id) {
                vm.boot_time_ms = 0.0;
                vm.uptime_seconds = 0;
            } else {
                return Ok(false);
            }
        } else {
            for vm in &mut data.vms {
                vm.boot_time_ms = 0.0;
                vm.uptime_seconds = 0;
            }
        }
        self.save_data_internal(&data)?;
        Ok(true)
    }

    fn load_data_internal(&self) -> Result<MicroVmRegistryData> {
        if !self.registry_path.exists() {
            return Ok(MicroVmRegistryData::default());
        }
        let content = fs::read_to_string(&self.registry_path)?;
        if content.trim().is_empty() {
            return Ok(MicroVmRegistryData::default());
        }
        serde_json::from_str(&content)
            .map_err(|e| CraftError::Other(format!("Failed to parse VM registry: {}", e)))
    }

    fn save_data_internal(&self, data: &MicroVmRegistryData) -> Result<()> {
        if let Some(parent) = self.registry_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| CraftError::Other(format!("Failed to serialize VM registry: {}", e)))?;
        let mut tmp_path = self.registry_path.clone();
        tmp_path.set_extension("tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &self.registry_path)?;
        Ok(())
    }
}

pub fn now_epoch_secs() -> u64 {
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
    fn test_virtio_device_types() {
        assert_eq!(VirtioDeviceType::from_str_opt("net"), Some(VirtioDeviceType::Net));
        assert_eq!(VirtioDeviceType::from_str_opt("block"), Some(VirtioDeviceType::Block));
        assert_eq!(VirtioDeviceType::from_str_opt("vsock"), Some(VirtioDeviceType::Vsock));
        assert_eq!(VirtioDeviceType::from_str_opt("console"), Some(VirtioDeviceType::Console));
        assert_eq!(VirtioDeviceType::from_str_opt("unknown"), None);
    }

    #[test]
    fn test_vsock_packet_encoding_decoding() {
        let src = VsockAddr { cid: 2, port: 1024 };
        let dst = VsockAddr { cid: 4, port: 5201 };
        let payload = b"GET /health HTTP/1.1\r\n\r\n";
        let header = VsockPacketHeader::new(src, dst, VsockOp::Request, payload.len() as u32, 65536, 0);

        let encoded = header.encode(payload);
        assert!(encoded.len() >= 36 + payload.len());

        let (decoded_hdr, decoded_payload) = VsockPacketHeader::decode(&encoded).expect("Decode should succeed");
        assert_eq!(decoded_hdr.src_cid, 2);
        assert_eq!(decoded_hdr.dst_cid, 4);
        assert_eq!(decoded_hdr.src_port, 1024);
        assert_eq!(decoded_hdr.dst_port, 5201);
        assert_eq!(decoded_hdr.op_type, VsockOp::Request);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_microvm_config_validation() {
        let mut cfg = MicroVmConfig::default();
        assert!(cfg.validate().is_ok());

        cfg.vcpus = 0;
        assert!(cfg.validate().is_err());

        cfg.vcpus = 2;
        cfg.memory_mb = 16;
        assert!(cfg.validate().is_err());

        cfg.memory_mb = 512;
        cfg.vsock_cid = 1;
        assert!(cfg.validate().is_err());

        cfg.vsock_cid = 3;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_microvm_registry_crud() {
        let dir = tempdir().unwrap();
        let reg_file = dir.path().join("registry.json");
        let lock_file = dir.path().join("vm.lock");
        let registry = MicroVmRegistry::with_paths(reg_file, lock_file);

        let cfg = MicroVmConfig {
            vm_id: "vm-101".to_string(),
            name: "lobby-worker".to_string(),
            vcpus: 2,
            memory_mb: 512,
            kernel_path: None,
            rootfs_path: None,
            vsock_cid: 4,
            virtio_devices: vec![VirtioDeviceType::Net, VirtioDeviceType::Vsock],
            jailer_uid: Some(1001),
            jailer_gid: Some(1001),
            seccomp_level: SeccompLevel::Strict,
        };

        let desc = MicroVmDescriptor {
            config: cfg.clone(),
            state: MicroVmState::Running,
            pid: Some(9999),
            boot_time_ms: 18.5,
            allocated_memory_bytes: 512 * 1024 * 1024,
            created_at: 1000,
            uptime_seconds: 60,
        };

        registry.upsert_vm(desc.clone()).unwrap();

        let fetched = registry.get_vm("vm-101").unwrap().expect("VM should exist");
        assert_eq!(fetched.config.name, "lobby-worker");
        assert_eq!(fetched.state, MicroVmState::Running);

        let summary = registry.get_status_summary().unwrap();
        assert_eq!(summary.active_vms, 1);
        assert_eq!(summary.total_vcpus, 2);
        assert_eq!(summary.total_memory_mb, 512);

        let removed = registry.remove_vm("vm-101").unwrap();
        assert!(removed);
        assert!(registry.get_vm("vm-101").unwrap().is_none());
    }
}
