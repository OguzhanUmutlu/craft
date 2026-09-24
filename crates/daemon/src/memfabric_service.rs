// crates/daemon/src/memfabric_service.rs
//
// Autonomous Distributed Inter-Server Memory Fabric, Remote Paged Compaction
// and Cluster NVRAM Pool Service.
// Strictly zero emojis.

use craft_core::error::Result;
use craft_core::memfabric::{
    MemFabricBenchmarkMetrics, MemFabricNodeInfo, MemFabricRegistry,
    MemFabricStatusSummary, MemoryTier, RemotePageDescriptor,
};
use craft_core::path::CraftPaths;
use craft_net::memfabric::{
    benchmark_remote_paging, DimensionMemoryFabric, RemotePagingEngine, UserfaultPageHandler,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static INSTANCE: OnceLock<Arc<MemFabricService>> = OnceLock::new();

pub struct MemFabricService {
    paths: CraftPaths,
    engine: Mutex<RemotePagingEngine>,
    _handler: Mutex<UserfaultPageHandler>,
    dim_fabric: Mutex<DimensionMemoryFabric>,
    page_faults_total: Arc<AtomicU64>,
    remote_evictions_total: Arc<AtomicU64>,
}

impl MemFabricService {
    pub fn new(paths: CraftPaths) -> Self {
        let reg = MemFabricRegistry::load(&paths).unwrap_or_default();
        let mut engine = RemotePagingEngine::new();
        for page in &reg.pages {
            let _ = engine.allocate_page(
                &page.page_id,
                page.page_size,
                page.tier,
                page.dimension.as_deref(),
            );
        }

        Self {
            paths,
            engine: Mutex::new(engine),
            _handler: Mutex::new(UserfaultPageHandler::new()),
            dim_fabric: Mutex::new(DimensionMemoryFabric::new()),
            page_faults_total: Arc::new(AtomicU64::new(reg.status.page_faults_total)),
            remote_evictions_total: Arc::new(AtomicU64::new(reg.status.remote_evictions_total)),
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
    ) -> Result<(MemFabricStatusSummary, Vec<MemFabricNodeInfo>)> {
        let reg = MemFabricRegistry::load(&self.paths)?;
        let mut summary = reg.status.clone();
        summary.page_faults_total = self.page_faults_total.load(Ordering::Relaxed);
        summary.remote_evictions_total = self.remote_evictions_total.load(Ordering::Relaxed);

        if let Ok(engine) = self.engine.lock() {
            let (faults, evictions, avg_lat) = engine.metrics();
            summary.page_faults_total = summary.page_faults_total.max(faults);
            summary.remote_evictions_total = summary.remote_evictions_total.max(evictions);
            if avg_lat > 0.0 {
                summary.avg_page_fault_latency_nanos = avg_lat as u64;
            }
        }

        Ok((summary, reg.nodes))
    }

    pub fn allocate_page(
        &self,
        page_id: String,
        size: usize,
        tier: MemoryTier,
        dimension: Option<String>,
    ) -> Result<RemotePageDescriptor> {
        let mut reg = MemFabricRegistry::load(&self.paths)?;
        let page = reg.modify(&self.paths, |r| {
            r.allocate_page(page_id.clone(), size, tier, dimension.clone())
        })?;

        if let Ok(mut engine) = self.engine.lock() {
            let _ = engine.allocate_page(&page_id, size, tier, dimension.as_deref());
        }

        Ok(page)
    }

    pub fn evict_dimension(
        &self,
        dimension: &str,
        target_node: Option<&str>,
    ) -> Result<(usize, u64)> {
        let mut reg = MemFabricRegistry::load(&self.paths)?;
        let (pages_count, bytes_freed) = reg.modify(&self.paths, |r| {
            r.evict_dimension(dimension, target_node)
        })?;

        self.remote_evictions_total
            .fetch_add(pages_count as u64, Ordering::Relaxed);

        if let (Ok(mut engine), Ok(mut dim_fabric)) = (self.engine.lock(), self.dim_fabric.lock()) {
            let target = target_node.unwrap_or("remote-fabric-node-1");
            let _ = dim_fabric.evict_dimension(&mut engine, dimension, target);
        }

        Ok((pages_count, bytes_freed))
    }

    pub fn list_pages(&self, _server: Option<&str>) -> Result<Vec<RemotePageDescriptor>> {
        let reg = MemFabricRegistry::load(&self.paths)?;
        Ok(reg.pages)
    }

    pub fn run_bench(
        &self,
        iterations: usize,
        page_size: usize,
    ) -> Result<MemFabricBenchmarkMetrics> {
        let metrics = benchmark_remote_paging(iterations, page_size)?;
        self.page_faults_total
            .fetch_add(metrics.pages_processed as u64, Ordering::Relaxed);
        Ok(metrics)
    }

    pub fn reset_metrics(&self, _server: Option<&str>) -> Result<String> {
        let mut reg = MemFabricRegistry::load(&self.paths).unwrap_or_default();
        reg.modify(&self.paths, |r| {
            r.reset_metrics();
            Ok(())
        })?;

        self.page_faults_total.store(0, Ordering::Relaxed);
        self.remote_evictions_total.store(0, Ordering::Relaxed);

        if let Ok(mut engine) = self.engine.lock() {
            engine.total_faults = 0;
            engine.total_evictions = 0;
            engine.total_fault_latency_nanos = 0;
            engine.fault_history.clear();
        }

        Ok("MemFabric metrics reset successfully".to_string())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        let reg = MemFabricRegistry::load(&self.paths).unwrap_or_default();

        out.push_str("# HELP craft_memfabric_active_nodes Number of active memory fabric nodes\n");
        out.push_str("# TYPE craft_memfabric_active_nodes gauge\n");
        out.push_str(&format!("craft_memfabric_active_nodes {}\n", reg.status.active_nodes));

        out.push_str("# HELP craft_memfabric_total_dram_bytes Total cluster DRAM capacity in bytes\n");
        out.push_str("# TYPE craft_memfabric_total_dram_bytes gauge\n");
        out.push_str(&format!("craft_memfabric_total_dram_bytes {}\n", reg.status.total_dram_bytes));

        out.push_str("# HELP craft_memfabric_allocated_dram_bytes Allocated cluster DRAM in bytes\n");
        out.push_str("# TYPE craft_memfabric_allocated_dram_bytes gauge\n");
        out.push_str(&format!("craft_memfabric_allocated_dram_bytes {}\n", reg.status.allocated_dram_bytes));

        out.push_str("# HELP craft_memfabric_total_nvram_bytes Total cluster NVRAM capacity in bytes\n");
        out.push_str("# TYPE craft_memfabric_total_nvram_bytes gauge\n");
        out.push_str(&format!("craft_memfabric_total_nvram_bytes {}\n", reg.status.total_nvram_bytes));

        out.push_str("# HELP craft_memfabric_allocated_nvram_bytes Allocated cluster NVRAM in bytes\n");
        out.push_str("# TYPE craft_memfabric_allocated_nvram_bytes gauge\n");
        out.push_str(&format!("craft_memfabric_allocated_nvram_bytes {}\n", reg.status.allocated_nvram_bytes));

        out.push_str("# HELP craft_memfabric_dram_utilization_percent Cluster DRAM utilization percent\n");
        out.push_str("# TYPE craft_memfabric_dram_utilization_percent gauge\n");
        out.push_str(&format!("craft_memfabric_dram_utilization_percent {:.2}\n", reg.status.dram_utilization_percent));

        out.push_str("# HELP craft_memfabric_nvram_utilization_percent Cluster NVRAM utilization percent\n");
        out.push_str("# TYPE craft_memfabric_nvram_utilization_percent gauge\n");
        out.push_str(&format!("craft_memfabric_nvram_utilization_percent {:.2}\n", reg.status.nvram_utilization_percent));

        out.push_str("# HELP craft_memfabric_total_pages_managed Total number of memory fabric virtual pages\n");
        out.push_str("# TYPE craft_memfabric_total_pages_managed gauge\n");
        out.push_str(&format!("craft_memfabric_total_pages_managed {}\n", reg.status.total_pages_managed));

        out.push_str("# HELP craft_memfabric_remote_pages_count Count of pages paged out to remote NVRAM\n");
        out.push_str("# TYPE craft_memfabric_remote_pages_count gauge\n");
        out.push_str(&format!("craft_memfabric_remote_pages_count {}\n", reg.status.remote_pages_count));

        out.push_str("# HELP craft_memfabric_page_faults_total Total userfaultfd page fault resolutions\n");
        out.push_str("# TYPE craft_memfabric_page_faults_total counter\n");
        out.push_str(&format!("craft_memfabric_page_faults_total {}\n", self.page_faults_total.load(Ordering::Relaxed)));

        out.push_str("# HELP craft_memfabric_remote_evictions_total Total remote memory page evictions\n");
        out.push_str("# TYPE craft_memfabric_remote_evictions_total counter\n");
        out.push_str(&format!("craft_memfabric_remote_evictions_total {}\n", self.remote_evictions_total.load(Ordering::Relaxed)));

        out.push_str("# HELP craft_memfabric_avg_page_fault_latency_nanos Average page fault resolution latency in nanoseconds\n");
        out.push_str("# TYPE craft_memfabric_avg_page_fault_latency_nanos gauge\n");
        out.push_str(&format!("craft_memfabric_avg_page_fault_latency_nanos {}\n", reg.status.avg_page_fault_latency_nanos));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_memfabric_service_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = MemFabricService::new(paths);

        let (status, nodes) = service.get_status(None).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(status.total_pages_managed, 0);

        let page = service
            .allocate_page(
                "dim-nether-test".to_string(),
                4096,
                MemoryTier::LocalDram,
                Some("the_nether".to_string()),
            )
            .unwrap();
        assert_eq!(page.page_id, "dim-nether-test");
        assert_eq!(page.tier, MemoryTier::LocalDram);

        let pages = service.list_pages(None).unwrap();
        assert_eq!(pages.len(), 1);

        let (evicted_count, bytes_freed) = service.evict_dimension("the_nether", None).unwrap();
        assert_eq!(evicted_count, 1);
        assert_eq!(bytes_freed, 4096);

        let bench = service.run_bench(10, 4096).unwrap();
        assert_eq!(bench.pages_processed, 10);

        let reset = service.reset_metrics(None).unwrap();
        assert!(reset.contains("reset successfully"));
    }
}
