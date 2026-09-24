use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::str::FromStr;

/// Hardware PMU event types monitored by the dynamic binary instrumentation engine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PmuEventType {
    CpuCycles,
    InstructionsRetired,
    L1DReadMiss,
    L1DWriteMiss,
    LlcMiss,
    BranchMisprediction,
    PageFaults,
    ContextSwitches,
}

impl fmt::Display for PmuEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CpuCycles => write!(f, "cpu_cycles"),
            Self::InstructionsRetired => write!(f, "instructions_retired"),
            Self::L1DReadMiss => write!(f, "l1d_read_miss"),
            Self::L1DWriteMiss => write!(f, "l1d_write_miss"),
            Self::LlcMiss => write!(f, "llc_miss"),
            Self::BranchMisprediction => write!(f, "branch_misprediction"),
            Self::PageFaults => write!(f, "page_faults"),
            Self::ContextSwitches => write!(f, "context_switches"),
        }
    }
}

impl FromStr for PmuEventType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "cpu_cycles" | "cycles" => Ok(Self::CpuCycles),
            "instructions_retired" | "instructions" | "instrs" => Ok(Self::InstructionsRetired),
            "l1d_read_miss" | "l1d_read" => Ok(Self::L1DReadMiss),
            "l1d_write_miss" | "l1d_write" => Ok(Self::L1DWriteMiss),
            "llc_miss" | "llc" | "cache_miss" => Ok(Self::LlcMiss),
            "branch_misprediction" | "branch_miss" | "branches" => Ok(Self::BranchMisprediction),
            "page_faults" | "faults" => Ok(Self::PageFaults),
            "context_switches" | "switches" => Ok(Self::ContextSwitches),
            other => Err(format!("Unknown PMU event type: {}", other)),
        }
    }
}

/// An identified hot instruction or function symbol
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotspotSymbol {
    pub symbol: String,
    pub demangled_symbol: String,
    pub sample_count: u64,
    pub percentage: f64,
    pub module_or_class: String,
}

/// A recorded hardware PMU sample record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmuSampleRecord {
    pub timestamp_secs: u64,
    pub event_type: PmuEventType,
    pub raw_count: u64,
    pub sample_period_ms: u64,
    pub cmpi: f64,
    pub bmpi: f64,
    pub ipc: f64,
    pub top_hotspot: Option<HotspotSymbol>,
}

/// Probe lifecycle status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PmuProbeStatus {
    Active,
    Idle,
    Error(String),
}

impl fmt::Display for PmuProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Idle => write!(f, "idle"),
            Self::Error(err) => write!(f, "error: {}", err),
        }
    }
}

/// Configuration of an active or configured PMU probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmuProbeConfig {
    pub id: String,
    pub target_pid: Option<u32>,
    pub target_server: Option<String>,
    pub sample_rate_hz: u32,
    pub status: PmuProbeStatus,
    pub enabled_events: Vec<PmuEventType>,
    pub created_at: u64,
}

/// Aggregated hardware performance metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmuMetricsSummary {
    pub total_samples: u64,
    pub instructions_retired: u64,
    pub cpu_cycles: u64,
    pub l1d_misses: u64,
    pub llc_misses: u64,
    pub branch_mispredictions: u64,
    pub cmpi_l1d: f64,
    pub cmpi_llc: f64,
    pub bmpi: f64,
    pub ipc: f64,
    pub top_hotspots: Vec<HotspotSymbol>,
    pub active_probes: usize,
    pub simulation_mode: bool,
}

impl Default for PmuMetricsSummary {
    fn default() -> Self {
        Self {
            total_samples: 0,
            instructions_retired: 0,
            cpu_cycles: 0,
            l1d_misses: 0,
            llc_misses: 0,
            branch_mispredictions: 0,
            cmpi_l1d: 0.0,
            cmpi_llc: 0.0,
            bmpi: 0.0,
            ipc: 0.0,
            top_hotspots: Vec::new(),
            active_probes: 0,
            simulation_mode: false,
        }
    }
}

impl PmuMetricsSummary {
    /// Compute CMPI, BMPI, and IPC ratios from raw counter values
    pub fn compute_ratios(&mut self) {
        if self.instructions_retired > 0 {
            let instrs = self.instructions_retired as f64;
            self.cmpi_l1d = self.l1d_misses as f64 / instrs;
            self.cmpi_llc = self.llc_misses as f64 / instrs;
            self.bmpi = self.branch_mispredictions as f64 / instrs;
        } else {
            self.cmpi_l1d = 0.0;
            self.cmpi_llc = 0.0;
            self.bmpi = 0.0;
        }

        if self.cpu_cycles > 0 {
            self.ipc = self.instructions_retired as f64 / self.cpu_cycles as f64;
        } else {
            self.ipc = 0.0;
        }
    }

    /// Render plain-text status summary table (zero emojis)
    pub fn render_plain_status(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Autonomous Hardware PMU & Cache Miss Profile ===\n");
        out.push_str(&format!("Execution Mode:    {}\n", if self.simulation_mode { "Simulation (Fallback)" } else { "Hardware PMU (Native)" }));
        out.push_str(&format!("Active Probes:     {}\n", self.active_probes));
        out.push_str(&format!("Total Samples:     {}\n", self.total_samples));
        out.push_str(&format!("Retired Instrs:    {}\n", self.instructions_retired));
        out.push_str(&format!("CPU Cycles:        {}\n", self.cpu_cycles));
        out.push_str(&format!("IPC (Instr/Cycle): {:.3}\n", self.ipc));
        out.push_str(&format!("L1D Cache Misses:  {}\n", self.l1d_misses));
        out.push_str(&format!("L1D CMPI:          {:.6}\n", self.cmpi_l1d));
        out.push_str(&format!("LLC Cache Misses:  {}\n", self.llc_misses));
        out.push_str(&format!("LLC CMPI:          {:.6}\n", self.cmpi_llc));
        out.push_str(&format!("Branch Mispredicts:{}\n", self.branch_mispredictions));
        out.push_str(&format!("BMPI:              {:.6}\n", self.bmpi));

        if !self.top_hotspots.is_empty() {
            out.push_str("\n--- Top Execution Hotspots ---\n");
            out.push_str(&format!("{:<6} {:<10} {:<32} {}\n", "Rank", "Share", "Module / Class", "Demangled Symbol"));
            for (idx, hot) in self.top_hotspots.iter().enumerate().take(10) {
                out.push_str(&format!(
                    "#{:<5} {:>6.2}%   {:<32} {}\n",
                    idx + 1,
                    hot.percentage * 100.0,
                    hot.module_or_class,
                    hot.demangled_symbol
                ));
            }
        }
        out
    }
}

/// Persistent registry of active and configured PMU probes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PmuRegistry {
    pub probes: Vec<PmuProbeConfig>,
}

impl PmuRegistry {
    /// Load registry from file with advisory file locking
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let lock_path = &paths.pmu_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        let file_path = &paths.pmu_probes_file;
        if !file_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(file_path)?;
        let reg: Self = serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse PMU registry: {}", e)))?;
        Ok(reg)
    }

    /// Save registry to file with advisory file locking
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_path = &paths.pmu_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        if let Some(parent) = paths.pmu_probes_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize PMU registry: {}", e)))?;
        fs::write(&paths.pmu_probes_file, json)?;
        Ok(())
    }

    /// Add or update a probe configuration
    pub fn add_probe(&mut self, paths: &CraftPaths, probe: PmuProbeConfig) -> Result<()> {
        self.probes.retain(|p| p.id != probe.id);
        self.probes.push(probe);
        self.save(paths)
    }

    /// Remove a probe by ID
    pub fn remove_probe(&mut self, paths: &CraftPaths, id: &str) -> Result<bool> {
        let initial_len = self.probes.len();
        self.probes.retain(|p| p.id != id);
        let removed = self.probes.len() < initial_len;
        if removed {
            self.save(paths)?;
        }
        Ok(removed)
    }

    /// Find a probe configuration by ID
    pub fn get_probe(&self, id: &str) -> Option<&PmuProbeConfig> {
        self.probes.iter().find(|p| p.id == id)
    }

    /// List all configured probes
    pub fn list_probes(&self) -> &[PmuProbeConfig] {
        &self.probes
    }

    /// Record live PMU metrics snapshot into state.json
    pub fn record_state(&self, paths: &CraftPaths, metrics: &PmuMetricsSummary) -> Result<()> {
        let lock_path = &paths.pmu_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        if let Some(parent) = paths.pmu_state_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(metrics)
            .map_err(|e| CraftError::Config(format!("Failed to serialize PMU state: {}", e)))?;
        fs::write(&paths.pmu_state_file, json)?;
        Ok(())
    }

    /// Load live PMU metrics snapshot from state.json
    pub fn load_state(paths: &CraftPaths) -> Result<PmuMetricsSummary> {
        let lock_path = &paths.pmu_lock;
        let _lock = LockGuard::acquire(lock_path)?;

        if !paths.pmu_state_file.exists() {
            return Ok(PmuMetricsSummary::default());
        }

        let content = fs::read_to_string(&paths.pmu_state_file)?;
        let summary: PmuMetricsSummary = serde_json::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse PMU state: {}", e)))?;
        Ok(summary)
    }
}

/// Advisory lock guard for PMU operations
struct LockGuard {
    _file: File,
}

impl LockGuard {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire pmu.lock: {}", e)))?;
        Ok(Self { _file: file })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pmu_event_type_display_and_parsing() {
        let event = PmuEventType::InstructionsRetired;
        assert_eq!(event.to_string(), "instructions_retired");
        assert_eq!(PmuEventType::from_str("instructions").unwrap(), PmuEventType::InstructionsRetired);
        assert_eq!(PmuEventType::from_str("cycles").unwrap(), PmuEventType::CpuCycles);
        assert_eq!(PmuEventType::from_str("llc").unwrap(), PmuEventType::LlcMiss);
        assert_eq!(PmuEventType::from_str("branch_miss").unwrap(), PmuEventType::BranchMisprediction);
        assert!(PmuEventType::from_str("invalid_event").is_err());
    }

    #[test]
    fn test_pmu_ratios_computation() {
        let mut summary = PmuMetricsSummary {
            instructions_retired: 10_000_000,
            cpu_cycles: 5_000_000,
            l1d_misses: 200_000,
            llc_misses: 25_000,
            branch_mispredictions: 100_000,
            ..Default::default()
        };

        summary.compute_ratios();

        // IPC = 10,000,000 / 5,000,000 = 2.0
        assert!((summary.ipc - 2.0).abs() < 1e-6);
        // CMPI L1D = 200,000 / 10,000,000 = 0.02
        assert!((summary.cmpi_l1d - 0.02).abs() < 1e-6);
        // CMPI LLC = 25,000 / 10,000,000 = 0.0025
        assert!((summary.cmpi_llc - 0.0025).abs() < 1e-6);
        // BMPI = 100,000 / 10,000,000 = 0.01
        assert!((summary.bmpi - 0.01).abs() < 1e-6);
    }

    #[test]
    fn test_pmu_registry_persistence() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let mut reg = PmuRegistry::load(&paths).unwrap();
        assert!(reg.list_probes().is_empty());

        let probe = PmuProbeConfig {
            id: "probe-test-1".to_string(),
            target_pid: Some(12345),
            target_server: Some("survival".to_string()),
            sample_rate_hz: 99,
            status: PmuProbeStatus::Active,
            enabled_events: vec![PmuEventType::CpuCycles, PmuEventType::InstructionsRetired],
            created_at: 1000,
        };

        reg.add_probe(&paths, probe.clone()).unwrap();

        let loaded = PmuRegistry::load(&paths).unwrap();
        assert_eq!(loaded.list_probes().len(), 1);
        assert_eq!(loaded.get_probe("probe-test-1").unwrap().id, "probe-test-1");

        let removed = reg.remove_probe(&paths, "probe-test-1").unwrap();
        assert!(removed);
        let loaded2 = PmuRegistry::load(&paths).unwrap();
        assert!(loaded2.list_probes().is_empty());
    }

    #[test]
    fn test_pmu_metrics_plain_status() {
        let summary = PmuMetricsSummary {
            total_samples: 42,
            instructions_retired: 1_000_000,
            cpu_cycles: 800_000,
            l1d_misses: 10_000,
            llc_misses: 1_500,
            branch_mispredictions: 5_000,
            cmpi_l1d: 0.01,
            cmpi_llc: 0.0015,
            bmpi: 0.005,
            ipc: 1.25,
            active_probes: 1,
            simulation_mode: true,
            top_hotspots: vec![HotspotSymbol {
                symbol: "_ZN3net10minecraft6Server4tickEv".to_string(),
                demangled_symbol: "net::minecraft::Server::tick()".to_string(),
                sample_count: 500,
                percentage: 0.50,
                module_or_class: "server.jar".to_string(),
            }],
        };

        let rendered = summary.render_plain_status();
        assert!(rendered.contains("Simulation (Fallback)"));
        assert!(rendered.contains("net::minecraft::Server::tick()"));
        assert!(rendered.contains("50.00%"));
    }
}
