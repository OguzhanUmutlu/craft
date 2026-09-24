// crates/daemon/src/nvme_service.rs
//
// Autonomous Zero-Copy Storage Fabrics, NVMe-oF Target & Distributed Flash Block Pool Service.
// Strictly zero emojis.

use craft_core::error::{CraftError, Result};
use craft_core::nvme::{
    current_epoch_secs, NvmeBenchmarkMetrics, NvmeNamespaceDescriptor,
    NvmeRegistry, NvmeStatusSummary, NvmeSubsystemDescriptor,
};
use craft_core::path::CraftPaths;
use craft_net::nvme::{benchmark_nvme_fabric, FlashBlockPoolEngine, NvmeTargetEngine};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static INSTANCE: OnceLock<Arc<NvmeTargetService>> = OnceLock::new();

pub struct NvmeTargetService {
    paths: CraftPaths,
    target: Mutex<NvmeTargetEngine>,
    multipath_failovers_total: Arc<AtomicU64>,
    iops_current: Arc<AtomicU64>,
}

impl NvmeTargetService {
    pub fn new(paths: CraftPaths) -> Self {
        let reg = NvmeRegistry::load(&paths).unwrap_or_default();
        let mut pool = FlashBlockPoolEngine::new(reg.status.total_pool_bytes);
        for ns in &reg.namespaces {
            pool.namespaces.insert(ns.nsid, ns.clone());
        }

        let mut target = NvmeTargetEngine::new(pool);
        for sub in &reg.subsystems {
            target.register_subsystem(sub.clone());
        }

        Self {
            paths,
            target: Mutex::new(target),
            multipath_failovers_total: Arc::new(AtomicU64::new(reg.status.multipath_failovers_total)),
            iops_current: Arc::new(AtomicU64::new(reg.status.iops_current as u64)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(
        &self,
        _server: Option<&str>,
    ) -> Result<(NvmeStatusSummary, Vec<NvmeSubsystemDescriptor>)> {
        let reg = NvmeRegistry::load(&self.paths)?;
        let mut summary = reg.status.clone();
        summary.multipath_failovers_total = self.multipath_failovers_total.load(Ordering::Relaxed);
        summary.iops_current = self.iops_current.load(Ordering::Relaxed) as f64;

        if let Ok(target) = self.target.lock() {
            let engine_metrics = target.get_metrics();
            summary.active_namespaces = engine_metrics.active_namespaces;
            summary.allocated_pool_bytes = engine_metrics.allocated_pool_bytes;
            summary.pool_utilization_percent = engine_metrics.pool_utilization_percent;
            if engine_metrics.avg_latency_micros > 0.0 {
                summary.avg_latency_micros = engine_metrics.avg_latency_micros;
            }
        }

        Ok((summary, reg.subsystems))
    }

    pub fn create_namespace(
        &self,
        nsid: u32,
        size_mb: u64,
        block_size: u32,
        server_id: Option<String>,
        dimension: Option<String>,
    ) -> Result<NvmeNamespaceDescriptor> {
        let blk_size = if block_size == 0 { 4096 } else { block_size };
        let capacity_bytes = size_mb * 1024 * 1024;
        let size_blocks = capacity_bytes / blk_size as u64;

        let desc = NvmeNamespaceDescriptor {
            nsid,
            size_blocks,
            block_size: blk_size,
            capacity_bytes,
            allocated_bytes: 0,
            server_id: server_id.clone(),
            dimension: dimension.clone(),
            thin_provisioned: true,
            read_only: false,
            created_at_secs: current_epoch_secs(),
        };

        let mut reg = NvmeRegistry::load(&self.paths)?;
        reg.modify(&self.paths, |r| {
            if r.namespaces.iter().any(|n| n.nsid == nsid) {
                return Err(CraftError::Config(format!(
                    "Namespace ID {} already exists in registry",
                    nsid
                )));
            }
            r.namespaces.push(desc.clone());
            if let Some(sub) = r.subsystems.first_mut() {
                if !sub.namespaces.contains(&nsid) {
                    sub.namespaces.push(nsid);
                }
            }
            Ok(())
        })?;

        if let Ok(mut target) = self.target.lock() {
            let _ = target.block_pool.create_namespace(
                nsid,
                size_mb,
                blk_size,
                server_id,
                dimension,
            );
        }

        Ok(desc)
    }

    pub fn delete_namespace(&self, nsid: u32) -> Result<bool> {
        let mut reg = NvmeRegistry::load(&self.paths)?;
        reg.modify(&self.paths, |r| {
            r.namespaces.retain(|n| n.nsid != nsid);
            for sub in &mut r.subsystems {
                sub.namespaces.retain(|&id| id != nsid);
            }
            Ok(())
        })?;

        if let Ok(mut target) = self.target.lock() {
            let _ = target.block_pool.delete_namespace(nsid);
        }

        Ok(true)
    }

    pub fn list_namespaces(&self, server: Option<&str>) -> Result<Vec<NvmeNamespaceDescriptor>> {
        let reg = NvmeRegistry::load(&self.paths)?;
        if let Some(srv) = server {
            Ok(reg
                .namespaces
                .into_iter()
                .filter(|n| n.server_id.as_deref() == Some(srv))
                .collect())
        } else {
            Ok(reg.namespaces)
        }
    }

    pub fn list_subsystems(&self) -> Result<Vec<NvmeSubsystemDescriptor>> {
        let reg = NvmeRegistry::load(&self.paths)?;
        Ok(reg.subsystems)
    }

    pub fn run_bench(
        &self,
        block_size: usize,
        iterations: usize,
    ) -> Result<NvmeBenchmarkMetrics> {
        let metrics = benchmark_nvme_fabric(block_size, iterations);
        self.iops_current.store(metrics.iops as u64, Ordering::Relaxed);
        self.multipath_failovers_total
            .fetch_add(metrics.multipath_failovers, Ordering::Relaxed);
        Ok(metrics)
    }

    pub fn reset_metrics(&self, _server: Option<&str>) -> Result<String> {
        let mut reg = NvmeRegistry::load(&self.paths).unwrap_or_default();
        reg.modify(&self.paths, |r| {
            r.status.iops_current = 0.0;
            r.status.multipath_failovers_total = 0;
            r.status.avg_latency_micros = 14.5;
            Ok(())
        })?;

        self.iops_current.store(0, Ordering::Relaxed);
        self.multipath_failovers_total.store(0, Ordering::Relaxed);

        if let Ok(mut target) = self.target.lock() {
            target.total_ops = 0;
            target.total_latency_nanos = 0;
            target.multipath_failover_count = 0;
        }

        Ok("NVMe storage fabric metrics reset successfully".to_string())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        let reg = NvmeRegistry::load(&self.paths).unwrap_or_default();

        out.push_str("# HELP craft_nvme_active_subsystems Number of active NVMe-oF target subsystems
");
        out.push_str("# TYPE craft_nvme_active_subsystems gauge
");
        out.push_str(&format!("craft_nvme_active_subsystems {}
", reg.status.active_subsystems));

        out.push_str("# HELP craft_nvme_active_namespaces Number of allocated flash storage namespaces
");
        out.push_str("# TYPE craft_nvme_active_namespaces gauge
");
        out.push_str(&format!("craft_nvme_active_namespaces {}
", reg.status.active_namespaces));

        out.push_str("# HELP craft_nvme_pool_total_bytes Total distributed flash storage pool capacity in bytes
");
        out.push_str("# TYPE craft_nvme_pool_total_bytes gauge
");
        out.push_str(&format!("craft_nvme_pool_total_bytes {}
", reg.status.total_pool_bytes));

        out.push_str("# HELP craft_nvme_pool_allocated_bytes Allocated flash storage pool capacity in bytes
");
        out.push_str("# TYPE craft_nvme_pool_allocated_bytes gauge
");
        out.push_str(&format!("craft_nvme_pool_allocated_bytes {}
", reg.status.allocated_pool_bytes));

        out.push_str("# HELP craft_nvme_pool_utilization_percent Flash storage pool utilization percentage
");
        out.push_str("# TYPE craft_nvme_pool_utilization_percent gauge
");
        out.push_str(&format!("craft_nvme_pool_utilization_percent {:.2}
", reg.status.pool_utilization_percent));

        out.push_str("# HELP craft_nvme_active_controllers Number of active NVMe block controllers
");
        out.push_str("# TYPE craft_nvme_active_controllers gauge
");
        out.push_str(&format!("craft_nvme_active_controllers {}
", reg.status.active_controllers));

        out.push_str("# HELP craft_nvme_iops_total Current flash fabric I/O throughput in IOPS
");
        out.push_str("# TYPE craft_nvme_iops_total gauge
");
        out.push_str(&format!("craft_nvme_iops_total {}
", self.iops_current.load(Ordering::Relaxed)));

        out.push_str("# HELP craft_nvme_avg_latency_micros Average flash fabric I/O latency in microseconds
");
        out.push_str("# TYPE craft_nvme_avg_latency_micros gauge
");
        out.push_str(&format!("craft_nvme_avg_latency_micros {:.2}
", reg.status.avg_latency_micros));

        out.push_str("# HELP craft_nvme_multipath_failovers_total Total multipath failover transitions between RDMA and TCP
");
        out.push_str("# TYPE craft_nvme_multipath_failovers_total counter
");
        out.push_str(&format!("craft_nvme_multipath_failovers_total {}
", self.multipath_failovers_total.load(Ordering::Relaxed)));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_nvme_service_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = NvmeTargetService::new(paths);

        let (status, subs) = service.get_status(None).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(status.active_namespaces, 0);

        let ns = service
            .create_namespace(
                1,
                512,
                4096,
                Some("srv-01".to_string()),
                Some("the_nether".to_string()),
            )
            .unwrap();
        assert_eq!(ns.nsid, 1);
        assert_eq!(ns.dimension.as_deref(), Some("the_nether"));

        let list = service.list_namespaces(None).unwrap();
        assert_eq!(list.len(), 1);

        let sub_list = service.list_subsystems().unwrap();
        assert_eq!(sub_list.len(), 1);

        let bench = service.run_bench(4096, 50).unwrap();
        assert!(bench.iops >= 850_000.0);

        let prom = service.generate_prometheus_metrics();
        assert!(prom.contains("craft_nvme_active_namespaces 1"));

        let del = service.delete_namespace(1).unwrap();
        assert!(del);
        assert_eq!(service.list_namespaces(None).unwrap().len(), 0);

        let reset = service.reset_metrics(None).unwrap();
        assert!(reset.contains("reset successfully"));
    }
}
