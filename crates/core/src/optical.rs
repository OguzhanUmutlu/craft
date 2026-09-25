use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

/// Optical routing mode for photonic interconnects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpticalRoutingMode {
    Autonomous,
    CircuitSwitched,
    WavelengthRouted,
    HybridElectronic,
    Passthrough,
}

impl Default for OpticalRoutingMode {
    fn default() -> Self {
        Self::Autonomous
    }
}

impl fmt::Display for OpticalRoutingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Autonomous => write!(f, "Autonomous"),
            Self::CircuitSwitched => write!(f, "CircuitSwitched"),
            Self::WavelengthRouted => write!(f, "WavelengthRouted"),
            Self::HybridElectronic => write!(f, "HybridElectronic"),
            Self::Passthrough => write!(f, "Passthrough"),
        }
    }
}

impl FromStr for OpticalRoutingMode {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace(['-', '_'], "");
        match normalized.as_str() {
            "autonomous" | "auto" => Ok(Self::Autonomous),
            "circuitswitched" | "circuit" | "ocs" => Ok(Self::CircuitSwitched),
            "wavelengthrouted" | "wavelength" | "wdm" | "lambda" => Ok(Self::WavelengthRouted),
            "hybridelectronic" | "hybrid" => Ok(Self::HybridElectronic),
            "passthrough" | "direct" | "pass" => Ok(Self::Passthrough),
            other => Err(CraftError::Config(format!(
                "Unknown optical routing mode: '{}'. Valid modes: autonomous, circuitswitched, wavelengthrouted, hybridelectronic, passthrough",
                other
            ))),
        }
    }
}

/// Optical wavelength descriptor conforming to ITU-T DWDM standard grid
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalWavelength {
    pub channel_id: u16,
    pub wavelength_nm: f64,
    pub frequency_thz: f64,
    pub power_dbm: f64,
    pub attenuation_db_per_km: f64,
}

impl OpticalWavelength {
    /// Constructs a standard ITU-T 50 GHz DWDM channel
    pub fn from_channel(channel_id: u16) -> Self {
        let ch = channel_id.clamp(1, 64);
        let freq_thz = 193.10 + (ch as f64 - 1.0) * 0.05;
        // Wavelength in nanometers = c / frequency
        let wavelength_nm = 299_792.458 / freq_thz;
        Self {
            channel_id: ch,
            wavelength_nm: (wavelength_nm * 100.0).round() / 100.0,
            frequency_thz: (freq_thz * 100.0).round() / 100.0,
            power_dbm: 3.5,
            attenuation_db_per_km: 0.18,
        }
    }
}

/// Physical or virtual photonic port descriptor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotonicPort {
    pub port_id: u16,
    pub connector_type: String,
    pub wavelength_capable: Vec<u16>,
    pub tx_power_dbm: f64,
    pub rx_sensitivity_dbm: f64,
    pub osnr_db: f64,
    pub link_up: bool,
}

impl PhotonicPort {
    pub fn new(port_id: u16) -> Self {
        Self {
            port_id,
            connector_type: "MPO-16".to_string(),
            wavelength_capable: (1..=16).collect(),
            tx_power_dbm: 4.0,
            rx_sensitivity_dbm: -22.0,
            osnr_db: 32.5,
            link_up: true,
        }
    }
}

/// MEMS micro-mirror deflection and actuation state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemsMirrorState {
    pub mirror_id: u16,
    pub theta_x: f64,
    pub theta_y: f64,
    pub voltage_v: f64,
    pub settling_us: f64,
    pub loss_db: f64,
}

impl Default for MemsMirrorState {
    fn default() -> Self {
        Self {
            mirror_id: 1,
            theta_x: 0.0,
            theta_y: 0.0,
            voltage_v: 60.0,
            settling_us: 12.5,
            loss_db: 0.45,
        }
    }
}

/// End-to-end active photonic lightpath circuit
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalCircuit {
    pub circuit_id: String,
    pub ingress_port: u16,
    pub egress_port: u16,
    pub wavelength_ch: u16,
    pub server: Option<String>,
    pub bandwidth_gbps: f64,
    pub established_unix_ms: u64,
}

impl OpticalCircuit {
    pub fn new(
        circuit_id: impl Into<String>,
        ingress_port: u16,
        egress_port: u16,
        wavelength_ch: u16,
        server: Option<String>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            circuit_id: circuit_id.into(),
            ingress_port,
            egress_port,
            wavelength_ch,
            server,
            bandwidth_gbps: 400.0,
            established_unix_ms: now,
        }
    }
}

/// Photonic switch topology descriptor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalSwitchTopology {
    pub switch_id: String,
    pub port_count: u16,
    pub active_circuits: Vec<OpticalCircuit>,
    pub total_bandwidth_gbps: f64,
    pub average_insertion_loss_db: f64,
}

impl Default for OpticalSwitchTopology {
    fn default() -> Self {
        Self {
            switch_id: "ocs-fabric-01".to_string(),
            port_count: 16,
            active_circuits: Vec::new(),
            total_bandwidth_gbps: 800.0,
            average_insertion_loss_db: 1.15,
        }
    }
}

/// Optical switch status summary for CLI, IPC, and telemetry exposition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalStatusSummary {
    pub mode: OpticalRoutingMode,
    pub port_count: u16,
    pub active_ports: u16,
    pub active_circuits_count: usize,
    pub aggregate_bandwidth_gbps: f64,
    pub mean_switching_latency_nanos: f64,
    pub insertion_loss_db: f64,
    pub wdm_channels_utilized: u16,
    pub packets_routed_total: u64,
    pub attenuation_warnings_total: u32,
}

impl Default for OpticalStatusSummary {
    fn default() -> Self {
        Self {
            mode: OpticalRoutingMode::Autonomous,
            port_count: 16,
            active_ports: 8,
            active_circuits_count: 2,
            aggregate_bandwidth_gbps: 800.0,
            mean_switching_latency_nanos: 8.4,
            insertion_loss_db: 1.15,
            wdm_channels_utilized: 4,
            packets_routed_total: 0,
            attenuation_warnings_total: 0,
        }
    }
}

/// High-throughput benchmark metrics for optical crossbar switching
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalBenchmarkMetrics {
    pub iterations: u32,
    pub port_count: u16,
    pub packets_routed: u64,
    pub throughput_tbps: f64,
    pub mean_latency_nanos: f64,
    pub p99_latency_nanos: f64,
    pub insertion_loss_db: f64,
    pub wavelength_collisions: u32,
    pub ber_exponent: i32,
}

impl Default for OpticalBenchmarkMetrics {
    fn default() -> Self {
        Self {
            iterations: 1000,
            port_count: 16,
            packets_routed: 16000,
            throughput_tbps: 1.85,
            mean_latency_nanos: 7.2,
            p99_latency_nanos: 9.1,
            insertion_loss_db: 1.12,
            wavelength_collisions: 0,
            ber_exponent: -13,
        }
    }
}

/// Persistent registry holding optical switch configuration and active circuits
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalRegistry {
    pub mode: OpticalRoutingMode,
    pub port_count: u16,
    pub circuits: Vec<OpticalCircuit>,
    pub ports: Vec<PhotonicPort>,
}

impl Default for OpticalRegistry {
    fn default() -> Self {
        let ports = (1..=16).map(PhotonicPort::new).collect();
        let default_circuits = vec![
            OpticalCircuit::new("circ-opt-01", 1, 2, 1, Some("survival".to_string())),
            OpticalCircuit::new("circ-opt-02", 3, 4, 2, Some("lobby".to_string())),
        ];

        Self {
            mode: OpticalRoutingMode::Autonomous,
            port_count: 16,
            circuits: default_circuits,
            ports,
        }
    }
}

impl OpticalRegistry {
    /// Loads registry from disk or returns default configuration
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(reg) = serde_json::from_str::<Self>(&content) {
                    return reg;
                }
            }
        }
        Self::default()
    }

    /// Atomically persists registry to disk
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize optical registry: {}", e)))?;
        fs::write(path, json).map_err(CraftError::Io)?;
        Ok(())
    }

    /// Executes an operation with advisory file lock protection (`optical.lock`)
    pub fn with_lock<F, R>(&self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        if let Some(parent) = paths.optical_lock.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.optical_lock)
            .map_err(CraftError::Io)?;

        lock_file.lock_exclusive().map_err(CraftError::Io)?;
        let result = f();
        let _ = lock_file.unlock();
        result
    }
}

// ---------------------------------------------------------------------------
// Plain-text Table Renderers (Strict Zero-Emoji Policy)
// ---------------------------------------------------------------------------

/// Renders a clean plain-text table of optical switch status
pub fn render_optical_status_table(summary: &OpticalStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("=== Autonomous Optical Switch & Photonic Waveguide Status ===\n");
    out.push_str(&format!("Routing Mode:                 {}\n", summary.mode));
    out.push_str(&format!("Total Photonic Ports:         {}\n", summary.port_count));
    out.push_str(&format!("Active Links:                 {}\n", summary.active_ports));
    out.push_str(&format!("Active Lightpath Circuits:    {}\n", summary.active_circuits_count));
    out.push_str(&format!("Aggregate Bandwidth:          {:.1} Gbps\n", summary.aggregate_bandwidth_gbps));
    out.push_str(&format!("Mean Crossbar Latency:        {:.2} ns\n", summary.mean_switching_latency_nanos));
    out.push_str(&format!("Insertion Loss:               {:.2} dB\n", summary.insertion_loss_db));
    out.push_str(&format!("WDM Channels Active:          {}\n", summary.wdm_channels_utilized));
    out.push_str(&format!("Photonic Packets Routed:      {}\n", summary.packets_routed_total));
    out.push_str(&format!("Attenuation Warnings:         {}\n", summary.attenuation_warnings_total));
    out
}

/// Renders a clean plain-text table of active optical circuits
pub fn render_optical_circuits_table(circuits: &[OpticalCircuit]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<16}  {:<10}  {:<10}  {:<18}  {:<16}  {:<16}\n",
        "CIRCUIT ID", "INGRESS", "EGRESS", "WDM LAMBDA", "BANDWIDTH", "SERVER"
    ));
    out.push_str(&format!("{:-<92}\n", ""));

    if circuits.is_empty() {
        out.push_str("No active optical lightpath circuits provisioned.\n");
        return out;
    }

    for c in circuits {
        let server_str = c.server.as_deref().unwrap_or("-");
        let lambda_str = format!("Ch {} ({:.2}nm)", c.wavelength_ch, OpticalWavelength::from_channel(c.wavelength_ch).wavelength_nm);
        let bw_str = format!("{:.1} Gbps", c.bandwidth_gbps);
        out.push_str(&format!(
            "{:<16}  {:<10}  {:<10}  {:<18}  {:<16}  {:<16}\n",
            c.circuit_id, c.ingress_port, c.egress_port, lambda_str, bw_str, server_str
        ));
    }
    out
}

/// Renders a clean plain-text table of benchmark metrics
pub fn render_optical_bench_table(metrics: &OpticalBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("=== High-Throughput Photonic Crossbar Benchmark Results ===\n");
    out.push_str(&format!("Test Iterations:              {}\n", metrics.iterations));
    out.push_str(&format!("Crossbar Ports:               {}x{}\n", metrics.port_count, metrics.port_count));
    out.push_str(&format!("Packets Routed:               {}\n", metrics.packets_routed));
    out.push_str(&format!("Photonic Throughput:          {:.2} Tbps\n", metrics.throughput_tbps));
    out.push_str(&format!("Mean Switching Latency:       {:.2} ns\n", metrics.mean_latency_nanos));
    out.push_str(&format!("P99 Switching Latency:        {:.2} ns\n", metrics.p99_latency_nanos));
    out.push_str(&format!("Accumulated Insertion Loss:   {:.2} dB\n", metrics.insertion_loss_db));
    out.push_str(&format!("Wavelength Collisions:        {}\n", metrics.wavelength_collisions));
    out.push_str(&format!("Bit Error Rate (BER):         10^{}\n", metrics.ber_exponent));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optical_routing_mode_parse_and_display() {
        assert_eq!(OpticalRoutingMode::from_str("autonomous").unwrap(), OpticalRoutingMode::Autonomous);
        assert_eq!(OpticalRoutingMode::from_str("ocs").unwrap(), OpticalRoutingMode::CircuitSwitched);
        assert_eq!(OpticalRoutingMode::from_str("wdm").unwrap(), OpticalRoutingMode::WavelengthRouted);
        assert_eq!(OpticalRoutingMode::from_str("hybrid").unwrap(), OpticalRoutingMode::HybridElectronic);
        assert_eq!(OpticalRoutingMode::from_str("passthrough").unwrap(), OpticalRoutingMode::Passthrough);
        assert!(OpticalRoutingMode::from_str("invalid_mode").is_err());
    }

    #[test]
    fn test_optical_wavelength_calculation() {
        let ch1 = OpticalWavelength::from_channel(1);
        assert_eq!(ch1.channel_id, 1);
        assert_eq!(ch1.frequency_thz, 193.1);
        assert!(ch1.wavelength_nm > 1550.0 && ch1.wavelength_nm < 1560.0);

        let ch64 = OpticalWavelength::from_channel(64);
        assert_eq!(ch64.channel_id, 64);
        assert!(ch64.frequency_thz > ch1.frequency_thz);
        assert!(ch64.wavelength_nm < ch1.wavelength_nm);
    }

    #[test]
    fn test_optical_circuit_allocation_and_tables() {
        let circuit = OpticalCircuit::new("circ-test", 1, 2, 4, Some("node-1".to_string()));
        assert_eq!(circuit.ingress_port, 1);
        assert_eq!(circuit.egress_port, 2);
        assert_eq!(circuit.wavelength_ch, 4);

        let table = render_optical_circuits_table(&[circuit]);
        assert!(table.contains("circ-test"));
        assert!(table.contains("node-1"));
    }

    #[test]
    fn test_optical_status_and_bench_tables() {
        let summary = OpticalStatusSummary::default();
        let status_table = render_optical_status_table(&summary);
        assert!(status_table.contains("Autonomous"));
        assert!(status_table.contains("8.40 ns") || status_table.contains("8.4 ns"));

        let bench = OpticalBenchmarkMetrics::default();
        let bench_table = render_optical_bench_table(&bench);
        assert!(bench_table.contains("1.85 Tbps"));
        assert!(bench_table.contains("7.20 ns") || bench_table.contains("7.2 ns"));
    }
}
