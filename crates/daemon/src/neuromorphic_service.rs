// crates/daemon/src/neuromorphic_service.rs
//
// Autonomous Neuromorphic AI Tick Scheduling & Microsecond Latency Forecasting Service.
// Pure-Rust event-driven spike trains, STDP synaptic adaptation, and idle state compression.
// Strictly zero emojis.

use craft_core::error::{CraftError, Result};
use craft_core::neuromorphic::{
    current_unix_millis, MembraneRasterPoint, NeuromorphicBenchmarkMetrics,
    NeuromorphicRegistry, NeuromorphicScheduleMode, NeuromorphicStatusSummary,
    SpikeEvent, SpikeSourceType, TickPrediction,
};
use craft_core::path::CraftPaths;
use craft_net::neuromorphic::{benchmark_neuromorphic_scheduler, SpikeNeuralNetwork};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static INSTANCE: OnceLock<Arc<NeuromorphicService>> = OnceLock::new();

pub struct NeuromorphicService {
    paths: CraftPaths,
    snn: Mutex<SpikeNeuralNetwork>,
    spikes_total: Arc<AtomicU64>,
    inferences_total: Arc<AtomicU64>,
    stdp_updates_total: Arc<AtomicU64>,
}

impl NeuromorphicService {
    pub fn new(paths: CraftPaths) -> Self {
        let _registry = NeuromorphicRegistry::load(&paths).unwrap_or_default();
        let mut snn = SpikeNeuralNetwork::new(16, 32, 16);

        // Seed initial quiescent background spikes for baseline raster visualization
        let now_ns = current_unix_millis() * 1_000_000;
        for i in 0..8 {
            snn.inject_external_spike(SpikeEvent {
                neuron_id: i,
                timestamp_ns: now_ns + (i as u64 * 5_000),
                weight: 0.8,
                source_type: SpikeSourceType::SchedulerTimer,
            });
        }
        let (_spikes, _pred) = snn.step(now_ns + 50_000);

        let initial_spikes = snn.total_spikes_processed;
        let initial_stdp = snn.total_stdp_updates;

        Self {
            paths,
            snn: Mutex::new(snn),
            spikes_total: Arc::new(AtomicU64::new(initial_spikes)),
            inferences_total: Arc::new(AtomicU64::new(1)),
            stdp_updates_total: Arc::new(AtomicU64::new(initial_stdp)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, server: Option<&str>) -> Result<NeuromorphicStatusSummary> {
        let server_name = server.unwrap_or("default");
        let snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut summary = snn.get_status_summary();

        // Check if there is persisted status override in registry
        if let Ok(reg) = NeuromorphicRegistry::load(&self.paths) {
            if let Some(persisted) = reg.servers.get(server_name) {
                summary.mode = persisted.mode;
            }
        }

        Ok(summary)
    }

    pub fn set_mode(&self, server: Option<&str>, mode_str: &str) -> Result<()> {
        let server_name = server.unwrap_or("default");
        let mode: NeuromorphicScheduleMode = mode_str.parse()?;

        let mut snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        snn.set_mode(mode);

        let mut registry = NeuromorphicRegistry::load(&self.paths).unwrap_or_default();
        let summary = registry
            .servers
            .entry(server_name.to_string())
            .or_insert_with(NeuromorphicStatusSummary::default);
        summary.mode = mode;
        registry.updated_at = current_unix_millis();
        registry.save(&self.paths)?;

        Ok(())
    }

    pub fn inject_spike(
        &self,
        server: Option<&str>,
        neuron_id: u32,
        weight: f32,
        source_str: &str,
    ) -> Result<u64> {
        let _server_name = server.unwrap_or("default");
        let source_type: SpikeSourceType = source_str.parse()?;
        let now_ns = current_unix_millis() * 1_000_000;

        let spike = SpikeEvent {
            neuron_id,
            timestamp_ns: now_ns,
            weight,
            source_type,
        };

        let mut snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        snn.inject_external_spike(spike);
        let (_spikes, _pred) = snn.step(now_ns);

        let spike_id = self.spikes_total.fetch_add(1, Ordering::SeqCst) + 1;
        self.inferences_total.fetch_add(1, Ordering::Relaxed);
        self.stdp_updates_total.store(snn.total_stdp_updates, Ordering::Relaxed);

        Ok(spike_id)
    }

    pub fn get_raster(&self, _server: Option<&str>, limit: Option<usize>) -> Result<Vec<MembraneRasterPoint>> {
        let snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(snn.get_raster_points(limit.unwrap_or(50)))
    }

    pub fn get_prediction(&self, _server: Option<&str>) -> Result<TickPrediction> {
        let now_ns = current_unix_millis() * 1_000_000;
        let mut snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let (_spikes, pred) = snn.step(now_ns);
        self.inferences_total.fetch_add(1, Ordering::Relaxed);
        Ok(pred)
    }

    pub fn run_bench(
        &self,
        iterations: Option<usize>,
        burst_ratio: Option<f64>,
    ) -> Result<NeuromorphicBenchmarkMetrics> {
        let iters = iterations.unwrap_or(100);
        let burst = burst_ratio.unwrap_or(0.20);

        let metrics = benchmark_neuromorphic_scheduler(iters, burst);

        // Update registry
        let mut registry = NeuromorphicRegistry::load(&self.paths).unwrap_or_default();
        registry.metrics = metrics.clone();
        registry.updated_at = current_unix_millis();
        let _ = registry.save(&self.paths);

        Ok(metrics)
    }

    pub fn reset_metrics(&self, server: Option<&str>) -> Result<()> {
        let server_name = server.unwrap_or("default");
        let mut snn = self.snn.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        snn.reset_metrics();

        self.spikes_total.store(0, Ordering::SeqCst);
        self.inferences_total.store(0, Ordering::SeqCst);
        self.stdp_updates_total.store(0, Ordering::SeqCst);

        let mut registry = NeuromorphicRegistry::load(&self.paths).unwrap_or_default();
        if let Some(status) = registry.servers.get_mut(server_name) {
            *status = NeuromorphicStatusSummary::default();
        }
        registry.metrics = NeuromorphicBenchmarkMetrics {
            iterations: 0,
            total_spikes_injected: 0,
            spikes_per_sec: 0.0,
            avg_inference_micros: 0.0,
            p99_inference_micros: 0.0,
            idle_power_reduction_percent: 0.0,
            tick_accuracy_percent: 0.0,
        };
        registry.updated_at = current_unix_millis();
        registry.save(&self.paths)?;

        Ok(())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let snn = match self.snn.lock() {
            Ok(guard) => guard,
            Err(_) => return String::new(),
        };

        let summary = snn.get_status_summary();
        let mut out = String::new();

        out.push_str("# HELP craft_neuromorphic_active_models Total active neuromorphic server models\n");
        out.push_str("# TYPE craft_neuromorphic_active_models gauge\n");
        out.push_str("craft_neuromorphic_active_models 1\n");

        out.push_str("# HELP craft_neuromorphic_total_neurons Total active LIF neurons\n");
        out.push_str("# TYPE craft_neuromorphic_total_neurons gauge\n");
        out.push_str(&format!("craft_neuromorphic_total_neurons {}\n", summary.total_neurons));

        out.push_str("# HELP craft_neuromorphic_total_synapses Total active synaptic connections\n");
        out.push_str("# TYPE craft_neuromorphic_total_synapses gauge\n");
        out.push_str(&format!("craft_neuromorphic_total_synapses {}\n", summary.total_synapses));

        out.push_str("# HELP craft_neuromorphic_spikes_processed_total Total discrete spike events processed\n");
        out.push_str("# TYPE craft_neuromorphic_spikes_processed_total counter\n");
        out.push_str(&format!("craft_neuromorphic_spikes_processed_total {}\n", summary.spikes_processed));

        out.push_str("# HELP craft_neuromorphic_inference_duration_micros SNN forward inference latency in microseconds\n");
        out.push_str("# TYPE craft_neuromorphic_inference_duration_micros gauge\n");
        out.push_str(&format!("craft_neuromorphic_inference_duration_micros {:.3}\n", summary.inference_latency_micros));

        out.push_str("# HELP craft_neuromorphic_idle_power_reduction_percent Estimated idle CPU power reduction percent\n");
        out.push_str("# TYPE craft_neuromorphic_idle_power_reduction_percent gauge\n");
        out.push_str(&format!("craft_neuromorphic_idle_power_reduction_percent {:.1}\n", summary.idle_cpu_saved_percent));

        out.push_str("# HELP craft_neuromorphic_predicted_mspt_micros Predicted MSPT in microseconds\n");
        out.push_str("# TYPE craft_neuromorphic_predicted_mspt_micros gauge\n");
        out.push_str(&format!("craft_neuromorphic_predicted_mspt_micros {:.1}\n", summary.predicted_mspt_micros));

        out.push_str("# HELP craft_neuromorphic_stdp_updates_total Total on-line STDP synaptic weight updates\n");
        out.push_str("# TYPE craft_neuromorphic_stdp_updates_total counter\n");
        out.push_str(&format!("craft_neuromorphic_stdp_updates_total {}\n", summary.stdp_weight_updates));

        out
    }
}
