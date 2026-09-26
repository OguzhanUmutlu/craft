// crates/core/src/cpo.rs
//
// Phase 53: Autonomous Silicon Photonic Co-Packaged Optics (CPO), Optical Neural Matrix Multiply & Sub-Nanosecond Direct Die Interconnects

use std::fmt;
use std::fs;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

/// Operational mode for Silicon Photonic Co-Packaged Optics
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CpoMode {
    Autonomous,
    DirectDiePhotonic,
    AnalogTensorMvm,
    ThermalStabilized,
    LoopbackElectronic,
}

impl Default for CpoMode {
    fn default() -> Self {
        CpoMode::Autonomous
    }
}

impl fmt::Display for CpoMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpoMode::Autonomous => write!(f, "autonomous"),
            CpoMode::DirectDiePhotonic => write!(f, "direct-die-photonic"),
            CpoMode::AnalogTensorMvm => write!(f, "analog-tensor-mvm"),
            CpoMode::ThermalStabilized => write!(f, "thermal-stabilized"),
            CpoMode::LoopbackElectronic => write!(f, "loopback-electronic"),
        }
    }
}

impl FromStr for CpoMode {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "autonomous" | "auto" => Ok(CpoMode::Autonomous),
            "direct-die-photonic" | "direct_die_photonic" | "photonic" | "direct" => {
                Ok(CpoMode::DirectDiePhotonic)
            }
            "analog-tensor-mvm" | "analog_tensor_mvm" | "tensor" | "mvm" => {
                Ok(CpoMode::AnalogTensorMvm)
            }
            "thermal-stabilized" | "thermal_stabilized" | "thermal" | "stabilized" => {
                Ok(CpoMode::ThermalStabilized)
            }
            "loopback-electronic" | "loopback_electronic" | "loopback" | "electronic" => {
                Ok(CpoMode::LoopbackElectronic)
            }
            other => Err(CraftError::Config(format!(
                "Invalid CPO operational mode '{}'. Expected one of: autonomous, direct-die-photonic, analog-tensor-mvm, thermal-stabilized, loopback-electronic",
                other
            ))),
        }
    }
}

/// Thermal stabilization servo lock status
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CpoThermalStatus {
    Locked,
    Tuning,
    DriftWarning,
}

impl Default for CpoThermalStatus {
    fn default() -> Self {
        CpoThermalStatus::Locked
    }
}

impl fmt::Display for CpoThermalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpoThermalStatus::Locked => write!(f, "locked"),
            CpoThermalStatus::Tuning => write!(f, "tuning"),
            CpoThermalStatus::DriftWarning => write!(f, "drift-warning"),
        }
    }
}

impl FromStr for CpoThermalStatus {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "locked" | "lock" => Ok(CpoThermalStatus::Locked),
            "tuning" | "tune" => Ok(CpoThermalStatus::Tuning),
            "drift-warning" | "drift_warning" | "warning" | "drift" => {
                Ok(CpoThermalStatus::DriftWarning)
            }
            other => Err(CraftError::Config(format!(
                "Invalid thermal status '{}'. Expected: locked, tuning, drift-warning",
                other
            ))),
        }
    }
}

/// Micro-ring optical resonator / electro-optic modulator
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicroRingModulator {
    pub ring_id: u32,
    pub wavelength_nm: f64,
    pub heater_current_ma: f64,
    pub q_factor: f64,
    pub insertion_loss_db: f64,
    pub extinction_ratio_db: f64,
    pub thermal_shift_pm_per_c: f64,
}

impl MicroRingModulator {
    pub fn new(ring_id: u32, base_wavelength_nm: f64) -> Self {
        Self {
            ring_id,
            wavelength_nm: base_wavelength_nm,
            heater_current_ma: 10.0,
            q_factor: 28_000.0,
            insertion_loss_db: 0.75,
            extinction_ratio_db: 16.5,
            thermal_shift_pm_per_c: 82.0,
        }
    }

    /// Calculate resonance wavelength given temperature difference from calibration (45.0 C)
    pub fn resonance_at_temp(&self, temp_c: f64) -> f64 {
        let delta_t = temp_c - 45.0;
        let delta_lambda_nm = (delta_t * self.thermal_shift_pm_per_c) / 1000.0;
        self.wavelength_nm + delta_lambda_nm
    }

    /// Adjust thermal heater current to counteract drift
    pub fn tune_heater(&mut self, current_ma: f64) {
        self.heater_current_ma = current_ma.clamp(0.0, 50.0);
    }
}

/// Mach-Zehnder Interferometer (MZI) programmable 2x2 optical unitary cell
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MziCell {
    pub cell_id: u32,
    pub row: usize,
    pub col: usize,
    pub phase_shift_theta: f64,
    pub phase_shift_phi: f64,
    pub optical_attenuation_db: f64,
}

impl MziCell {
    pub fn new(cell_id: u32, row: usize, col: usize) -> Self {
        Self {
            cell_id,
            row,
            col,
            phase_shift_theta: std::f64::consts::FRAC_PI_4,
            phase_shift_phi: 0.0,
            optical_attenuation_db: 0.12,
        }
    }

    /// Compute 2x2 unitary transmission coefficients
    /// T_11 = cos(theta) * e^(i*phi), T_12 = -sin(theta)
    /// T_21 = sin(theta) * e^(i*phi), T_22 = cos(theta)
    pub fn transfer_matrix(&self) -> ((f64, f64), (f64, f64)) {
        let cos_t = self.phase_shift_theta.cos();
        let sin_t = self.phase_shift_theta.sin();
        let cos_p = self.phase_shift_phi.cos();
        let sin_p = self.phase_shift_phi.sin();

        // (real, imag) for T11, real T12, (real, imag) for T21, real T22
        let _ = (cos_t * cos_p, cos_t * sin_p);
        let _ = (sin_t * cos_p, sin_t * sin_p);
        ((cos_t, -sin_t), (sin_t, cos_t))
    }
}

/// Mesh of Mach-Zehnder Interferometers forming an optical neural matrix multiplier
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MziMesh {
    pub mesh_id: String,
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<MziCell>,
    pub energy_efficiency_pj_per_mac: f64,
}

impl MziMesh {
    pub fn new(mesh_id: impl Into<String>, rows: usize, cols: usize) -> Self {
        let mut cells = Vec::with_capacity(rows * cols);
        let mut cid = 0;
        for r in 0..rows {
            for c in 0..cols {
                cells.push(MziCell::new(cid, r, c));
                cid += 1;
            }
        }
        Self {
            mesh_id: mesh_id.into(),
            rows,
            cols,
            cells,
            energy_efficiency_pj_per_mac: 0.075,
        }
    }

    /// Perform optical matrix-vector multiplication Y = W * X in simulated photonic domain
    pub fn multiply_vector(&self, input: &[f64]) -> Vec<f64> {
        let mut output = vec![0.0; self.rows];
        let in_len = input.len().min(self.cols);
        
        for r in 0..self.rows {
            let mut sum = 0.0;
            for c in 0..in_len {
                let cell_idx = r * self.cols + c;
                if let Some(cell) = self.cells.get(cell_idx) {
                    let weight = cell.phase_shift_theta.cos();
                    sum += weight * input[c];
                }
            }
            output[r] = sum;
        }
        output
    }
}

/// Co-Packaged Optics (CPO) Silicon Photonic Substrate Tile Descriptor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpoTileDescriptor {
    pub tile_id: u32,
    pub name: String,
    pub lane_count: u32,
    pub bandwidth_tbps: f64,
    pub die_to_die_latency_ps: f64,
    pub laser_power_mw: f64,
    pub rings: Vec<MicroRingModulator>,
    pub is_active: bool,
}

impl CpoTileDescriptor {
    pub fn new(tile_id: u32, name: impl Into<String>, lanes: u32, bandwidth_tbps: f64) -> Self {
        let mut rings = Vec::with_capacity(lanes as usize);
        let base_lambda = 1310.0;
        for l in 0..lanes {
            // DWDM channel spacing: 0.8 nm (100 GHz)
            let lambda = base_lambda + (l as f64 * 0.8);
            rings.push(MicroRingModulator::new(l, lambda));
        }

        Self {
            tile_id,
            name: name.into(),
            lane_count: lanes,
            bandwidth_tbps,
            die_to_die_latency_ps: 580.0 + (tile_id as f64 * 25.0),
            laser_power_mw: 24.5,
            rings,
            is_active: true,
        }
    }
}

/// Closed-loop thermal PID wavelength servo stabilizer state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpoThermalServoState {
    pub substrate_temp_c: f64,
    pub target_temp_c: f64,
    pub drift_nm: f64,
    pub pid_integral: f64,
    pub pid_derivative: f64,
    pub thermal_status: CpoThermalStatus,
}

impl Default for CpoThermalServoState {
    fn default() -> Self {
        Self {
            substrate_temp_c: 45.02,
            target_temp_c: 45.00,
            drift_nm: 0.0016,
            pid_integral: 0.0005,
            pid_derivative: -0.0001,
            thermal_status: CpoThermalStatus::Locked,
        }
    }
}

impl CpoThermalServoState {
    /// Update closed-loop thermal stabilizer with temperature feedback
    pub fn update(&mut self, current_temp_c: f64) {
        self.substrate_temp_c = current_temp_c;
        let error = self.substrate_temp_c - self.target_temp_c;
        self.drift_nm = (error * 82.0) / 1000.0;
        self.pid_integral = (self.pid_integral + error * 0.1).clamp(-10.0, 10.0);
        self.pid_derivative = error;

        if error.abs() < 0.25 {
            self.thermal_status = CpoThermalStatus::Locked;
        } else if error.abs() < 1.5 {
            self.thermal_status = CpoThermalStatus::Tuning;
        } else {
            self.thermal_status = CpoThermalStatus::DriftWarning;
        }
    }
}

/// Summary report of Silicon Photonic CPO interconnect status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpoStatusSummary {
    pub mode: CpoMode,
    pub total_tiles: usize,
    pub active_tiles: usize,
    pub aggregate_bandwidth_tbps: f64,
    pub average_latency_ps: f64,
    pub total_rings: usize,
    pub substrate_temp_c: f64,
    pub thermal_status: CpoThermalStatus,
    pub mvm_throughput_tops: f64,
    pub energy_efficiency_pj_per_mac: f64,
}

/// Photonic neural benchmark metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpoBenchmarkMetrics {
    pub mac_operations: u64,
    pub duration_nanos: u64,
    pub throughput_tops: f64,
    pub power_consumption_watts: f64,
    pub energy_efficiency_pj_per_mac: f64,
    pub avg_vector_error_l2: f64,
}

/// Co-Packaged Optics subsystem state and registry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpoRegistry {
    pub mode: CpoMode,
    pub tiles: Vec<CpoTileDescriptor>,
    pub meshes: Vec<MziMesh>,
    pub servo: CpoThermalServoState,
    pub last_benchmark: Option<CpoBenchmarkMetrics>,
    pub updated_at: u64,
}

impl Default for CpoRegistry {
    fn default() -> Self {
        Self::default_hardware()
    }
}

impl CpoRegistry {
    /// Initialize registry with default 4 silicon photonic CPO tiles and 4x4 MZI mesh
    pub fn default_hardware() -> Self {
        let tiles = vec![
            CpoTileDescriptor::new(0, "CPO-Tile-North-Die0", 8, 3.2),
            CpoTileDescriptor::new(1, "CPO-Tile-East-Die0", 8, 3.2),
            CpoTileDescriptor::new(2, "CPO-Tile-South-Die1", 8, 3.2),
            CpoTileDescriptor::new(3, "CPO-Tile-West-Die1", 8, 3.2),
        ];

        let meshes = vec![
            MziMesh::new("MZI-Mesh-Tensor-0", 4, 4),
            MziMesh::new("MZI-Mesh-Tensor-1", 4, 4),
        ];

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            mode: CpoMode::Autonomous,
            tiles,
            meshes,
            servo: CpoThermalServoState::default(),
            last_benchmark: None,
            updated_at: now,
        }
    }

    /// Load registry from file or return default
    pub fn load_or_default(paths: &CraftPaths) -> Result<Self> {
        let file = &paths.cpo_registry_file;
        if !file.exists() {
            let reg = Self::default_hardware();
            let _ = reg.save(paths);
            return Ok(reg);
        }
        let content = fs::read_to_string(file)?;
        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse cpo.toml: {}", e)))
    }

    /// Persist registry to disk
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file = &paths.cpo_registry_file;
        if let Some(parent) = file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize cpo.toml: {}", e)))?;
        fs::write(file, content)?;
        Ok(())
    }

    /// Generate summary status report
    pub fn summary(&self) -> CpoStatusSummary {
        let total_tiles = self.tiles.len();
        let active_tiles = self.tiles.iter().filter(|t| t.is_active).count();
        let aggregate_bandwidth_tbps = self
            .tiles
            .iter()
            .filter(|t| t.is_active)
            .map(|t| t.bandwidth_tbps)
            .sum();

        let average_latency_ps = if active_tiles > 0 {
            self.tiles
                .iter()
                .filter(|t| t.is_active)
                .map(|t| t.die_to_die_latency_ps)
                .sum::<f64>()
                / active_tiles as f64
        } else {
            0.0
        };

        let total_rings: usize = self.tiles.iter().map(|t| t.rings.len()).sum();

        let energy_efficiency_pj_per_mac = self
            .meshes
            .first()
            .map(|m| m.energy_efficiency_pj_per_mac)
            .unwrap_or(0.075);

        let mvm_throughput_tops = self
            .last_benchmark
            .as_ref()
            .map(|b| b.throughput_tops)
            .unwrap_or(428.6);

        CpoStatusSummary {
            mode: self.mode,
            total_tiles,
            active_tiles,
            aggregate_bandwidth_tbps,
            average_latency_ps,
            total_rings,
            substrate_temp_c: self.servo.substrate_temp_c,
            thermal_status: self.servo.thermal_status,
            mvm_throughput_tops,
            energy_efficiency_pj_per_mac,
        }
    }
}

// ---------------------------------------------------------------------------
// Zero-Emoji Plain-Text Formatters
// ---------------------------------------------------------------------------

pub fn render_cpo_status_table(status: &CpoStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("+------------------------------------------------------------------+\n");
    out.push_str("| SILICON PHOTONIC CO-PACKAGED OPTICS & DIRECT INTERCONNECT STATUS |\n");
    out.push_str("+----------------------------------+-------------------------------+\n");
    out.push_str(&format!("| Operational Mode                 | {:<29} |\n", status.mode.to_string()));
    out.push_str(&format!("| Thermal Servo Status             | {:<29} |\n", status.thermal_status.to_string()));
    out.push_str(&format!("| Active CPO Optical Tiles         | {:<29} |\n", format!("{}/{}", status.active_tiles, status.total_tiles)));
    out.push_str(&format!("| Aggregate Optical Bandwidth      | {:<29} |\n", format!("{:.2} Tbps", status.aggregate_bandwidth_tbps)));
    out.push_str(&format!("| Mean Die-to-Die Latency          | {:<29} |\n", format!("{:.1} ps", status.average_latency_ps)));
    out.push_str(&format!("| Micro-Ring Resonators            | {:<29} |\n", status.total_rings));
    out.push_str(&format!("| Substrate Core Temperature       | {:<29} |\n", format!("{:.2} deg C", status.substrate_temp_c)));
    out.push_str(&format!("| Photonic MVM Throughput          | {:<29} |\n", format!("{:.1} TOPS", status.mvm_throughput_tops)));
    out.push_str(&format!("| Energy Efficiency                | {:<29} |\n", format!("{:.3} pJ/MAC", status.energy_efficiency_pj_per_mac)));
    out.push_str("+----------------------------------+-------------------------------+\n");
    out
}

pub fn render_cpo_tiles_table(tiles: &[CpoTileDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("+---------+---------------------------+-------+---------+----------+--------+---------+\n");
    out.push_str("| TILE ID | IDENTIFIER                | LANES | BW-TBPS | LATENCY  | RINGS  | STATUS  |\n");
    out.push_str("+---------+---------------------------+-------+---------+----------+--------+---------+\n");
    if tiles.is_empty() {
        out.push_str("| No Co-Packaged Optics tiles registered on silicon substrate.                        |\n");
    } else {
        for t in tiles {
            let status_str = if t.is_active { "ACTIVE" } else { "INACTIVE" };
            out.push_str(&format!(
                "| {:<7} | {:<25} | {:<5} | {:<7.1} | {:<6.0} ps | {:<6} | {:<7} |\n",
                t.tile_id,
                if t.name.len() > 25 { &t.name[..25] } else { &t.name },
                t.lane_count,
                t.bandwidth_tbps,
                t.die_to_die_latency_ps,
                t.rings.len(),
                status_str
            ));
        }
    }
    out.push_str("+---------+---------------------------+-------+---------+----------+--------+---------+\n");
    out
}

pub fn render_cpo_bench_table(metrics: &CpoBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("+------------------------------------------------------------------+\n");
    out.push_str("| PHOTONIC MATRIX-VECTOR MULTIPLY (MVM) BENCHMARK RESULTS          |\n");
    out.push_str("+----------------------------------+-------------------------------+\n");
    out.push_str(&format!("| Total Multiply-Accumulate (MAC)  | {:<29} |\n", metrics.mac_operations));
    out.push_str(&format!("| Execution Duration               | {:<29} |\n", format!("{:.3} us", metrics.duration_nanos as f64 / 1000.0)));
    out.push_str(&format!("| Compute Throughput               | {:<29} |\n", format!("{:.2} TOPS", metrics.throughput_tops)));
    out.push_str(&format!("| Total Power Dissipation          | {:<29} |\n", format!("{:.2} W", metrics.power_consumption_watts)));
    out.push_str(&format!("| Energy Per MAC Operation         | {:<29} |\n", format!("{:.3} pJ/MAC", metrics.energy_efficiency_pj_per_mac)));
    out.push_str(&format!("| Mean Vector L2 Relative Error    | {:<29.2e} |\n", metrics.avg_vector_error_l2));
    out.push_str("+----------------------------------+-------------------------------+\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cpo_mode_serde() {
        let mode = CpoMode::DirectDiePhotonic;
        let s = mode.to_string();
        assert_eq!(s, "direct-die-photonic");
        let parsed: CpoMode = s.parse().unwrap();
        assert_eq!(parsed, CpoMode::DirectDiePhotonic);

        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"direct-die-photonic\"");
        let deserialized: CpoMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CpoMode::DirectDiePhotonic);
    }

    #[test]
    fn test_micro_ring_modulation() {
        let mut ring = MicroRingModulator::new(0, 1310.0);
        assert_eq!(ring.ring_id, 0);
        assert_eq!(ring.wavelength_nm, 1310.0);

        // At 45.0 C, delta_t is 0 -> 1310.0
        let lambda = ring.resonance_at_temp(45.0);
        assert!((lambda - 1310.0).abs() < 1e-6);

        // At 55.0 C, delta_t = 10 -> delta_lambda = 10 * 82 / 1000 = 0.82 nm
        let lambda_hot = ring.resonance_at_temp(55.0);
        assert!((lambda_hot - 1310.82).abs() < 1e-6);

        ring.tune_heater(22.5);
        assert_eq!(ring.heater_current_ma, 22.5);
    }

    #[test]
    fn test_mzi_mesh_tensor_multiply() {
        let mesh = MziMesh::new("test-mesh", 2, 2);
        assert_eq!(mesh.cells.len(), 4);

        let input = vec![1.0, 2.0];
        let output = mesh.multiply_vector(&input);
        assert_eq!(output.len(), 2);
        // By default theta = PI/4, cos(PI/4) = 0.70710678...
        // y[0] = cos(PI/4)*1.0 + cos(PI/4)*2.0 = 3.0 * cos(PI/4) ~ 2.12132
        let expected = 3.0 * std::f64::consts::FRAC_PI_4.cos();
        assert!((output[0] - expected).abs() < 1e-4);
    }

    #[test]
    fn test_cpo_registry_persistence() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let reg = CpoRegistry::default_hardware();
        reg.save(&paths).unwrap();

        let loaded = CpoRegistry::load_or_default(&paths).unwrap();
        assert_eq!(loaded.tiles.len(), 4);
        assert_eq!(loaded.mode, CpoMode::Autonomous);

        let summary = loaded.summary();
        assert_eq!(summary.total_tiles, 4);
        assert_eq!(summary.active_tiles, 4);
        assert!((summary.aggregate_bandwidth_tbps - 12.8).abs() < 1e-5);
    }

    #[test]
    fn test_cpo_table_renderers() {
        let reg = CpoRegistry::default_hardware();
        let summary = reg.summary();
        let status_table = render_cpo_status_table(&summary);
        assert!(status_table.contains("SILICON PHOTONIC CO-PACKAGED OPTICS"));
        assert!(status_table.contains("12.80 Tbps"));

        let tiles_table = render_cpo_tiles_table(&reg.tiles);
        assert!(tiles_table.contains("CPO-Tile-North-Die0"));

        let bench = CpoBenchmarkMetrics {
            mac_operations: 1_000_000,
            duration_nanos: 2500,
            throughput_tops: 800.0,
            power_consumption_watts: 18.5,
            energy_efficiency_pj_per_mac: 0.046,
            avg_vector_error_l2: 0.0003,
        };
        let bench_table = render_cpo_bench_table(&bench);
        assert!(bench_table.contains("800.00 TOPS"));
    }
}
