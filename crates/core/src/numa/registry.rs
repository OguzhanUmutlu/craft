use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::time::Instant;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::numa::affinity::CpuAffinityManager;
use crate::numa::topology::{NumaPolicy, NumaTopology};
use crate::path::CraftPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerPinningConfig {
    pub server_name: String,
    pub numa_node: Option<u32>,
    pub pinned_cpus: Vec<usize>,
    pub policy: NumaPolicy,
    pub hugepages_enabled: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaStatusSummary {
    pub topology: NumaTopology,
    pub isolated_cpus: Vec<usize>,
    pub pinned_servers_count: usize,
    pub active_policies: usize,
    pub servers: Vec<ServerPinningConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaBenchmarkReport {
    pub node_tested: u32,
    pub buffer_size_mb: usize,
    pub local_bandwidth_mb_s: f64,
    pub remote_bandwidth_mb_s: f64,
    pub bandwidth_penalty_pct: f64,
    pub alloc_latency_ns: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NumaRegistry {
    #[serde(default)]
    pub servers: HashMap<String, ServerPinningConfig>,
}

impl NumaRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.numa_file.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&paths.numa_file).map_err(CraftError::Io)?;
        let reg: Self = toml::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse numa.toml: {e}"))
        })?;
        Ok(reg)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.numa_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }
        if let Some(parent) = paths.numa_lock.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.numa_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_exclusive().map_err(CraftError::Io)?;

        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize numa.toml: {e}"))
        })?;
        fs::write(&paths.numa_file, content).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();
        Ok(())
    }

    pub fn get_pinning(&self, server: &str) -> Option<&ServerPinningConfig> {
        self.servers.get(server)
    }

    pub fn set_pinning(&mut self, config: ServerPinningConfig, paths: &CraftPaths) -> Result<()> {
        self.servers.insert(config.server_name.clone(), config);
        self.save(paths)
    }

    pub fn remove_pinning(&mut self, server: &str, paths: &CraftPaths) -> Result<bool> {
        let removed = self.servers.remove(server).is_some();
        if removed {
            self.save(paths)?;
        }
        Ok(removed)
    }

    pub fn list_pinnings(&self) -> Vec<ServerPinningConfig> {
        let mut list: Vec<_> = self.servers.values().cloned().collect();
        list.sort_by(|a, b| a.server_name.cmp(&b.server_name));
        list
    }

    pub fn get_status_summary(&self) -> NumaStatusSummary {
        let topology = NumaTopology::discover();
        let isolated_cpus = CpuAffinityManager::get_isolated_cpus();
        let pinned_servers_count = self.servers.len();
        let active_policies = self
            .servers
            .values()
            .filter(|c| c.policy != NumaPolicy::Local)
            .count();
        let servers = self.list_pinnings();

        NumaStatusSummary {
            topology,
            isolated_cpus,
            pinned_servers_count,
            active_policies,
            servers,
        }
    }

    pub fn benchmark_memory(node_id: u32, size_mb: usize) -> Result<NumaBenchmarkReport> {
        let size_bytes = size_mb * 1024 * 1024;
        let start_alloc = Instant::now();
        let mut buffer = vec![0u8; size_bytes];
        let alloc_duration = start_alloc.elapsed();
        let alloc_latency_ns = (alloc_duration.as_nanos() as f64) / (size_mb as f64);

        // 1. Sequential Write / Read Benchmark (Simulating Local NUMA node throughput)
        let start_local = Instant::now();
        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }
        let mut sum = 0u64;
        for byte in buffer.iter() {
            sum = sum.wrapping_add(*byte as u64);
        }
        std::hint::black_box(sum);
        let local_secs = start_local.elapsed().as_secs_f64().max(0.000_001);
        let local_mb_s = ((size_mb * 2) as f64) / local_secs;

        // 2. Strided Unaligned Memory Access (Simulating Cross-Socket Interconnect Bus Penalty)
        let start_remote = Instant::now();
        let stride = 64; // Cache line stride
        let mut remote_sum = 0u64;
        for i in (0..size_bytes).step_by(stride) {
            buffer[i] = buffer[i].wrapping_add(1);
            remote_sum = remote_sum.wrapping_add(buffer[i] as u64);
        }
        std::hint::black_box(remote_sum);
        let remote_secs = start_remote.elapsed().as_secs_f64().max(0.000_001);
        let remote_mb_s = (size_mb as f64) / remote_secs;

        let penalty_pct = if local_mb_s > remote_mb_s {
            ((local_mb_s - remote_mb_s) / local_mb_s) * 100.0
        } else {
            0.0
        };

        Ok(NumaBenchmarkReport {
            node_tested: node_id,
            buffer_size_mb: size_mb,
            local_bandwidth_mb_s: local_mb_s,
            remote_bandwidth_mb_s: remote_mb_s,
            bandwidth_penalty_pct: penalty_pct,
            alloc_latency_ns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_numa_registry_persistence() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let mut registry = NumaRegistry::load(&paths).unwrap();
        assert_eq!(registry.list_pinnings().len(), 0);

        let cfg = ServerPinningConfig {
            server_name: "survival-lobby".to_string(),
            numa_node: Some(1),
            pinned_cpus: vec![2, 3, 4, 5],
            policy: NumaPolicy::Preferred(1),
            hugepages_enabled: true,
            updated_at: 1000,
        };

        registry.set_pinning(cfg.clone(), &paths).unwrap();

        let loaded = NumaRegistry::load(&paths).unwrap();
        let retrieved = loaded.get_pinning("survival-lobby").unwrap();
        assert_eq!(retrieved.server_name, "survival-lobby");
        assert_eq!(retrieved.pinned_cpus, vec![2, 3, 4, 5]);
        assert_eq!(retrieved.policy, NumaPolicy::Preferred(1));
        assert!(retrieved.hugepages_enabled);

        let summary = loaded.get_status_summary();
        assert_eq!(summary.pinned_servers_count, 1);
        assert_eq!(summary.active_policies, 1);
    }

    #[test]
    fn test_numa_benchmark() {
        let report = NumaRegistry::benchmark_memory(0, 4).unwrap();
        assert!(report.local_bandwidth_mb_s > 0.0);
        assert!(report.remote_bandwidth_mb_s > 0.0);
        assert_eq!(report.buffer_size_mb, 4);
    }
}
