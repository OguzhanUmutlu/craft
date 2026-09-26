// crates/daemon/src/cryo_service.rs
//
// Phase 54: Autonomous Zero-Point Vacuum Energy Harvesting, Thermoelectric Cluster Power Balancing & Sub-Kelvin Cryogenic Cooling Supervisor Service.
// Pure-Rust Casimir cavity MEMS, thermoelectric Seebeck routing, and multi-stage sub-Kelvin dilution refrigeration.
// Strictly zero emojis.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use craft_core::cryo::{
    CasimirCavityMems, CryoBenchmarkMetrics, CryoMode, CryoRegistry, CryoStatusSummary,
    CryoZoneDescriptor, ThermoelectricModule,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_net::cryo::{
    benchmark_cryogenic_cooling, CasimirVacuumHarvester, CryogenicThermalBalancer,
    ThermoelectricPowerRouter,
};

static INSTANCE: OnceLock<Arc<CryoService>> = OnceLock::new();

pub struct CryoService {
    paths: CraftPaths,
    registry: Mutex<CryoRegistry>,
    harvester: Mutex<CasimirVacuumHarvester>,
    router: Mutex<ThermoelectricPowerRouter>,
    balancer: Mutex<CryogenicThermalBalancer>,
    total_zpe_harvested_uj: Arc<AtomicU64>,
    total_teg_routed_joules: Arc<AtomicU64>,
    quenches_averted_count: Arc<AtomicU64>,
}

impl CryoService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = CryoRegistry::load_or_default(&paths);
        let harvester = CasimirVacuumHarvester::new(registry.cavities.clone());
        let router = ThermoelectricPowerRouter::new(registry.thermoelectrics.clone());
        let balancer = CryogenicThermalBalancer::new(registry.target_mixing_chamber_mk);
        let quenches = registry.quenches_averted;

        Self {
            paths,
            registry: Mutex::new(registry),
            harvester: Mutex::new(harvester),
            router: Mutex::new(router),
            balancer: Mutex::new(balancer),
            total_zpe_harvested_uj: Arc::new(AtomicU64::new(0)),
            total_teg_routed_joules: Arc::new(AtomicU64::new(0)),
            quenches_averted_count: Arc::new(AtomicU64::new(quenches)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<CryoStatusSummary> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut summary = reg.generate_summary();
        summary.quenches_averted_count = self.quenches_averted_count.load(Ordering::Relaxed);
        Ok(summary)
    }

    pub fn set_mode(&self, new_mode: CryoMode, _server: Option<&str>) -> Result<bool> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.mode = new_mode;
        reg.save(&self.paths)?;
        Ok(true)
    }

    pub fn balance_zones(
        &self,
        target_temp_mk: Option<f64>,
        _server: Option<&str>,
    ) -> Result<(usize, f64, u32)> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut balancer = self.balancer.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        if let Some(target) = target_temp_mk {
            balancer.target_temp_mk = target;
            reg.target_mixing_chamber_mk = target;
        }

        let (mean_temp, averted) = balancer.balance_zones(&mut reg.zones);
        self.quenches_averted_count.store(averted as u64, Ordering::Relaxed);
        reg.quenches_averted = averted as u64;
        let count = reg.zones.len();
        reg.save(&self.paths)?;

        Ok((count, mean_temp, averted))
    }

    pub fn harvest_zero_point(
        &self,
        _cavity_id: Option<String>,
        _server: Option<&str>,
    ) -> Result<(f64, f64, f64)> {
        let mut harvester = self.harvester.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut router = self.router.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let (harvested_uw, total_uj) = harvester.harvest_cycle();
        let teg_w = router.route_power();

        self.total_zpe_harvested_uj.store(total_uj as u64, Ordering::Relaxed);
        self.total_teg_routed_joules.fetch_add(teg_w as u64, Ordering::Relaxed);

        reg.cavities = harvester.cavities().to_vec();
        reg.thermoelectrics = router.modules().to_vec();
        reg.save(&self.paths)?;

        let total_joules = (total_uj * 1e-6) + (self.total_teg_routed_joules.load(Ordering::Relaxed) as f64);
        Ok((harvested_uw, teg_w, total_joules))
    }

    pub fn list_zones(&self, _server: Option<&str>) -> Result<Vec<CryoZoneDescriptor>> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(reg.zones.clone())
    }

    pub fn list_cavities(&self) -> Result<Vec<CasimirCavityMems>> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(reg.cavities.clone())
    }

    pub fn list_thermoelectrics(&self) -> Result<Vec<ThermoelectricModule>> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(reg.thermoelectrics.clone())
    }

    pub fn run_bench(
        &self,
        zones: Option<usize>,
        _duration_sec: Option<u64>,
        _server: Option<&str>,
    ) -> Result<CryoBenchmarkMetrics> {
        let count = zones.unwrap_or(4);
        let metrics = benchmark_cryogenic_cooling(count);

        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.quenches_averted += 1;
        self.quenches_averted_count.fetch_add(1, Ordering::Relaxed);
        reg.save(&self.paths)?;

        Ok(metrics)
    }

    pub fn reset_metrics(&self, _server: Option<&str>) -> Result<bool> {
        self.total_zpe_harvested_uj.store(0, Ordering::Relaxed);
        self.total_teg_routed_joules.store(0, Ordering::Relaxed);
        self.quenches_averted_count.store(0, Ordering::Relaxed);

        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.quenches_averted = 0;
        reg.zones = CryoRegistry::default_zones();
        reg.cavities = CryoRegistry::default_cavities();
        reg.thermoelectrics = CryoRegistry::default_thermoelectrics();
        reg.save(&self.paths)?;

        Ok(true)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let status = match self.get_status(None) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let mut out = String::new();
        out.push_str("# HELP craft_cryo_total_zones Total cryogenic thermal zones in cluster\n");
        out.push_str("# TYPE craft_cryo_total_zones gauge\n");
        out.push_str(&format!("craft_cryo_total_zones {}\n", status.total_zones));

        out.push_str("# HELP craft_cryo_superconducting_nominal_zones Zones operating below 20 mK superconducting threshold\n");
        out.push_str("# TYPE craft_cryo_superconducting_nominal_zones gauge\n");
        out.push_str(&format!("craft_cryo_superconducting_nominal_zones {}\n", status.superconducting_nominal_zones));

        out.push_str("# HELP craft_cryo_mean_mixing_chamber_mk Mean dilution mixing chamber temperature in milli-Kelvin\n");
        out.push_str("# TYPE craft_cryo_mean_mixing_chamber_mk gauge\n");
        out.push_str(&format!("craft_cryo_mean_mixing_chamber_mk {:.2}\n", status.mean_mixing_chamber_mk));

        out.push_str("# HELP craft_cryo_lowest_mixing_chamber_mk Lowest dilution mixing chamber temperature in milli-Kelvin\n");
        out.push_str("# TYPE craft_cryo_lowest_mixing_chamber_mk gauge\n");
        out.push_str(&format!("craft_cryo_lowest_mixing_chamber_mk {:.2}\n", status.lowest_mixing_chamber_mk));

        out.push_str("# HELP craft_cryo_harvested_zero_point_uw Active Casimir cavity zero-point power harvested in micro-Watts\n");
        out.push_str("# TYPE craft_cryo_harvested_zero_point_uw gauge\n");
        out.push_str(&format!("craft_cryo_harvested_zero_point_uw {:.2}\n", status.total_harvested_zero_point_uw));

        out.push_str("# HELP craft_cryo_thermoelectric_power_w Active thermoelectric Seebeck waste-heat power recovered in Watts\n");
        out.push_str("# TYPE craft_cryo_thermoelectric_power_w gauge\n");
        out.push_str(&format!("craft_cryo_thermoelectric_power_w {:.2}\n", status.total_thermoelectric_power_w));

        out.push_str("# HELP craft_cryo_total_cooling_power_mw Total dilution refrigerator cooling power in milli-Watts\n");
        out.push_str("# TYPE craft_cryo_total_cooling_power_mw gauge\n");
        out.push_str(&format!("craft_cryo_total_cooling_power_mw {:.2}\n", status.total_cooling_power_mw));

        out.push_str("# HELP craft_cryo_quenches_averted_total Cumulative superconducting thermal quenches averted\n");
        out.push_str("# TYPE craft_cryo_quenches_averted_total counter\n");
        out.push_str(&format!("craft_cryo_quenches_averted_total {}\n", status.quenches_averted_count));

        out
    }
}
