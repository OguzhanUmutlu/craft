use craft_core::pmu::{
    HotspotSymbol, PmuMetricsSummary, PmuProbeConfig, PmuProbeStatus, PmuRegistry, PmuSampleRecord,
};
use craft_core::{CraftError, CraftPaths, Result};
use craft_net::{MemoryChurnReport, PmuSampler};
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fmt::Write as FmtWrite;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;
use tracing::info;

static INSTANCE: OnceLock<Arc<PmuService>> = OnceLock::new();

/// Background supervisor coordinating hardware PMU sampling, profiling, and telemetry
pub struct PmuService {
    paths: CraftPaths,
    registry: Arc<RwLock<PmuRegistry>>,
    sampler: Arc<RwLock<PmuSampler>>,
    is_sampling: AtomicBool,
    active_pid: AtomicU32,
    pub start_time: Instant,
}

impl PmuService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = PmuRegistry::load(&paths).unwrap_or_default();
        let sampler = PmuSampler::new(100, 512);

        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            sampler: Arc::new(RwLock::new(sampler)),
            is_sampling: AtomicBool::new(false),
            active_pid: AtomicU32::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    /// Retrieve full PMU metrics summary
    pub fn get_status(&self) -> Result<PmuMetricsSummary> {
        let sampler = self
            .sampler
            .read()
            .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;

        let mut summary = sampler.get_summary();
        if summary.total_samples == 0 {
            if let Ok(state) = PmuRegistry::load_state(&self.paths) {
                if state.total_samples > 0 {
                    summary = state;
                }
            }
        }
        if self.is_sampling.load(Ordering::SeqCst) {
            summary.active_probes = summary.active_probes.max(1);
        } else {
            summary.active_probes = 0;
        }
        Ok(summary)
    }

    /// Start hardware PMU sampling against a target PID or system-wide
    pub fn start_sampling(&self, target_pid: Option<u32>, sample_rate_hz: u32) -> Result<PmuMetricsSummary> {
        let rate = sample_rate_hz.max(10).min(10000);
        let pid_val = target_pid.unwrap_or(0);
        self.active_pid.store(pid_val, Ordering::SeqCst);
        self.is_sampling.store(true, Ordering::SeqCst);

        {
            let mut sampler = self
                .sampler
                .write()
                .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;
            sampler.sample_rate_hz = rate;

            // Collect initial burst of 10 samples for instant observability
            for _ in 0..10 {
                sampler.sample_once(target_pid);
            }
        }

        // Add or update probe in registry
        let probe = PmuProbeConfig {
            id: format!("pmu-probe-{}", if pid_val > 0 { pid_val.to_string() } else { "global".to_string() }),
            target_pid,
            target_server: None,
            sample_rate_hz: rate,
            status: PmuProbeStatus::Active,
            enabled_events: vec![
                craft_core::pmu::PmuEventType::InstructionsRetired,
                craft_core::pmu::PmuEventType::CpuCycles,
                craft_core::pmu::PmuEventType::L1DReadMiss,
                craft_core::pmu::PmuEventType::LlcMiss,
                craft_core::pmu::PmuEventType::BranchMisprediction,
            ],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("PMU registry lock poisoned".to_string()))?;
            let _ = reg.add_probe(&self.paths, probe);
        }

        let summary = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &summary));

        info!(
            target_pid = ?target_pid,
            sample_rate_hz = rate,
            ipc = summary.ipc,
            cmpi_l1d = summary.cmpi_l1d,
            "Started autonomous hardware PMU instrumentation"
        );

        // Fire lifecycle hooks if thresholds exceeded or hotspots detected
        if let Some(top) = summary.top_hotspots.first() {
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::PmuHotspotDetected,
                HookContext::for_pmu_hotspot(&top.demangled_symbol, top.percentage, summary.ipc),
                5,
            );
        }

        if summary.cmpi_l1d > 0.05 {
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::CacheMissThresholdExceeded,
                HookContext::for_cache_miss_threshold(summary.cmpi_l1d, 0.05),
                5,
            );
        }

        if summary.bmpi > 0.02 {
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::BranchMispredictionSurge,
                HookContext::for_branch_misprediction(summary.bmpi, 0.02),
                5,
            );
        }

        Ok(summary)
    }

    /// Stop active hardware PMU sampling
    pub fn stop_sampling(&self) -> Result<PmuMetricsSummary> {
        self.is_sampling.store(false, Ordering::SeqCst);
        let pid_val = self.active_pid.load(Ordering::SeqCst);
        let probe_id = format!("pmu-probe-{}", if pid_val > 0 { pid_val.to_string() } else { "global".to_string() });

        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("PMU registry lock poisoned".to_string()))?;
            if let Some(mut p) = reg.get_probe(&probe_id).cloned() {
                p.status = PmuProbeStatus::Idle;
                let _ = reg.add_probe(&self.paths, p);
            }
        }

        let summary = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &summary));

        info!(probe_id = %probe_id, "Stopped hardware PMU sampling");
        Ok(summary)
    }

    /// Sample single counter reading immediately
    pub fn sample_now(&self, target_pid: Option<u32>) -> Result<PmuSampleRecord> {
        let mut sampler = self
            .sampler
            .write()
            .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;
        let rec = sampler.sample_once(target_pid);
        let summary = sampler.get_summary();
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &summary));
        Ok(rec)
    }

    /// Retrieve the top execution hotspot symbols
    pub fn get_hotspots(&self, limit: usize) -> Result<Vec<HotspotSymbol>> {
        let sampler = self
            .sampler
            .read()
            .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;
        let hotspots = sampler.get_top_hotspots(limit);
        if !hotspots.is_empty() {
            return Ok(hotspots);
        }
        if let Ok(state) = PmuRegistry::load_state(&self.paths) {
            if !state.top_hotspots.is_empty() {
                return Ok(state.top_hotspots.into_iter().take(limit).collect());
            }
        }
        Ok(Vec::new())
    }

    /// Run synthetic memory churn benchmark comparing sequential vs strided access
    pub fn run_bench(&self, iterations: usize) -> Result<MemoryChurnReport> {
        let mut sampler = self
            .sampler
            .write()
            .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;
        let report = sampler.run_synthetic_churn(iterations);
        let summary = sampler.get_summary();
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &summary));
        Ok(report)
    }

    /// Reset all collected metrics and clear sample buffers
    pub fn reset_metrics(&self) -> Result<PmuMetricsSummary> {
        {
            let mut sampler = self
                .sampler
                .write()
                .map_err(|_| CraftError::Other("PMU sampler lock poisoned".to_string()))?;
            sampler.reset_metrics();
        }
        let summary = PmuMetricsSummary::default();
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &summary));
        Ok(summary)
    }

    /// Render plain text status summary table (zero emojis)
    pub fn render_plain_status(&self) -> Result<String> {
        let summary = self.get_status()?;
        Ok(summary.render_plain_status())
    }

    /// Generate Prometheus telemetry metrics
    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        if let Ok(m) = self.get_status() {
            let sim_val = if m.simulation_mode { 1 } else { 0 };

            let _ = writeln!(out, "# HELP craft_pmu_active_probes Number of active hardware PMU probes");
            let _ = writeln!(out, "# TYPE craft_pmu_active_probes gauge");
            let _ = writeln!(out, "craft_pmu_active_probes {}", m.active_probes);

            let _ = writeln!(out, "# HELP craft_pmu_total_samples Total hardware counter samples recorded");
            let _ = writeln!(out, "# TYPE craft_pmu_total_samples counter");
            let _ = writeln!(out, "craft_pmu_total_samples {}", m.total_samples);

            let _ = writeln!(out, "# HELP craft_pmu_instructions_retired_total Retired hardware instructions");
            let _ = writeln!(out, "# TYPE craft_pmu_instructions_retired_total counter");
            let _ = writeln!(out, "craft_pmu_instructions_retired_total {}", m.instructions_retired);

            let _ = writeln!(out, "# HELP craft_pmu_cpu_cycles_total Total elapsed CPU cycles");
            let _ = writeln!(out, "# TYPE craft_pmu_cpu_cycles_total counter");
            let _ = writeln!(out, "craft_pmu_cpu_cycles_total {}", m.cpu_cycles);

            let _ = writeln!(out, "# HELP craft_pmu_ipc Instructions per CPU cycle");
            let _ = writeln!(out, "# TYPE craft_pmu_ipc gauge");
            let _ = writeln!(out, "craft_pmu_ipc {:.4}", m.ipc);

            let _ = writeln!(out, "# HELP craft_pmu_l1d_misses_total L1 data cache misses");
            let _ = writeln!(out, "# TYPE craft_pmu_l1d_misses_total counter");
            let _ = writeln!(out, "craft_pmu_l1d_misses_total {}", m.l1d_misses);

            let _ = writeln!(out, "# HELP craft_pmu_l1d_cmpi L1 data cache misses per retired instruction");
            let _ = writeln!(out, "# TYPE craft_pmu_l1d_cmpi gauge");
            let _ = writeln!(out, "craft_pmu_l1d_cmpi {:.6}", m.cmpi_l1d);

            let _ = writeln!(out, "# HELP craft_pmu_llc_misses_total Last level cache misses");
            let _ = writeln!(out, "# TYPE craft_pmu_llc_misses_total counter");
            let _ = writeln!(out, "craft_pmu_llc_misses_total {}", m.llc_misses);

            let _ = writeln!(out, "# HELP craft_pmu_llc_cmpi Last level cache misses per retired instruction");
            let _ = writeln!(out, "# TYPE craft_pmu_llc_cmpi gauge");
            let _ = writeln!(out, "craft_pmu_llc_cmpi {:.6}", m.cmpi_llc);

            let _ = writeln!(out, "# HELP craft_pmu_branch_mispredictions_total Branch mispredictions");
            let _ = writeln!(out, "# TYPE craft_pmu_branch_mispredictions_total counter");
            let _ = writeln!(out, "craft_pmu_branch_mispredictions_total {}", m.branch_mispredictions);

            let _ = writeln!(out, "# HELP craft_pmu_bmpi Branch mispredictions per retired instruction");
            let _ = writeln!(out, "# TYPE craft_pmu_bmpi gauge");
            let _ = writeln!(out, "craft_pmu_bmpi {:.6}", m.bmpi);

            let _ = writeln!(out, "# HELP craft_pmu_simulation_mode 1 if simulation fallback mode, 0 if native PMU");
            let _ = writeln!(out, "# TYPE craft_pmu_simulation_mode gauge");
            let _ = writeln!(out, "craft_pmu_simulation_mode {}", sim_val);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_pmu_service_lifecycle() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = PmuService::new(paths);

        let initial_status = service.get_status().unwrap();
        assert_eq!(initial_status.total_samples, 0);

        let sampled_status = service.start_sampling(Some(999), 100).unwrap();
        assert!(sampled_status.total_samples >= 10);
        assert!(sampled_status.active_probes >= 1);

        let hotspots = service.get_hotspots(5).unwrap();
        assert!(!hotspots.is_empty());

        let report = service.run_bench(1_500).unwrap();
        assert!(report.total_accesses >= 6_000);

        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_pmu_active_probes"));
        assert!(metrics.contains("craft_pmu_l1d_cmpi"));

        let stopped = service.stop_sampling().unwrap();
        assert_eq!(stopped.active_probes, 0);

        let reset = service.reset_metrics().unwrap();
        assert_eq!(reset.total_samples, 0);
    }
}
