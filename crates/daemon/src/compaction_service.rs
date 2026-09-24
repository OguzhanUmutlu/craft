use craft_core::compaction::{
    CompactionCycleResult, CompactionOrchestrator, CompactionRegistry,
    CompactionStatusSummary, PagePoolStats, ThpDefragMode, ThpMode, ThpStatus,
};
use craft_core::{CraftError, CraftPaths, Result};
use craft_net::SocketPagePool;
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fmt::Write as FmtWrite;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use tracing::info;

static INSTANCE: OnceLock<Arc<CompactionService>> = OnceLock::new();

pub struct CompactionService {
    paths: CraftPaths,
    registry: Arc<RwLock<CompactionRegistry>>,
    orchestrator: Arc<RwLock<CompactionOrchestrator>>,
    page_pool: Arc<SocketPagePool>,
    compaction_runs_total: AtomicU64,
    pages_migrated_total: AtomicU64,
    hugepages_coalesced_total: AtomicU64,
    last_compaction_time: AtomicU64,
}

impl CompactionService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = CompactionRegistry::load(&paths).unwrap_or_default();
        let mut orchestrator = CompactionOrchestrator::default();
        orchestrator.buddy_state = registry.buddy_state.clone();
        orchestrator.thp_status.enabled = registry.config.hugepage_mode;
        let page_pool = SocketPagePool::default();

        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            orchestrator: Arc::new(RwLock::new(orchestrator)),
            page_pool: Arc::new(page_pool),
            compaction_runs_total: AtomicU64::new(0),
            pages_migrated_total: AtomicU64::new(0),
            hugepages_coalesced_total: AtomicU64::new(0),
            last_compaction_time: AtomicU64::new(0),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self) -> Result<CompactionStatusSummary> {
        let orch = self
            .orchestrator
            .read()
            .map_err(|_| CraftError::Other("Compaction orchestrator lock poisoned".to_string()))?;

        let reg = self
            .registry
            .read()
            .map_err(|_| CraftError::Other("Compaction registry lock poisoned".to_string()))?;

        let last_cycle = reg.history.last().cloned();
        let mut summary = orch.get_status_summary(last_cycle);

        let pool_stats = self.page_pool.stats();
        summary.total_pages_allocated = summary.total_pages_allocated.max(pool_stats.total_pages_allocated);
        summary.pages_recycled = summary.pages_recycled.max(pool_stats.pages_recycled);
        summary.zero_alloc_hits = summary.zero_alloc_hits.max(pool_stats.zero_alloc_hits);
        summary.allocation_stalls = summary.allocation_stalls.max(pool_stats.allocation_stalls);
        summary.recycle_efficiency_percent = pool_stats.recycle_efficiency();
        summary.total_cycles_completed = summary
            .total_cycles_completed
            .max(self.compaction_runs_total.load(Ordering::Relaxed))
            .max(reg.history.len() as u64);

        Ok(summary)
    }

    pub fn trigger_compaction(&self) -> Result<CompactionCycleResult> {
        let mut orch = self
            .orchestrator
            .write()
            .map_err(|_| CraftError::Other("Compaction orchestrator lock poisoned".to_string()))?;

        let cycle = orch.trigger_compaction();

        self.compaction_runs_total.fetch_add(1, Ordering::SeqCst);
        self.pages_migrated_total.fetch_add(cycle.pages_migrated, Ordering::SeqCst);
        self.hugepages_coalesced_total.fetch_add(cycle.hugepages_formed, Ordering::SeqCst);
        self.last_compaction_time.store(cycle.timestamp_epoch, Ordering::SeqCst);

        // Update registry
        if let Ok(mut reg) = self.registry.write() {
            reg.record_cycle(cycle.clone());
            reg.buddy_state = orch.buddy_state.clone();
            let _ = reg.save(&self.paths);
        }

        info!(
            "[COMPACTION] Completed cycle {}: status={:?}, migrated={} pages, hugepages={} formed, frag={:.2} -> {:.2}",
            cycle.cycle_id,
            cycle.status,
            cycle.pages_migrated,
            cycle.hugepages_formed,
            cycle.initial_fragmentation,
            cycle.final_fragmentation
        );

        // Dispatch HookContext for Lua Scripting
        let ctx = HookContext::for_memory_compaction(
            cycle.pages_migrated,
            cycle.hugepages_formed,
            cycle.final_fragmentation,
        );
        HookBus::dispatch_async(
            self.paths.clone(),
            LifecycleEvent::MemoryCompactionCompleted,
            ctx,
            10,
        );

        if cycle.final_fragmentation >= 0.65 {
            let warn_ctx = HookContext::for_high_memory_fragmentation(cycle.final_fragmentation);
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::HighMemoryFragmentationDetected,
                warn_ctx,
                10,
            );
        }

        Ok(cycle)
    }

    pub fn configure_thp(&self, mode: ThpMode, defrag: ThpDefragMode) -> Result<ThpStatus> {
        let mut orch = self
            .orchestrator
            .write()
            .map_err(|_| CraftError::Other("Compaction orchestrator lock poisoned".to_string()))?;

        orch.tune_thp(mode, defrag)?;

        #[cfg(target_os = "linux")]
        {
            // Best effort sysfs configuration
            let _ = std::fs::write(
                "/sys/kernel/mm/transparent_hugepage/enabled",
                format!("{}\n", mode),
            );
            let _ = std::fs::write(
                "/sys/kernel/mm/transparent_hugepage/defrag",
                format!("{}\n", defrag),
            );
        }

        let updated = orch.thp_status.clone();

        if let Ok(mut reg) = self.registry.write() {
            reg.config.hugepage_mode = mode;
            let _ = reg.save(&self.paths);
        }

        info!("[THP] Configured transparent hugepages: mode={}, defrag={}", mode, defrag);
        Ok(updated)
    }

    pub fn get_pool_stats(&self) -> Result<PagePoolStats> {
        Ok(self.page_pool.stats())
    }

    pub fn reset_metrics(&self) -> Result<()> {
        self.compaction_runs_total.store(0, Ordering::SeqCst);
        self.pages_migrated_total.store(0, Ordering::SeqCst);
        self.hugepages_coalesced_total.store(0, Ordering::SeqCst);
        self.last_compaction_time.store(0, Ordering::SeqCst);
        Ok(())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(1024);
        let summary = self.get_status().unwrap_or(CompactionStatusSummary {
            thp_status: ThpStatus::default(),
            fragmentation_index: 0.0,
            fragmentation_threshold: 0.65,
            compaction_needed: false,
            total_pages_allocated: 0,
            pages_recycled: 0,
            recycle_efficiency_percent: 100.0,
            zero_alloc_hits: 0,
            allocation_stalls: 0,
            total_cycles_completed: 0,
            last_cycle: None,
        });

        let pool_stats = self.get_pool_stats().unwrap_or_default();
        let runs = self.compaction_runs_total.load(Ordering::Relaxed).max(summary.total_cycles_completed);
        let migrated = self.pages_migrated_total.load(Ordering::Relaxed);
        let hugepages = self.hugepages_coalesced_total.load(Ordering::Relaxed);

        let _ = writeln!(
            out,
            "# HELP craft_compaction_runs_total Total memory compaction cycles executed"
        );
        let _ = writeln!(out, "# TYPE craft_compaction_runs_total counter");
        let _ = writeln!(out, "craft_compaction_runs_total {}", runs);

        let _ = writeln!(
            out,
            "# HELP craft_compaction_pages_migrated_total Total buddy pages migrated into coalesced blocks"
        );
        let _ = writeln!(out, "# TYPE craft_compaction_pages_migrated_total counter");
        let _ = writeln!(
            out,
            "craft_compaction_pages_migrated_total {}",
            migrated
        );

        let _ = writeln!(
            out,
            "# HELP craft_compaction_hugepages_coalesced_total Total 2MB transparent hugepages formed"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_compaction_hugepages_coalesced_total counter"
        );
        let _ = writeln!(
            out,
            "craft_compaction_hugepages_coalesced_total {}",
            hugepages
        );

        let _ = writeln!(
            out,
            "# HELP craft_compaction_fragmentation_index Memory fragmentation index (0.0 to 1.0)"
        );
        let _ = writeln!(out, "# TYPE craft_compaction_fragmentation_index gauge");
        let _ = writeln!(
            out,
            "craft_compaction_fragmentation_index {:.4}",
            summary.fragmentation_index
        );

        let thp_val = match summary.thp_status.enabled {
            ThpMode::Always => 1,
            ThpMode::Madvise => 2,
            ThpMode::Never => 0,
        };
        let _ = writeln!(
            out,
            "# HELP craft_compaction_thp_enabled Transparent hugepage mode (1=always, 2=madvise, 0=never)"
        );
        let _ = writeln!(out, "# TYPE craft_compaction_thp_enabled gauge");
        let _ = writeln!(out, "craft_compaction_thp_enabled {}", thp_val);

        let _ = writeln!(
            out,
            "# HELP craft_compaction_page_pool_active_pages Active pre-mapped page pool slots"
        );
        let _ = writeln!(out, "# TYPE craft_compaction_page_pool_active_pages gauge");
        let _ = writeln!(
            out,
            "craft_compaction_page_pool_active_pages {}",
            pool_stats.active_pages
        );

        let _ = writeln!(
            out,
            "# HELP craft_compaction_page_pool_recycled_total Total zero-alloc recycled network page buffers"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_compaction_page_pool_recycled_total counter"
        );
        let _ = writeln!(
            out,
            "craft_compaction_page_pool_recycled_total {}",
            pool_stats.pages_recycled
        );

        let _ = writeln!(
            out,
            "# HELP craft_compaction_page_pool_exhaustions_total Total page pool buffer exhaustion fallbacks"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_compaction_page_pool_exhaustions_total counter"
        );
        let _ = writeln!(
            out,
            "craft_compaction_page_pool_exhaustions_total {}",
            pool_stats.pool_exhaustions
        );

        let _ = writeln!(
            out,
            "# HELP craft_compaction_page_pool_fast_path_reuse_ratio Page pool zero-allocation reuse ratio (0.0 to 1.0)"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_compaction_page_pool_fast_path_reuse_ratio gauge"
        );
        let _ = writeln!(
            out,
            "craft_compaction_page_pool_fast_path_reuse_ratio {:.4}",
            pool_stats.fast_path_reuse_ratio()
        );

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::compaction::CompactionStatus;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_compaction_service_lifecycle() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = CompactionService::new(paths);

        let status = service.get_status().unwrap();
        assert!(status.fragmentation_index >= 0.0);
        assert_eq!(status.total_cycles_completed, 0);

        let cycle = service.trigger_compaction().unwrap();
        assert_eq!(cycle.status, CompactionStatus::Success);
        assert!(cycle.pages_migrated > 0);

        let after = service.get_status().unwrap();
        assert_eq!(after.total_cycles_completed, 1);
        assert!(after.last_cycle.is_some());

        // Test THP configuration
        let thp = service.configure_thp(ThpMode::Always, ThpDefragMode::Always).unwrap();
        assert_eq!(thp.enabled, ThpMode::Always);

        // Test Prometheus metrics
        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_compaction_runs_total 1"));
        assert!(metrics.contains("craft_compaction_fragmentation_index"));
        assert!(metrics.contains("craft_compaction_thp_enabled 1"));
    }
}
