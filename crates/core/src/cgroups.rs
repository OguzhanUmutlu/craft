use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Server scheduling and resource priority tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerPriority {
    /// Ingress proxy gateways (Velocity, BungeeCord, HAProxy) requiring ultra-low latency
    GatewayProxy,
    /// Standard game server world instances (Paper, Purpur, Fabric, Bedrock)
    StandardWorld,
    /// Asynchronous background workers (world pre-generation, chunk rendering, Dynmap)
    BackgroundWorker,
    /// Ephemeral batch compute tasks (backup compression, snapshot export, log indexing)
    BatchTask,
}

impl ServerPriority {
    pub fn name(&self) -> &'static str {
        match self {
            Self::GatewayProxy => "GatewayProxy",
            Self::StandardWorld => "StandardWorld",
            Self::BackgroundWorker => "BackgroundWorker",
            Self::BatchTask => "BatchTask",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "gateway" | "gatewayproxy" | "proxy" => Some(Self::GatewayProxy),
            "standard" | "standardworld" | "world" | "game" => Some(Self::StandardWorld),
            "worker" | "background" | "backgroundworker" => Some(Self::BackgroundWorker),
            "batch" | "batchtask" | "task" => Some(Self::BatchTask),
            _ => None,
        }
    }

    /// Default CFS CPU scheduling weight (1 to 10000, standard is 100)
    pub fn default_cpu_weight(&self) -> u32 {
        match self {
            Self::GatewayProxy => 500,
            Self::StandardWorld => 100,
            Self::BackgroundWorker => 50,
            Self::BatchTask => 20,
        }
    }

    /// Default BFQ / blk-iocost I/O scheduling weight (1 to 10000, standard is 100)
    pub fn default_io_weight(&self) -> u32 {
        match self {
            Self::GatewayProxy => 500,
            Self::StandardWorld => 100,
            Self::BackgroundWorker => 50,
            Self::BatchTask => 20,
        }
    }
}

impl Default for ServerPriority {
    fn default() -> Self {
        Self::StandardWorld
    }
}

/// Resource limit configuration applied to a server's cgroup
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerResourceLimit {
    pub server_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Hard memory ceiling in bytes (written to memory.max)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_max_bytes: Option<u64>,
    /// Soft memory throttling threshold in bytes (written to memory.high)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_high_bytes: Option<u64>,
    /// CPU quota limit percentage (e.g. 100 = 1 full core, 250 = 2.5 cores)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_max_quota: Option<u32>,
    /// CFS / cgroups v2 fair-share weight (1 to 10000, default 100)
    #[serde(default = "default_cpu_weight_100")]
    pub cpu_weight: u32,
    /// BFQ / blk-iocost I/O weight (1 to 10000, default 100)
    #[serde(default = "default_io_weight_100")]
    pub io_weight: u32,
    /// Maximum task/thread limit (written to pids.max)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u32>,
    /// Priority tier for fair-share rebalancing
    #[serde(default)]
    pub priority: ServerPriority,
}

fn default_cpu_weight_100() -> u32 {
    100
}

fn default_io_weight_100() -> u32 {
    100
}

impl ServerResourceLimit {
    pub fn new(server_name: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
            tenant_id: None,
            memory_max_bytes: None,
            memory_high_bytes: None,
            cpu_max_quota: None,
            cpu_weight: 100,
            io_weight: 100,
            pids_max: None,
            priority: ServerPriority::StandardWorld,
        }
    }

    pub fn with_memory_mb(mut self, max_mb: u64, high_mb: Option<u64>) -> Self {
        self.memory_max_bytes = Some(max_mb.saturating_mul(1024 * 1024));
        self.memory_high_bytes = high_mb.map(|h| h.saturating_mul(1024 * 1024));
        self
    }

    pub fn with_cpu_percent(mut self, cpu_percent: u32) -> Self {
        self.cpu_max_quota = Some(cpu_percent);
        self
    }

    pub fn with_priority(mut self, priority: ServerPriority) -> Self {
        self.cpu_weight = priority.default_cpu_weight();
        self.io_weight = priority.default_io_weight();
        self.priority = priority;
        self
    }
}

/// Tenant resource budget policy defining collective caps
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: String,
    pub max_servers: usize,
    pub max_memory_bytes: u64,
    pub max_cpu_percent: u32,
    pub max_storage_bytes: u64,
    #[serde(default)]
    pub allow_burst: bool,
    pub created_at: DateTime<Utc>,
}

impl TenantQuota {
    pub fn new(tenant_id: &str, max_servers: usize, max_memory_mb: u64, max_cpu_percent: u32) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            max_servers,
            max_memory_bytes: max_memory_mb.saturating_mul(1024 * 1024),
            max_cpu_percent,
            max_storage_bytes: 100 * 1024 * 1024 * 1024, // 100 GB default
            allow_burst: false,
            created_at: Utc::now(),
        }
    }
}

/// Real-time snapshot of cgroup v2 accounting statistics
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CgroupStatSnapshot {
    pub cpu_usage_usec: u64,
    pub cpu_user_usec: u64,
    pub cpu_system_usec: u64,
    pub cpu_nr_periods: u64,
    pub cpu_nr_throttled: u64,
    pub cpu_throttled_usec: u64,
    pub cpu_throttle_ratio: f64,
    pub memory_current_bytes: u64,
    pub memory_anon_bytes: u64,
    pub memory_file_bytes: u64,
    pub memory_oom_events: u64,
    pub memory_oom_kill_events: u64,
    pub memory_high_events: u64,
    pub pids_current: u32,
    pub io_rbytes: u64,
    pub io_wbytes: u64,
}

/// Summary report combining server resource limits, live stats, and health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaUsageSummary {
    pub server_name: String,
    pub tenant_id: Option<String>,
    pub active: bool,
    pub pid: Option<u32>,
    pub limits: ServerResourceLimit,
    pub stats: CgroupStatSnapshot,
    pub throttled: bool,
    pub health_indicator: String,
}

/// Pure-Rust Linux cgroups v2 controller driver
#[derive(Debug, Clone)]
pub struct CgroupV2Driver {
    cgroup_root: PathBuf,
    is_mock: bool,
}

impl CgroupV2Driver {
    pub fn new(paths: &CraftPaths) -> Self {
        let kernel_cgroup = PathBuf::from("/sys/fs/cgroup");
        // Test if kernel cgroup v2 is mounted and accessible
        let is_real_cgroup = kernel_cgroup.join("cgroup.controllers").exists();

        if is_real_cgroup {
            let craft_cgroup = kernel_cgroup.join("craft");
            // Check if we can create / write to craft cgroup
            if fs::create_dir_all(&craft_cgroup).is_ok() {
                return Self {
                    cgroup_root: craft_cgroup,
                    is_mock: false,
                };
            }
        }

        // Resilient fallback: userspace mock cgroups hierarchy
        let mock_root = paths.cgroups_dir.join("mock_sys_fs");
        let _ = fs::create_dir_all(&mock_root);
        Self {
            cgroup_root: mock_root,
            is_mock: true,
        }
    }

    pub fn with_root(root: PathBuf, is_mock: bool) -> Self {
        let _ = fs::create_dir_all(&root);
        Self {
            cgroup_root: root,
            is_mock,
        }
    }

    pub fn cgroup_root(&self) -> &Path {
        &self.cgroup_root
    }

    pub fn is_mock(&self) -> bool {
        self.is_mock
    }

    pub fn server_cgroup_dir(&self, server_name: &str) -> PathBuf {
        self.cgroup_root.join(server_name)
    }

    /// Ensures the cgroup directory exists and initializes controllers
    pub fn ensure_cgroup(&self, server_name: &str) -> Result<PathBuf> {
        let dir = self.server_cgroup_dir(server_name);
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(CraftError::Io)?;
        }

        // Initialize parent subtree control if possible
        let parent_control = self.cgroup_root.join("cgroup.subtree_control");
        if parent_control.exists() {
            let _ = fs::write(&parent_control, "+cpu +memory +io +pids");
        }

        Ok(dir)
    }

    /// Attaches an active PID into the server cgroup procs list
    pub fn attach_pid(&self, server_name: &str, pid: u32) -> Result<()> {
        let dir = self.ensure_cgroup(server_name)?;
        let procs_file = dir.join("cgroup.procs");
        fs::write(&procs_file, pid.to_string()).map_err(CraftError::Io)?;
        Ok(())
    }

    /// Applies resource limits into cgroups v2 control files
    pub fn apply_limits(&self, server_name: &str, limits: &ServerResourceLimit) -> Result<()> {
        let dir = self.ensure_cgroup(server_name)?;

        // 1. Memory Max (Hard limit)
        let memory_max_file = dir.join("memory.max");
        let memory_max_val = match limits.memory_max_bytes {
            Some(bytes) => bytes.to_string(),
            None => "max".to_string(),
        };
        fs::write(&memory_max_file, memory_max_val).map_err(CraftError::Io)?;

        // 2. Memory High (Soft throttle threshold)
        let memory_high_file = dir.join("memory.high");
        let memory_high_val = match limits.memory_high_bytes {
            Some(bytes) => bytes.to_string(),
            None => "max".to_string(),
        };
        fs::write(&memory_high_file, memory_high_val).map_err(CraftError::Io)?;

        // 3. CPU Max (Quota and Period: "$QUOTA_USEC $PERIOD_USEC")
        // Base period = 100,000 usec (100ms)
        let cpu_max_file = dir.join("cpu.max");
        let period_usec = 100_000u64;
        let cpu_max_val = match limits.cpu_max_quota {
            Some(percent) => {
                let quota_usec = (percent as u64).saturating_mul(period_usec) / 100;
                format!("{} {}", quota_usec, period_usec)
            }
            None => format!("max {}", period_usec),
        };
        fs::write(&cpu_max_file, cpu_max_val).map_err(CraftError::Io)?;

        // 4. CPU Weight (CFS proportional share: 1 to 10000)
        let cpu_weight_file = dir.join("cpu.weight");
        let weight = limits.cpu_weight.clamp(1, 10000);
        fs::write(&cpu_weight_file, weight.to_string()).map_err(CraftError::Io)?;

        // 5. I/O Weight (BFQ / blk-iocost: 1 to 10000)
        let io_weight_file = dir.join("io.weight");
        let io_w = limits.io_weight.clamp(1, 10000);
        fs::write(&io_weight_file, io_w.to_string()).map_err(CraftError::Io)?;

        // 6. PIDs Max
        let pids_max_file = dir.join("pids.max");
        let pids_val = match limits.pids_max {
            Some(pids) => pids.to_string(),
            None => "max".to_string(),
        };
        fs::write(&pids_max_file, pids_val).map_err(CraftError::Io)?;

        Ok(())
    }

    /// Reads live cgroups v2 statistics from the kernel or mock files
    pub fn read_stats(&self, server_name: &str) -> Result<CgroupStatSnapshot> {
        let dir = self.server_cgroup_dir(server_name);
        if !dir.exists() {
            return Ok(CgroupStatSnapshot::default());
        }

        let mut stats = CgroupStatSnapshot::default();

        // 1. Parse cpu.stat
        let cpu_stat_file = dir.join("cpu.stat");
        if let Ok(content) = fs::read_to_string(&cpu_stat_file) {
            for line in content.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    match k {
                        "usage_usec" => stats.cpu_usage_usec = v.parse().unwrap_or(0),
                        "user_usec" => stats.cpu_user_usec = v.parse().unwrap_or(0),
                        "system_usec" => stats.cpu_system_usec = v.parse().unwrap_or(0),
                        "nr_periods" => stats.cpu_nr_periods = v.parse().unwrap_or(0),
                        "nr_throttled" => stats.cpu_nr_throttled = v.parse().unwrap_or(0),
                        "throttled_usec" => stats.cpu_throttled_usec = v.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            if stats.cpu_usage_usec > 0 {
                stats.cpu_throttle_ratio =
                    stats.cpu_throttled_usec as f64 / stats.cpu_usage_usec as f64;
            }
        }

        // 2. Parse memory.current
        let memory_current_file = dir.join("memory.current");
        if let Ok(content) = fs::read_to_string(&memory_current_file) {
            stats.memory_current_bytes = content.trim().parse().unwrap_or(0);
        }

        // 3. Parse memory.stat
        let memory_stat_file = dir.join("memory.stat");
        if let Ok(content) = fs::read_to_string(&memory_stat_file) {
            for line in content.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    match k {
                        "anon" => stats.memory_anon_bytes = v.parse().unwrap_or(0),
                        "file" => stats.memory_file_bytes = v.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }

        // 4. Parse memory.events
        let memory_events_file = dir.join("memory.events");
        if let Ok(content) = fs::read_to_string(&memory_events_file) {
            for line in content.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    match k {
                        "oom" => stats.memory_oom_events = v.parse().unwrap_or(0),
                        "oom_kill" => stats.memory_oom_kill_events = v.parse().unwrap_or(0),
                        "high" => stats.memory_high_events = v.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }

        // 5. Parse pids.current
        let pids_current_file = dir.join("pids.current");
        if let Ok(content) = fs::read_to_string(&pids_current_file) {
            stats.pids_current = content.trim().parse().unwrap_or(0);
        }

        // 6. Parse io.stat
        let io_stat_file = dir.join("io.stat");
        if let Ok(content) = fs::read_to_string(&io_stat_file) {
            for line in content.lines() {
                for token in line.split_whitespace() {
                    if let Some(rest) = token.strip_prefix("rbytes=") {
                        stats.io_rbytes = rest.parse().unwrap_or(0);
                    } else if let Some(rest) = token.strip_prefix("wbytes=") {
                        stats.io_wbytes = rest.parse().unwrap_or(0);
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Destroys a server's cgroup directory after stopping
    pub fn destroy_cgroup(&self, server_name: &str) -> Result<()> {
        let dir = self.server_cgroup_dir(server_name);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        Ok(())
    }

    /// Lists all active server cgroups currently provisioned
    pub fn list_active_cgroups(&self) -> Vec<String> {
        let mut list = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.cgroup_root) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !name.starts_with('.') && name != "mock_sys_fs" {
                            list.push(name);
                        }
                    }
                }
            }
        }
        list.sort();
        list
    }
}

/// Persistent multi-tenant quota and server resource limits registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRegistry {
    pub tenants: HashMap<String, TenantQuota>,
    pub server_limits: HashMap<String, ServerResourceLimit>,
    #[serde(default)]
    pub overcommit_allowed: bool,
}

impl Default for QuotaRegistry {
    fn default() -> Self {
        let mut tenants = HashMap::new();
        let default_tenant = TenantQuota::new("default", 20, 32768, 800);
        tenants.insert("default".to_string(), default_tenant);

        Self {
            tenants,
            server_limits: HashMap::new(),
            overcommit_allowed: false,
        }
    }
}

impl QuotaRegistry {
    /// Loads the QuotaRegistry under an advisory shared lock
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.quotas_file.exists() {
            let registry = Self::default();
            registry.save(paths)?;
            return Ok(registry);
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.quotas_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_shared().map_err(CraftError::Io)?;

        let mut content = String::new();
        let mut file = File::open(&paths.quotas_file).map_err(CraftError::Io)?;
        file.read_to_string(&mut content).map_err(CraftError::Io)?;

        let _ = lock_file.unlock();

        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse quotas.toml: {}", e)))
    }

    /// Saves the QuotaRegistry atomically under an advisory exclusive lock
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.quotas_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }
        if let Some(parent) = paths.quotas_lock.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.quotas_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_exclusive().map_err(CraftError::Io)?;

        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize quotas: {}", e)))?;

        let tmp_path = paths.quotas_dir.join(format!("quotas.toml.tmp.{}", std::process::id()));
        {
            let mut tmp_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .map_err(CraftError::Io)?;
            tmp_file.write_all(content.as_bytes()).map_err(CraftError::Io)?;
            tmp_file.flush().map_err(CraftError::Io)?;
        }

        fs::rename(&tmp_path, &paths.quotas_file).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();

        Ok(())
    }

    pub fn get_server_limit(&self, server_name: &str) -> Option<&ServerResourceLimit> {
        self.server_limits.get(server_name)
    }

    pub fn get_tenant_quota(&self, tenant_id: &str) -> Option<&TenantQuota> {
        self.tenants.get(tenant_id)
    }

    pub fn set_tenant_quota(&mut self, quota: TenantQuota) {
        self.tenants.insert(quota.tenant_id.clone(), quota);
    }

    /// Calculates current aggregate usage (server count, memory bytes, cpu percent) for a tenant
    pub fn calculate_tenant_usage(&self, tenant_id: &str) -> (usize, u64, u32) {
        let mut count = 0;
        let mut mem_total = 0u64;
        let mut cpu_total = 0u32;

        for limit in self.server_limits.values() {
            if limit.tenant_id.as_deref() == Some(tenant_id) {
                count += 1;
                if let Some(mem) = limit.memory_max_bytes {
                    mem_total = mem_total.saturating_add(mem);
                }
                if let Some(cpu) = limit.cpu_max_quota {
                    cpu_total = cpu_total.saturating_add(cpu);
                }
            }
        }

        (count, mem_total, cpu_total)
    }

    /// Validates whether adding/updating a server limit fits within tenant caps
    pub fn validate_tenant_allocation(
        &self,
        server_name: &str,
        new_limit: &ServerResourceLimit,
    ) -> Result<()> {
        let tenant_id = new_limit.tenant_id.as_deref().unwrap_or("default");
        let tenant = match self.tenants.get(tenant_id) {
            Some(t) => t,
            None => {
                return Err(CraftError::Config(format!(
                    "Tenant '{}' does not exist in QuotaRegistry",
                    tenant_id
                )));
            }
        };

        if self.overcommit_allowed || tenant.allow_burst {
            return Ok(());
        }

        let (mut count, mut mem_total, mut cpu_total) = self.calculate_tenant_usage(tenant_id);

        // Subtract existing server usage if this is an update
        if let Some(existing) = self.server_limits.get(server_name) {
            if existing.tenant_id.as_deref() == Some(tenant_id) {
                count = count.saturating_sub(1);
                if let Some(mem) = existing.memory_max_bytes {
                    mem_total = mem_total.saturating_sub(mem);
                }
                if let Some(cpu) = existing.cpu_max_quota {
                    cpu_total = cpu_total.saturating_sub(cpu);
                }
            }
        }

        let new_count = count + 1;
        if new_count > tenant.max_servers {
            return Err(CraftError::Config(format!(
                "Tenant '{}' server limit exceeded: {} > max {}",
                tenant_id, new_count, tenant.max_servers
            )));
        }

        if let Some(req_mem) = new_limit.memory_max_bytes {
            let prospective_mem = mem_total.saturating_add(req_mem);
            if prospective_mem > tenant.max_memory_bytes {
                let req_mb = req_mem / (1024 * 1024);
                let max_mb = tenant.max_memory_bytes / (1024 * 1024);
                let cur_mb = mem_total / (1024 * 1024);
                return Err(CraftError::Config(format!(
                    "Tenant '{}' memory quota exceeded: requesting {} MB (current {} MB) exceeds cap of {} MB",
                    tenant_id, req_mb, cur_mb, max_mb
                )));
            }
        }

        if let Some(req_cpu) = new_limit.cpu_max_quota {
            let prospective_cpu = cpu_total.saturating_add(req_cpu);
            if prospective_cpu > tenant.max_cpu_percent {
                return Err(CraftError::Config(format!(
                    "Tenant '{}' CPU quota exceeded: requesting {}% (current {}%) exceeds cap of {}%",
                    tenant_id, req_cpu, cpu_total, tenant.max_cpu_percent
                )));
            }
        }

        Ok(())
    }

    /// Sets or updates a server limit after validating tenant budgets
    pub fn set_server_limit(&mut self, limit: ServerResourceLimit) -> Result<()> {
        self.validate_tenant_allocation(&limit.server_name, &limit)?;
        self.server_limits.insert(limit.server_name.clone(), limit);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_priority_weights_and_defaults() {
        let p_proxy = ServerPriority::GatewayProxy;
        let p_world = ServerPriority::StandardWorld;
        let p_worker = ServerPriority::BackgroundWorker;
        let p_batch = ServerPriority::BatchTask;

        assert_eq!(p_proxy.default_cpu_weight(), 500);
        assert_eq!(p_world.default_cpu_weight(), 100);
        assert_eq!(p_worker.default_cpu_weight(), 50);
        assert_eq!(p_batch.default_cpu_weight(), 20);

        assert_eq!(ServerPriority::from_str_opt("proxy"), Some(p_proxy));
        assert_eq!(ServerPriority::from_str_opt("world"), Some(p_world));
        assert_eq!(ServerPriority::from_str_opt("worker"), Some(p_worker));
        assert_eq!(ServerPriority::from_str_opt("batch"), Some(p_batch));
    }

    #[test]
    fn test_cgroup_v2_driver_file_operations() {
        let dir = tempdir().unwrap();
        let driver = CgroupV2Driver::with_root(dir.path().to_path_buf(), true);

        let srv = "test-lobby";
        let cgroup_dir = driver.ensure_cgroup(srv).unwrap();
        assert!(cgroup_dir.exists());

        let limit = ServerResourceLimit::new(srv)
            .with_memory_mb(2048, Some(1536))
            .with_cpu_percent(150)
            .with_priority(ServerPriority::GatewayProxy);

        driver.apply_limits(srv, &limit).unwrap();

        // Verify written files
        let mem_max = fs::read_to_string(cgroup_dir.join("memory.max")).unwrap();
        assert_eq!(mem_max.trim(), (2048u64 * 1024 * 1024).to_string());

        let mem_high = fs::read_to_string(cgroup_dir.join("memory.high")).unwrap();
        assert_eq!(mem_high.trim(), (1536u64 * 1024 * 1024).to_string());

        let cpu_max = fs::read_to_string(cgroup_dir.join("cpu.max")).unwrap();
        assert_eq!(cpu_max.trim(), "150000 100000");

        let cpu_weight = fs::read_to_string(cgroup_dir.join("cpu.weight")).unwrap();
        assert_eq!(cpu_weight.trim(), "500");

        // Write mock stats
        fs::write(
            cgroup_dir.join("cpu.stat"),
            "usage_usec 1000000\nuser_usec 800000\nsystem_usec 200000\nnr_periods 100\nnr_throttled 20\nthrottled_usec 150000\n",
        ).unwrap();
        fs::write(cgroup_dir.join("memory.current"), "1073741824\n").unwrap();
        fs::write(cgroup_dir.join("memory.events"), "oom 0\noom_kill 0\nhigh 1\n").unwrap();

        let stats = driver.read_stats(srv).unwrap();
        assert_eq!(stats.cpu_usage_usec, 1000000);
        assert_eq!(stats.cpu_nr_throttled, 20);
        assert_eq!(stats.cpu_throttled_usec, 150000);
        assert!((stats.cpu_throttle_ratio - 0.15).abs() < 1e-6);
        assert_eq!(stats.memory_current_bytes, 1073741824);
        assert_eq!(stats.memory_high_events, 1);

        driver.destroy_cgroup(srv).unwrap();
        assert!(!cgroup_dir.exists());
    }

    #[test]
    fn test_tenant_quota_budget_validation() {
        let mut reg = QuotaRegistry::default();
        let tenant = TenantQuota::new("tenant-a", 2, 4096, 200);
        reg.set_tenant_quota(tenant);

        let s1 = ServerResourceLimit::new("server-1")
            .with_memory_mb(2048, None)
            .with_cpu_percent(100);
        let mut s1_mod = s1.clone();
        s1_mod.tenant_id = Some("tenant-a".to_string());
        assert!(reg.set_server_limit(s1_mod).is_ok());

        // Fits within cap: 2048 + 1024 = 3072 <= 4096, 100 + 50 = 150 <= 200
        let s2 = ServerResourceLimit::new("server-2")
            .with_memory_mb(1024, None)
            .with_cpu_percent(50);
        let mut s2_mod = s2.clone();
        s2_mod.tenant_id = Some("tenant-a".to_string());
        assert!(reg.set_server_limit(s2_mod).is_ok());

        // Exceeds count cap (max 2 servers)
        let s3 = ServerResourceLimit::new("server-3")
            .with_memory_mb(512, None)
            .with_cpu_percent(25);
        let mut s3_mod = s3.clone();
        s3_mod.tenant_id = Some("tenant-a".to_string());
        assert!(reg.set_server_limit(s3_mod).is_err());
    }
}
