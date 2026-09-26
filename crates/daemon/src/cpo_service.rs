// crates/daemon/src/cpo_service.rs
//
// Autonomous Silicon Photonic Co-Packaged Optics (CPO), Optical Neural Matrix Multiply & Sub-Nanosecond Direct Die Interconnects Supervisor Service.
// Pure-Rust MZI photonic meshes, micro-ring thermal regulation, and sub-nanosecond direct die interconnects.
// Strictly zero emojis.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use craft_core::cpo::{
    CpoBenchmarkMetrics, CpoMode, CpoRegistry, CpoStatusSummary, CpoThermalServoState,
    CpoTileDescriptor, MziMesh,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_net::cpo::{
    benchmark_cpo_interconnect, CpoThermalRegulator, PhotonicTensorEngine,
};

static INSTANCE: OnceLock<Arc<CpoService>> = OnceLock::new();

pub struct CpoService {
    paths: CraftPaths,
    registry: Mutex<CpoRegistry>,
    thermal_regulator: Mutex<CpoThermalRegulator>,
    engine: Mutex<PhotonicTensorEngine>,
    total_mvm_ops: Arc<AtomicU64>,
    total_photonic_joules_pj: Arc<AtomicU64>,
}

impl CpoService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = CpoRegistry::load_or_default(&paths).unwrap_or_default();
        let thermal_regulator = CpoThermalRegulator::new(45.0);
        let default_mesh = registry
            .meshes
            .first()
            .cloned()
            .unwrap_or_else(|| MziMesh::new("CPO-Default-Tensor-0", 4, 4));
        let engine = PhotonicTensorEngine::new(default_mesh);

        Self {
            paths,
            registry: Mutex::new(registry),
            thermal_regulator: Mutex::new(thermal_regulator),
            engine: Mutex::new(engine),
            total_mvm_ops: Arc::new(AtomicU64::new(0)),
            total_photonic_joules_pj: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<CpoStatusSummary> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(reg.summary())
    }

    pub fn set_mode(&self, new_mode: CpoMode, _server: Option<&str>) -> Result<bool> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.mode = new_mode;
        reg.save(&self.paths)?;
        Ok(true)
    }

    pub fn execute_mvm(
        &self,
        input: &[f32],
        _server: Option<&str>,
    ) -> Result<(Vec<f32>, u64, f64, f64)> {
        let engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let (output, mac, latency_ps, energy_pj) = engine.forward_vector(input);

        self.total_mvm_ops.fetch_add(mac, Ordering::Relaxed);
        self.total_photonic_joules_pj
            .fetch_add(energy_pj as u64, Ordering::Relaxed);

        Ok((output, mac, latency_ps, energy_pj))
    }

    pub fn adjust_thermal(&self, temp_c: f64, _server: Option<&str>) -> Result<CpoThermalServoState> {
        let mut regulator = self
            .thermal_regulator
            .lock()
            .map_err(|e| CraftError::Other(e.to_string()))?;
        let servo_state = regulator.step(temp_c);

        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.servo = servo_state.clone();
        reg.save(&self.paths)?;

        Ok(servo_state)
    }

    pub fn list_tiles(&self, _server: Option<&str>) -> Result<Vec<CpoTileDescriptor>> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(reg.tiles.clone())
    }

    pub fn run_bench(
        &self,
        iterations: usize,
        vector_dim: usize,
        _server: Option<&str>,
    ) -> Result<CpoBenchmarkMetrics> {
        let metrics = benchmark_cpo_interconnect(iterations, vector_dim);
        self.total_mvm_ops
            .fetch_add(metrics.mac_operations, Ordering::Relaxed);

        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.last_benchmark = Some(metrics.clone());
        reg.save(&self.paths)?;

        Ok(metrics)
    }

    pub fn reset_metrics(&self, _server: Option<&str>) -> Result<bool> {
        self.total_mvm_ops.store(0, Ordering::Relaxed);
        self.total_photonic_joules_pj.store(0, Ordering::Relaxed);

        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.last_benchmark = None;
        reg.save(&self.paths)?;

        Ok(true)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let status = match self.get_status(None) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let mut out = String::new();
        out.push_str("# HELP craft_cpo_aggregate_bandwidth_tbps Aggregate optical substrate bandwidth in Tbps\n");
        out.push_str("# TYPE craft_cpo_aggregate_bandwidth_tbps gauge\n");
        out.push_str(&format!("craft_cpo_aggregate_bandwidth_tbps {:.2}\n", status.aggregate_bandwidth_tbps));

        out.push_str("# HELP craft_cpo_average_latency_ps Mean die-to-die optical transit latency in picoseconds\n");
        out.push_str("# TYPE craft_cpo_average_latency_ps gauge\n");
        out.push_str(&format!("craft_cpo_average_latency_ps {:.1}\n", status.average_latency_ps));

        out.push_str("# HELP craft_cpo_active_tiles Active co-packaged optics tiles\n");
        out.push_str("# TYPE craft_cpo_active_tiles gauge\n");
        out.push_str(&format!("craft_cpo_active_tiles {}\n", status.active_tiles));

        out.push_str("# HELP craft_cpo_total_tiles Total co-packaged optics tiles on silicon interposer\n");
        out.push_str("# TYPE craft_cpo_total_tiles gauge\n");
        out.push_str(&format!("craft_cpo_total_tiles {}\n", status.total_tiles));

        out.push_str("# HELP craft_cpo_substrate_temp_c Substrate thermal core temperature in Celsius\n");
        out.push_str("# TYPE craft_cpo_substrate_temp_c gauge\n");
        out.push_str(&format!("craft_cpo_substrate_temp_c {:.2}\n", status.substrate_temp_c));

        out.push_str("# HELP craft_cpo_mvm_throughput_tops Photonic neural tensor matrix-vector multiply throughput in TOPS\n");
        out.push_str("# TYPE craft_cpo_mvm_throughput_tops gauge\n");
        out.push_str(&format!("craft_cpo_mvm_throughput_tops {:.2}\n", status.mvm_throughput_tops));

        out.push_str("# HELP craft_cpo_energy_efficiency_pj_per_mac Photonic neural energy efficiency in picojoules per MAC\n");
        out.push_str("# TYPE craft_cpo_energy_efficiency_pj_per_mac gauge\n");
        out.push_str(&format!("craft_cpo_energy_efficiency_pj_per_mac {:.4}\n", status.energy_efficiency_pj_per_mac));

        out.push_str("# HELP craft_cpo_cumulative_mvm_ops_total Cumulative optical MAC operations processed\n");
        out.push_str("# TYPE craft_cpo_cumulative_mvm_ops_total counter\n");
        out.push_str(&format!("craft_cpo_cumulative_mvm_ops_total {}\n", self.total_mvm_ops.load(Ordering::Relaxed)));

        out
    }
}
