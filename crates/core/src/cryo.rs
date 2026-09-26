// crates/core/src/cryo.rs
//
// Phase 54: Autonomous Zero-Point Vacuum Energy Harvesting, Thermoelectric Cluster Power Balancing & Sub-Kelvin Cryogenic Cooling

use std::fmt;
use std::fs;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

/// Operational mode for Cryogenic Cooling and Zero-Point Power Balancing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CryoMode {
    Autonomous,
    SuperconductingMaxQ,
    WasteHeatThermoelectric,
    ZeroPointHarvesting,
    SubKelvinCryoStabilized,
    EcoDilution,
}

impl Default for CryoMode {
    fn default() -> Self {
        CryoMode::Autonomous
    }
}

impl fmt::Display for CryoMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryoMode::Autonomous => write!(f, "autonomous"),
            CryoMode::SuperconductingMaxQ => write!(f, "superconducting-max-q"),
            CryoMode::WasteHeatThermoelectric => write!(f, "waste-heat-thermoelectric"),
            CryoMode::ZeroPointHarvesting => write!(f, "zero-point-harvesting"),
            CryoMode::SubKelvinCryoStabilized => write!(f, "sub-kelvin-cryo-stabilized"),
            CryoMode::EcoDilution => write!(f, "eco-dilution"),
        }
    }
}

impl FromStr for CryoMode {
    type Err = CraftError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "autonomous" | "auto" => Ok(CryoMode::Autonomous),
            "superconducting-max-q" | "superconducting_max_q" | "superconducting" | "max-q" => {
                Ok(CryoMode::SuperconductingMaxQ)
            }
            "waste-heat-thermoelectric" | "waste_heat_thermoelectric" | "thermoelectric" | "teg" => {
                Ok(CryoMode::WasteHeatThermoelectric)
            }
            "zero-point-harvesting" | "zero_point_harvesting" | "zero-point" | "casimir" | "vacuum" => {
                Ok(CryoMode::ZeroPointHarvesting)
            }
            "sub-kelvin-cryo-stabilized" | "sub_kelvin_cryo_stabilized" | "sub-kelvin" | "cryo" | "stabilized" => {
                Ok(CryoMode::SubKelvinCryoStabilized)
            }
            "eco-dilution" | "eco_dilution" | "eco" | "dilution" => {
                Ok(CryoMode::EcoDilution)
            }
            other => Err(CraftError::Config(format!(
                "Unknown cryo mode '{}'. Valid modes: autonomous, superconducting-max-q, waste-heat-thermoelectric, zero-point-harvesting, sub-kelvin-cryo-stabilized, eco-dilution",
                other
            ))),
        }
    }
}

/// Temperature status of a cryogenic thermal zone
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CryoTemperatureStatus {
    SuperconductingNominal,
    ThermalFluctuation,
    CriticalQuenchWarning,
    Regenerating,
}

impl Default for CryoTemperatureStatus {
    fn default() -> Self {
        CryoTemperatureStatus::SuperconductingNominal
    }
}

impl fmt::Display for CryoTemperatureStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryoTemperatureStatus::SuperconductingNominal => write!(f, "superconducting-nominal"),
            CryoTemperatureStatus::ThermalFluctuation => write!(f, "thermal-fluctuation"),
            CryoTemperatureStatus::CriticalQuenchWarning => write!(f, "critical-quench-warning"),
            CryoTemperatureStatus::Regenerating => write!(f, "regenerating"),
        }
    }
}

impl CryoTemperatureStatus {
    pub fn from_mixing_chamber_mk(temp_mk: f64) -> Self {
        if temp_mk < 20.0 {
            CryoTemperatureStatus::SuperconductingNominal
        } else if temp_mk < 100.0 {
            CryoTemperatureStatus::ThermalFluctuation
        } else {
            CryoTemperatureStatus::CriticalQuenchWarning
        }
    }
}

/// Micro-electromechanical Casimir cavity for vacuum zero-point energy extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasimirCavityMems {
    pub cavity_id: String,
    pub plate_separation_nm: f64,
    pub plate_area_um2: f64,
    pub resonance_frequency_ghz: f64,
    pub casimir_force_nn: f64,
    pub harvested_power_uw: f64,
    pub q_factor: f64,
}

impl CasimirCavityMems {
    pub fn new(cavity_id: impl Into<String>, separation_nm: f64, area_um2: f64, freq_ghz: f64) -> Self {
        let id = cavity_id.into();
        let force_nn = Self::compute_casimir_force(separation_nm, area_um2);
        let power_uw = Self::compute_harvested_power_uw(separation_nm, area_um2, freq_ghz);
        Self {
            cavity_id: id,
            plate_separation_nm: separation_nm,
            plate_area_um2: area_um2,
            resonance_frequency_ghz: freq_ghz,
            casimir_force_nn: force_nn,
            harvested_power_uw: power_uw,
            q_factor: 150_000.0,
        }
    }

    /// Computes Casimir attraction force in nano-Newtons:
    /// F_c = (pi^2 * hbar * c) / (240 * d^4) * Area
    pub fn compute_casimir_force(separation_nm: f64, area_um2: f64) -> f64 {
        let d_m = (separation_nm * 1e-9).max(1e-10);
        let area_m2 = area_um2 * 1e-12;
        // pi^2 * hbar * c / 240 ~= 1.3006e-27 N * m^2
        let const_factor = 1.300624e-27;
        let force_n = (const_factor / d_m.powi(4)) * area_m2;
        force_n * 1e9 // Convert N to nN
    }

    /// Estimates harvested electrical power in micro-Watts from modulated Casimir cavity
    pub fn compute_harvested_power_uw(separation_nm: f64, area_um2: f64, freq_ghz: f64) -> f64 {
        let force_nn = Self::compute_casimir_force(separation_nm, area_um2);
        // Energy extracted per cycle = eta * F * delta_d, power = dE * freq
        let delta_d_nm = (separation_nm * 0.15).max(1.0);
        let energy_per_cycle_femtjoules = force_nn * delta_d_nm * 0.42; // nN * nm = aJ, scaled
        let freq_hz = freq_ghz * 1e9;
        let power_watts = (energy_per_cycle_femtjoules * 1e-15) * freq_hz * 0.05; // 5% harvesting efficiency
        (power_watts * 1e6).max(0.01) // In micro-watts
    }
}

/// Thermoelectric generator module for cluster waste-heat harvesting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermoelectricModule {
    pub module_id: String,
    pub seebeck_coeff_uv_k: f64,
    pub hot_side_temp_k: f64,
    pub cold_side_temp_k: f64,
    pub figure_of_merit_zt: f64,
    pub internal_resistance_ohms: f64,
    pub output_power_w: f64,
}

impl ThermoelectricModule {
    pub fn new(module_id: impl Into<String>, seebeck_uv_k: f64, t_hot: f64, t_cold: f64, zt: f64) -> Self {
        let r_int = 1.25;
        let p_w = Self::calculate_power_w(seebeck_uv_k, t_hot, t_cold, r_int);
        Self {
            module_id: module_id.into(),
            seebeck_coeff_uv_k: seebeck_uv_k,
            hot_side_temp_k: t_hot,
            cold_side_temp_k: t_cold,
            figure_of_merit_zt: zt,
            internal_resistance_ohms: r_int,
            output_power_w: p_w,
        }
    }

    /// Calculates matched-load thermoelectric output power:
    /// P_max = (S * delta_T)^2 / (4 * R_int)
    pub fn calculate_power_w(seebeck_uv_k: f64, t_hot_k: f64, t_cold_k: f64, r_int: f64) -> f64 {
        let delta_t = (t_hot_k - t_cold_k).max(0.0);
        let s_volts_per_k = seebeck_uv_k * 1e-6;
        let voc = s_volts_per_k * delta_t;
        let r_safe = r_int.max(0.05);
        (voc * voc) / (4.0 * r_safe)
    }
}

/// Multi-stage dilution refrigerator temperature and pressure telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DilutionStageTelemetry {
    pub ambient_300k: f64,
    pub radiation_shield_50k: f64,
    pub pulse_tube_4k: f64,
    pub still_stage_800mk: f64,
    pub cold_plate_100mk: f64,
    pub mixing_chamber_mk: f64,
    pub he3_he4_flow_rate_mmol_s: f64,
    pub compressor_pressure_bar: f64,
}

impl Default for DilutionStageTelemetry {
    fn default() -> Self {
        Self {
            ambient_300k: 295.15,
            radiation_shield_50k: 48.5,
            pulse_tube_4k: 4.15,
            still_stage_800mk: 0.78,
            cold_plate_100mk: 95.0, // in mK
            mixing_chamber_mk: 14.5, // in mK
            he3_he4_flow_rate_mmol_s: 0.82,
            compressor_pressure_bar: 2.35,
        }
    }
}

/// Descriptor for a cryogenic thermal zone within the server cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoZoneDescriptor {
    pub zone_id: String,
    pub rack_id: String,
    pub stage_telemetry: DilutionStageTelemetry,
    pub thermal_status: CryoTemperatureStatus,
    pub active_workload_pct: f64,
    pub cooling_power_mw: f64,
    pub harvested_zero_point_uw: f64,
    pub recovered_thermoelectric_w: f64,
    pub quench_margin_mk: f64,
}

impl CryoZoneDescriptor {
    pub fn new(zone_id: impl Into<String>, rack_id: impl Into<String>, mix_temp_mk: f64) -> Self {
        let mut telemetry = DilutionStageTelemetry::default();
        telemetry.mixing_chamber_mk = mix_temp_mk;
        let status = CryoTemperatureStatus::from_mixing_chamber_mk(mix_temp_mk);
        let margin = (100.0 - mix_temp_mk).max(0.0);
        Self {
            zone_id: zone_id.into(),
            rack_id: rack_id.into(),
            stage_telemetry: telemetry,
            thermal_status: status,
            active_workload_pct: 42.0,
            cooling_power_mw: 24.5,
            harvested_zero_point_uw: 112.5,
            recovered_thermoelectric_w: 38.4,
            quench_margin_mk: margin,
        }
    }
}

/// Aggregated cryogenic status summary for daemon, remote and CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoStatusSummary {
    pub mode: CryoMode,
    pub total_zones: usize,
    pub superconducting_nominal_zones: usize,
    pub mean_mixing_chamber_mk: f64,
    pub lowest_mixing_chamber_mk: f64,
    pub total_harvested_zero_point_uw: f64,
    pub total_thermoelectric_power_w: f64,
    pub total_cooling_power_mw: f64,
    pub quenches_averted_count: u64,
    pub uptime_seconds: u64,
}

impl Default for CryoStatusSummary {
    fn default() -> Self {
        Self {
            mode: CryoMode::Autonomous,
            total_zones: 4,
            superconducting_nominal_zones: 4,
            mean_mixing_chamber_mk: 14.5,
            lowest_mixing_chamber_mk: 11.8,
            total_harvested_zero_point_uw: 450.0,
            total_thermoelectric_power_w: 153.6,
            total_cooling_power_mw: 98.0,
            quenches_averted_count: 0,
            uptime_seconds: 0,
        }
    }
}

/// Synthetic cryogenic cooling and power balancing benchmark metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoBenchmarkMetrics {
    pub zones_evaluated: usize,
    pub stabilization_latency_us: f64,
    pub harvested_zero_point_power_uw: f64,
    pub thermoelectric_efficiency_pct: f64,
    pub simulated_quench_recovery_ms: f64,
    pub thermal_uniformity_score: f64,
    pub cop_cooling_efficiency: f64,
    pub passed: bool,
}

impl Default for CryoBenchmarkMetrics {
    fn default() -> Self {
        Self {
            zones_evaluated: 4,
            stabilization_latency_us: 185.0,
            harvested_zero_point_power_uw: 450.0,
            thermoelectric_efficiency_pct: 12.8,
            simulated_quench_recovery_ms: 42.5,
            thermal_uniformity_score: 0.985,
            cop_cooling_efficiency: 0.082,
            passed: true,
        }
    }
}

/// Persistent registry holding cryogenic cluster configurations and zones
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoRegistry {
    pub mode: CryoMode,
    pub target_mixing_chamber_mk: f64,
    pub zones: Vec<CryoZoneDescriptor>,
    pub cavities: Vec<CasimirCavityMems>,
    pub thermoelectrics: Vec<ThermoelectricModule>,
    pub quenches_averted: u64,
    pub last_updated_epoch_secs: u64,
}

impl Default for CryoRegistry {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            mode: CryoMode::Autonomous,
            target_mixing_chamber_mk: 15.0,
            zones: Self::default_zones(),
            cavities: Self::default_cavities(),
            thermoelectrics: Self::default_thermoelectrics(),
            quenches_averted: 0,
            last_updated_epoch_secs: now,
        }
    }
}

impl CryoRegistry {
    pub fn default_zones() -> Vec<CryoZoneDescriptor> {
        vec![
            CryoZoneDescriptor::new("cryo-zone-0", "rack-subkelvin-alpha", 12.4),
            CryoZoneDescriptor::new("cryo-zone-1", "rack-subkelvin-alpha", 13.8),
            CryoZoneDescriptor::new("cryo-zone-2", "rack-subkelvin-beta", 15.2),
            CryoZoneDescriptor::new("cryo-zone-3", "rack-subkelvin-beta", 16.5),
        ]
    }

    pub fn default_cavities() -> Vec<CasimirCavityMems> {
        vec![
            CasimirCavityMems::new("casimir-mems-0", 25.0, 500.0, 14.2),
            CasimirCavityMems::new("casimir-mems-1", 20.0, 600.0, 18.5),
            CasimirCavityMems::new("casimir-mems-2", 30.0, 450.0, 12.0),
            CasimirCavityMems::new("casimir-mems-3", 18.0, 750.0, 22.0),
        ]
    }

    pub fn default_thermoelectrics() -> Vec<ThermoelectricModule> {
        vec![
            ThermoelectricModule::new("teg-array-0", 220.0, 295.0, 48.5, 2.4),
            ThermoelectricModule::new("teg-array-1", 240.0, 48.5, 4.2, 2.1),
            ThermoelectricModule::new("teg-array-2", 210.0, 295.0, 77.0, 2.3),
            ThermoelectricModule::new("teg-array-3", 260.0, 77.0, 4.2, 2.6),
        ]
    }

    pub fn load_or_default(paths: &CraftPaths) -> Self {
        if paths.cryo_registry_file.exists() {
            if let Ok(content) = fs::read_to_string(&paths.cryo_registry_file) {
                if let Ok(reg) = toml::from_str::<CryoRegistry>(&content) {
                    return reg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.cryo_registry_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize cryo registry: {}", e)))?;
        fs::write(&paths.cryo_registry_file, serialized)?;
        Ok(())
    }

    pub fn generate_summary(&self) -> CryoStatusSummary {
        let total = self.zones.len();
        let nominal = self
            .zones
            .iter()
            .filter(|z| z.thermal_status == CryoTemperatureStatus::SuperconductingNominal)
            .count();
        let mean_mk = if total > 0 {
            self.zones.iter().map(|z| z.stage_telemetry.mixing_chamber_mk).sum::<f64>() / (total as f64)
        } else {
            0.0
        };
        let lowest_mk = self
            .zones
            .iter()
            .map(|z| z.stage_telemetry.mixing_chamber_mk)
            .fold(f64::INFINITY, f64::min);
        let lowest_clean = if lowest_mk.is_infinite() { 0.0 } else { lowest_mk };

        let total_zpe_uw: f64 = self.cavities.iter().map(|c| c.harvested_power_uw).sum();
        let total_teg_w: f64 = self.thermoelectrics.iter().map(|t| t.output_power_w).sum();
        let total_cooling_mw: f64 = self.zones.iter().map(|z| z.cooling_power_mw).sum();

        CryoStatusSummary {
            mode: self.mode,
            total_zones: total,
            superconducting_nominal_zones: nominal,
            mean_mixing_chamber_mk: mean_mk,
            lowest_mixing_chamber_mk: lowest_clean,
            total_harvested_zero_point_uw: total_zpe_uw,
            total_thermoelectric_power_w: total_teg_w,
            total_cooling_power_mw: total_cooling_mw,
            quenches_averted_count: self.quenches_averted,
            uptime_seconds: 3600,
        }
    }
}

// ---------------------------------------------------------------------------
// Zero-Emoji Plain-Text Formatters
// ---------------------------------------------------------------------------

pub fn render_cryo_status_table(summary: &CryoStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out.push_str("| Metric                               | Value                                |\n");
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out.push_str(&format!("| Operational Mode                     | {:<36} |\n", summary.mode.to_string()));
    out.push_str(&format!("| Total Cryogenic Zones                | {:<36} |\n", summary.total_zones));
    out.push_str(&format!("| Superconducting Nominal Zones        | {:<36} |\n", summary.superconducting_nominal_zones));
    out.push_str(&format!("| Mean Mixing Chamber Temp             | {:<36} |\n", format!("{:.2} mK", summary.mean_mixing_chamber_mk)));
    out.push_str(&format!("| Lowest Mixing Chamber Temp           | {:<36} |\n", format!("{:.2} mK", summary.lowest_mixing_chamber_mk)));
    out.push_str(&format!("| Harvested Zero-Point Power           | {:<36} |\n", format!("{:.2} uW", summary.total_harvested_zero_point_uw)));
    out.push_str(&format!("| Recovered Thermoelectric Power       | {:<36} |\n", format!("{:.2} W", summary.total_thermoelectric_power_w)));
    out.push_str(&format!("| Total Cooling Capacity               | {:<36} |\n", format!("{:.2} mW", summary.total_cooling_power_mw)));
    out.push_str(&format!("| Superconducting Quenches Averted     | {:<36} |\n", summary.quenches_averted_count));
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out
}

pub fn render_cryo_zones_table(zones: &[CryoZoneDescriptor]) -> String {
    let mut out = String::new();
    out.push_str("+---------------+----------------------+------------+------------+------------+-------------------------+\n");
    out.push_str("| Zone ID       | Rack Placement       | Mix Temp   | Workload   | Cooling    | Status                  |\n");
    out.push_str("+---------------+----------------------+------------+------------+------------+-------------------------+\n");
    for z in zones {
        out.push_str(&format!(
            "| {:<13} | {:<20} | {:<10} | {:<10} | {:<10} | {:<23} |\n",
            z.zone_id,
            z.rack_id,
            format!("{:.1} mK", z.stage_telemetry.mixing_chamber_mk),
            format!("{:.1}%", z.active_workload_pct),
            format!("{:.1} mW", z.cooling_power_mw),
            z.thermal_status.to_string()
        ));
    }
    out.push_str("+---------------+----------------------+------------+------------+------------+-------------------------+\n");
    out
}

pub fn render_cryo_power_table(cavities: &[CasimirCavityMems], tegs: &[ThermoelectricModule]) -> String {
    let mut out = String::new();
    out.push_str("Casimir Cavity Micro-Electromechanical Systems (MEMS):\n");
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n");
    out.push_str("| Cavity ID         | Gap (nm)    | Area (um2)  | Freq (GHz)  | Force (nN)  | Power (uW)  |\n");
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n");
    for c in cavities {
        out.push_str(&format!(
            "| {:<17} | {:<11} | {:<11} | {:<11} | {:<11} | {:<11} |\n",
            c.cavity_id,
            format!("{:.1}", c.plate_separation_nm),
            format!("{:.1}", c.plate_area_um2),
            format!("{:.1}", c.resonance_frequency_ghz),
            format!("{:.2}", c.casimir_force_nn),
            format!("{:.2}", c.harvested_power_uw)
        ));
    }
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n\n");

    out.push_str("Thermoelectric Waste-Heat Recovery Arrays (TEG):\n");
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n");
    out.push_str("| Module ID         | Seebeck uV  | T_hot (K)   | T_cold (K)  | Figure ZT   | Power (W)   |\n");
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n");
    for t in tegs {
        out.push_str(&format!(
            "| {:<17} | {:<11} | {:<11} | {:<11} | {:<11} | {:<11} |\n",
            t.module_id,
            format!("{:.1}", t.seebeck_coeff_uv_k),
            format!("{:.1}", t.hot_side_temp_k),
            format!("{:.1}", t.cold_side_temp_k),
            format!("{:.2}", t.figure_of_merit_zt),
            format!("{:.2}", t.output_power_w)
        ));
    }
    out.push_str("+-------------------+-------------+-------------+-------------+-------------+-------------+\n");
    out
}

pub fn render_cryo_bench_table(metrics: &CryoBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out.push_str("| Benchmark Metric                     | Measured Result                      |\n");
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out.push_str(&format!("| Zones Evaluated                      | {:<36} |\n", metrics.zones_evaluated));
    out.push_str(&format!("| Thermal Stabilization Latency        | {:<36} |\n", format!("{:.1} us", metrics.stabilization_latency_us)));
    out.push_str(&format!("| Harvested Zero-Point Power           | {:<36} |\n", format!("{:.2} uW", metrics.harvested_zero_point_power_uw)));
    out.push_str(&format!("| Thermoelectric Waste-Heat Eff        | {:<36} |\n", format!("{:.2}%", metrics.thermoelectric_efficiency_pct)));
    out.push_str(&format!("| Quench Recovery Latency              | {:<36} |\n", format!("{:.1} ms", metrics.simulated_quench_recovery_ms)));
    out.push_str(&format!("| Thermal Spatial Uniformity           | {:<36} |\n", format!("{:.3}", metrics.thermal_uniformity_score)));
    out.push_str(&format!("| Dilution Refrigerator COP            | {:<36} |\n", format!("{:.4}", metrics.cop_cooling_efficiency)));
    out.push_str(&format!("| Benchmark Verdict                    | {:<36} |\n", if metrics.passed { "[PASSED] 100% Sub-Kelvin Stable" } else { "[FAILED]" }));
    out.push_str("+--------------------------------------+--------------------------------------+\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cryo_mode_parsing_and_display() {
        assert_eq!(CryoMode::from_str("autonomous").unwrap(), CryoMode::Autonomous);
        assert_eq!(CryoMode::from_str("superconducting").unwrap(), CryoMode::SuperconductingMaxQ);
        assert_eq!(CryoMode::from_str("thermoelectric").unwrap(), CryoMode::WasteHeatThermoelectric);
        assert_eq!(CryoMode::from_str("zero-point").unwrap(), CryoMode::ZeroPointHarvesting);
        assert_eq!(CryoMode::from_str("sub-kelvin").unwrap(), CryoMode::SubKelvinCryoStabilized);
        assert_eq!(CryoMode::from_str("eco").unwrap(), CryoMode::EcoDilution);

        assert_eq!(CryoMode::SuperconductingMaxQ.to_string(), "superconducting-max-q");
        assert_eq!(CryoMode::WasteHeatThermoelectric.to_string(), "waste-heat-thermoelectric");
    }

    #[test]
    fn test_casimir_force_and_power() {
        let cavity = CasimirCavityMems::new("test-cavity", 25.0, 500.0, 14.2);
        assert!(cavity.casimir_force_nn > 0.0);
        assert!(cavity.harvested_power_uw > 0.0);
    }

    #[test]
    fn test_thermoelectric_module_power() {
        let module = ThermoelectricModule::new("test-teg", 220.0, 300.0, 50.0, 2.4);
        assert!(module.output_power_w > 0.0);
    }

    #[test]
    fn test_cryo_temperature_status_thresholds() {
        assert_eq!(CryoTemperatureStatus::from_mixing_chamber_mk(12.5), CryoTemperatureStatus::SuperconductingNominal);
        assert_eq!(CryoTemperatureStatus::from_mixing_chamber_mk(45.0), CryoTemperatureStatus::ThermalFluctuation);
        assert_eq!(CryoTemperatureStatus::from_mixing_chamber_mk(120.0), CryoTemperatureStatus::CriticalQuenchWarning);
    }

    #[test]
    fn test_plain_text_table_rendering() {
        let summary = CryoStatusSummary::default();
        let rendered_status = render_cryo_status_table(&summary);
        assert!(rendered_status.contains("Metric"));
        assert!(rendered_status.contains("Superconducting Nominal Zones"));

        let zones = CryoRegistry::default_zones();
        let rendered_zones = render_cryo_zones_table(&zones);
        assert!(rendered_zones.contains("cryo-zone-0"));
        assert!(rendered_zones.contains("superconducting-nominal"));
        assert!(rendered_zones.contains("Rack Placement"));
    }
}
