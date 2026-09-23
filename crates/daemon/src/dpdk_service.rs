use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use craft_core::{
    CpuAffinityManager, KernelBootParams, NumaBenchmarkReport, NumaPolicy, NumaRegistry,
    NumaStatusSummary, ServerPinningConfig,
};
use craft_core::path::CraftPaths;
use craft_net::{DpdkDriver, DpdkDriverConfig, DpdkDriverStats};

static GLOBAL_DPDK_NUMA: OnceLock<Arc<DpdkNumaService>> = OnceLock::new();

pub struct DpdkNumaService {
    paths: CraftPaths,
    registry: Arc<Mutex<NumaRegistry>>,
    dpdk_driver: Arc<DpdkDriver>,
}

impl DpdkNumaService {
    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        GLOBAL_DPDK_NUMA
            .get_or_init(|| {
                let registry = NumaRegistry::load(paths).unwrap_or_default();
                let dpdk_driver = Arc::new(DpdkDriver::new(DpdkDriverConfig::default()));
                dpdk_driver.start();

                Arc::new(Self {
                    paths: paths.clone(),
                    registry: Arc::new(Mutex::new(registry)),
                    dpdk_driver,
                })
            })
            .clone()
    }

    pub async fn get_numa_status(&self) -> NumaStatusSummary {
        let reg = self.registry.lock().await;
        reg.get_status_summary()
    }

    pub async fn pin_server_cores(
        &self,
        server_name: &str,
        cpus: Vec<usize>,
        numa_node: Option<u32>,
        policy: NumaPolicy,
    ) -> Result<ServerPinningConfig, String> {
        let mut reg = self.registry.lock().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cfg = ServerPinningConfig {
            server_name: server_name.to_string(),
            numa_node,
            pinned_cpus: cpus.clone(),
            policy,
            hugepages_enabled: false,
            updated_at: now,
        };

        reg.set_pinning(cfg.clone(), &self.paths)
            .map_err(|e| format!("Failed to save pinning: {e}"))?;

        // If server is running, attempt to apply affinity to its process
        if let Ok(server_path) = self.paths.resolve_server_path(None, Some(server_name), true) {
            let pid_file = server_path.join("server.pid");
            if pid_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = content.trim().parse::<u32>() {
                        let _ = CpuAffinityManager::set_process_affinity(pid, &cpus);
                    }
                }
            }
        }

        Ok(cfg)
    }

    pub async fn set_numa_policy(
        &self,
        server_name: &str,
        policy: NumaPolicy,
    ) -> Result<ServerPinningConfig, String> {
        let mut reg = self.registry.lock().await;
        let mut cfg = if let Some(existing) = reg.get_pinning(server_name) {
            existing.clone()
        } else {
            ServerPinningConfig {
                server_name: server_name.to_string(),
                numa_node: None,
                pinned_cpus: Vec::new(),
                policy,
                hugepages_enabled: false,
                updated_at: 0,
            }
        };

        cfg.policy = policy;
        cfg.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        reg.set_pinning(cfg.clone(), &self.paths)
            .map_err(|e| format!("Failed to save policy: {e}"))?;

        Ok(cfg)
    }

    pub async fn benchmark_numa_memory(
        &self,
        node_id: u32,
        size_mb: usize,
    ) -> Result<NumaBenchmarkReport, String> {
        NumaRegistry::benchmark_memory(node_id, size_mb)
            .map_err(|e| format!("Memory benchmark failed: {e}"))
    }

    pub fn get_dpdk_status(&self, bench_count: Option<usize>) -> DpdkDriverStats {
        if let Some(count) = bench_count {
            if count > 0 {
                return self.dpdk_driver.benchmark_synthetic_stream(count);
            }
        }
        self.dpdk_driver.get_stats()
    }

    pub fn generate_boot_params(&self, isolated_cores: &[usize], hugepages_1g: usize) -> KernelBootParams {
        CpuAffinityManager::generate_boot_params(isolated_cores, hugepages_1g)
    }

    pub async fn format_prometheus_metrics(&self) -> String {
        let stats = self.dpdk_driver.get_stats();
        let summary = self.get_numa_status().await;

        let mut out = String::new();
        out.push_str("# HELP craft_dpdk_rx_packets_total Total DPDK packets received\n");
        out.push_str("# TYPE craft_dpdk_rx_packets_total counter\n");
        out.push_str(&format!("craft_dpdk_rx_packets_total {}\n", stats.rx_packets));

        out.push_str("# HELP craft_dpdk_tx_packets_total Total DPDK packets transmitted\n");
        out.push_str("# TYPE craft_dpdk_tx_packets_total counter\n");
        out.push_str(&format!("craft_dpdk_tx_packets_total {}\n", stats.tx_packets));

        out.push_str("# HELP craft_dpdk_rx_bytes_total Total DPDK RX bytes\n");
        out.push_str("# TYPE craft_dpdk_rx_bytes_total counter\n");
        out.push_str(&format!("craft_dpdk_rx_bytes_total {}\n", stats.rx_bytes));

        out.push_str("# HELP craft_dpdk_rx_dropped_total Total DPDK dropped RX packets\n");
        out.push_str("# TYPE craft_dpdk_rx_dropped_total counter\n");
        out.push_str(&format!("craft_dpdk_rx_dropped_total {}\n", stats.rx_dropped));

        out.push_str("# HELP craft_dpdk_jitter_micros Current average packet inter-arrival jitter in microseconds\n");
        out.push_str("# TYPE craft_dpdk_jitter_micros gauge\n");
        out.push_str(&format!("craft_dpdk_jitter_micros {:.2}\n", stats.avg_jitter_micros));

        out.push_str("# HELP craft_numa_nodes_count Detected physical/logical NUMA nodes\n");
        out.push_str("# TYPE craft_numa_nodes_count gauge\n");
        out.push_str(&format!("craft_numa_nodes_count {}\n", summary.topology.nodes.len()));

        out.push_str("# HELP craft_numa_pinned_servers_count Active servers with CPU core affinity pinning\n");
        out.push_str("# TYPE craft_numa_pinned_servers_count gauge\n");
        out.push_str(&format!("craft_numa_pinned_servers_count {}\n", summary.pinned_servers_count));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_dpdk_numa_service_lifecycle() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = DpdkNumaService::global(&paths);

        let status = service.get_numa_status().await;
        assert!(!status.topology.nodes.is_empty());

        let pin_res = service
            .pin_server_cores("srv-alpha", vec![0, 1], Some(0), NumaPolicy::Local)
            .await;
        assert!(pin_res.is_ok());

        let cfg = pin_res.unwrap();
        assert_eq!(cfg.server_name, "srv-alpha");
        assert_eq!(cfg.pinned_cpus, vec![0, 1]);

        let dpdk_stats = service.get_dpdk_status(Some(50));
        assert_eq!(dpdk_stats.rx_packets, 50);

        let metrics = service.format_prometheus_metrics().await;
        assert!(metrics.contains("craft_dpdk_rx_packets_total"));
        assert!(metrics.contains("craft_numa_nodes_count"));
    }
}
