// crates/daemon/src/optical_service.rs
//
// Autonomous Optical Network Switching & Photonic Interconnect Supervisor Service.
// Pure-Rust MEMS crossbar, WDM wavelength multiplexing, and line-rate nanosecond waveguide routing.
// Strictly zero emojis.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use craft_core::error::{CraftError, Result};
use craft_core::optical::{
    OpticalBenchmarkMetrics, OpticalCircuit, OpticalRegistry, OpticalRoutingMode,
    OpticalStatusSummary,
};
use craft_core::path::CraftPaths;
use craft_net::optical::{benchmark_optical_crossbar, MemsCrossbarSwitch, WdmMultiplexer};

static INSTANCE: OnceLock<Arc<OpticalSwitchService>> = OnceLock::new();

pub struct OpticalSwitchService {
    paths: CraftPaths,
    switch: Mutex<MemsCrossbarSwitch>,
    wdm: Mutex<WdmMultiplexer>,
    mode: Mutex<OpticalRoutingMode>,
    packets_total: Arc<AtomicU64>,
    attenuation_warnings_total: Arc<AtomicU32>,
}

impl OpticalSwitchService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = OpticalRegistry::load_or_default(&paths.optical_registry_file);
        let mut switch = MemsCrossbarSwitch::new(registry.port_count);
        let mut wdm = WdmMultiplexer::new();

        for circuit in &registry.circuits {
            let _ = switch.provision_circuit(circuit.clone());
            let _ = wdm.allocate_channel(circuit.wavelength_ch, circuit.circuit_id.clone());
        }

        Self {
            paths,
            switch: Mutex::new(switch),
            wdm: Mutex::new(wdm),
            mode: Mutex::new(registry.mode),
            packets_total: Arc::new(AtomicU64::new(0)),
            attenuation_warnings_total: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<OpticalStatusSummary> {
        let switch = self.switch.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mode = *self.mode.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut summary = switch.status_summary(mode);

        summary.packets_routed_total += self.packets_total.load(Ordering::Relaxed);
        summary.attenuation_warnings_total += self.attenuation_warnings_total.load(Ordering::Relaxed);
        Ok(summary)
    }

    pub fn set_mode(&self, new_mode: OpticalRoutingMode, _server: Option<&str>) -> Result<bool> {
        let mut mode = self.mode.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        *mode = new_mode;

        // Persist to registry
        let mut registry = OpticalRegistry::load_or_default(&self.paths.optical_registry_file);
        registry.mode = new_mode;
        let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.optical_registry_file));
        Ok(true)
    }

    pub fn create_circuit(
        &self,
        circuit_id: String,
        ingress_port: u16,
        egress_port: u16,
        wavelength_ch: u16,
        server: Option<String>,
    ) -> Result<OpticalCircuit> {
        let mut switch = self.switch.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut wdm = self.wdm.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let circuit = OpticalCircuit::new(&circuit_id, ingress_port, egress_port, wavelength_ch, server);

        switch
            .provision_circuit(circuit.clone())
            .map_err(CraftError::Other)?;

        let _ = wdm.allocate_channel(wavelength_ch, circuit_id.clone());

        // Persist to registry
        let mut registry = OpticalRegistry::load_or_default(&self.paths.optical_registry_file);
        registry.circuits.retain(|c| c.circuit_id != circuit_id);
        registry.circuits.push(circuit.clone());
        let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.optical_registry_file));

        Ok(circuit)
    }

    pub fn delete_circuit(&self, circuit_id: &str) -> Result<bool> {
        let mut switch = self.switch.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut wdm = self.wdm.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        if let Some(c) = switch.active_circuits.get(circuit_id).cloned() {
            wdm.release_channel(c.wavelength_ch);
        }

        let removed = switch.teardown_circuit(circuit_id);

        if removed {
            let mut registry = OpticalRegistry::load_or_default(&self.paths.optical_registry_file);
            registry.circuits.retain(|c| c.circuit_id != circuit_id);
            let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.optical_registry_file));
        }

        Ok(removed)
    }

    pub fn list_circuits(&self, server: Option<&str>) -> Result<Vec<OpticalCircuit>> {
        let switch = self.switch.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let circuits: Vec<OpticalCircuit> = switch
            .active_circuits
            .values()
            .filter(|c| {
                if let Some(srv) = server {
                    c.server.as_deref() == Some(srv)
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        Ok(circuits)
    }

    pub fn run_bench(&self, iterations: u32, port_count: u16) -> Result<OpticalBenchmarkMetrics> {
        let metrics = benchmark_optical_crossbar(iterations, port_count);
        self.packets_total.fetch_add(metrics.packets_routed, Ordering::Relaxed);
        Ok(metrics)
    }

    pub fn reset_metrics(&self) -> Result<bool> {
        self.packets_total.store(0, Ordering::Relaxed);
        self.attenuation_warnings_total.store(0, Ordering::Relaxed);
        let mut switch = self.switch.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        switch.packets_routed = 0;
        switch.attenuation_warnings = 0;
        Ok(true)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let status = match self.get_status(None) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let mut out = String::new();
        out.push_str("# HELP craft_optical_active_ports Total active optical links\n");
        out.push_str("# TYPE craft_optical_active_ports gauge\n");
        out.push_str(&format!("craft_optical_active_ports {}\n", status.active_ports));

        out.push_str("# HELP craft_optical_active_circuits Total active optical lightpaths\n");
        out.push_str("# TYPE craft_optical_active_circuits gauge\n");
        out.push_str(&format!("craft_optical_active_circuits {}\n", status.active_circuits_count));

        out.push_str("# HELP craft_optical_aggregate_bandwidth_gbps Total optical switch aggregate bandwidth\n");
        out.push_str("# TYPE craft_optical_aggregate_bandwidth_gbps gauge\n");
        out.push_str(&format!("craft_optical_aggregate_bandwidth_gbps {}\n", status.aggregate_bandwidth_gbps));

        out.push_str("# HELP craft_optical_switching_latency_nanos Mean crossbar nanosecond switching latency\n");
        out.push_str("# TYPE craft_optical_switching_latency_nanos gauge\n");
        out.push_str(&format!("craft_optical_switching_latency_nanos {}\n", status.mean_switching_latency_nanos));

        out.push_str("# HELP craft_optical_insertion_loss_db Waveguide insertion loss in decibels\n");
        out.push_str("# TYPE craft_optical_insertion_loss_db gauge\n");
        out.push_str(&format!("craft_optical_insertion_loss_db {}\n", status.insertion_loss_db));

        out.push_str("# HELP craft_optical_wdm_channels_utilized Total active WDM wavelength channels\n");
        out.push_str("# TYPE craft_optical_wdm_channels_utilized gauge\n");
        out.push_str(&format!("craft_optical_wdm_channels_utilized {}\n", status.wdm_channels_utilized));

        out.push_str("# HELP craft_optical_photonic_packets_routed_total Cumulative optical packets switched\n");
        out.push_str("# TYPE craft_optical_photonic_packets_routed_total counter\n");
        out.push_str(&format!("craft_optical_photonic_packets_routed_total {}\n", status.packets_routed_total));

        out.push_str("# HELP craft_optical_attenuation_warnings_total Cumulative link attenuation warnings\n");
        out.push_str("# TYPE craft_optical_attenuation_warnings_total counter\n");
        out.push_str(&format!("craft_optical_attenuation_warnings_total {}\n", status.attenuation_warnings_total));

        out
    }
}
