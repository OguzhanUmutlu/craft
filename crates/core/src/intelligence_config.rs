use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutopilotMode {
    Disabled,
    Advisory,
    Autonomous,
}

impl Default for AutopilotMode {
    fn default() -> Self {
        Self::Advisory
    }
}

impl std::fmt::Display for AutopilotMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Advisory => write!(f, "advisory"),
            Self::Autonomous => write!(f, "autonomous"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationAction {
    EntityCull,
    GarbageCollectionHint,
    OffPeakRestart,
}

impl std::fmt::Display for RemediationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EntityCull => write!(f, "entity_cull"),
            Self::GarbageCollectionHint => write!(f, "gc_hint"),
            Self::OffPeakRestart => write!(f, "off_peak_restart"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    MsptSpike,
    SustainedTickDrop,
    MemoryLeakGradient,
    GcThrashing,
    EntityRunaway,
    CpuStarvation,
}

impl std::fmt::Display for AnomalyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MsptSpike => write!(f, "MSPT Spike"),
            Self::SustainedTickDrop => write!(f, "Sustained Tick Drop"),
            Self::MemoryLeakGradient => write!(f, "Memory Leak Gradient"),
            Self::GcThrashing => write!(f, "GC Thrashing"),
            Self::EntityRunaway => write!(f, "Entity Runaway"),
            Self::CpuStarvation => write!(f, "CPU Starvation"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnomalySeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AnomalySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyRecord {
    pub id: String,
    pub timestamp: u64,
    pub server_name: String,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub confidence: f64,
    pub metric_value: f64,
    pub threshold_value: f64,
    pub description: String,
    pub suggested_remediation: Option<RemediationAction>,
    pub remediated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub timestamp: u64,
    pub server_name: String,
    pub tps_current: f64,
    pub mspt_current: f64,
    pub mspt_p50: f64,
    pub mspt_p95: f64,
    pub mspt_p99: f64,
    pub memory_rss_bytes: u64,
    pub memory_growth_rate_mb_min: f64,
    pub predicted_tte_seconds: Option<u64>,
    pub cpu_usage_percent: f32,
    pub active_players: u32,
    pub anomalies: Vec<AnomalyRecord>,
    pub recommendations: Vec<String>,
    pub jfr_profile_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligencePolicy {
    #[serde(default)]
    pub mode: AutopilotMode,
    #[serde(default = "default_sample_interval")]
    pub sample_interval_secs: u64,
    #[serde(default = "default_window_samples")]
    pub window_samples: usize,
    #[serde(default = "default_mspt_warning")]
    pub mspt_warning_ms: f64,
    #[serde(default = "default_mspt_critical")]
    pub mspt_critical_ms: f64,
    #[serde(default = "default_mem_slope")]
    pub memory_leak_slope_mb_min: f64,
    #[serde(default = "default_gc_freq")]
    pub gc_thrashing_frequency_per_min: u32,
    #[serde(default = "default_true")]
    pub profiling_enabled: bool,
    #[serde(default = "default_profile_duration")]
    pub profiling_duration_secs: u64,
    #[serde(default = "default_remediations")]
    pub allowed_remediations: Vec<RemediationAction>,
    #[serde(default = "default_off_peak_start")]
    pub off_peak_start_hour: u8,
    #[serde(default = "default_off_peak_end")]
    pub off_peak_end_hour: u8,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
}

fn default_sample_interval() -> u64 { 5 }
fn default_window_samples() -> usize { 120 }
fn default_mspt_warning() -> f64 { 40.0 }
fn default_mspt_critical() -> f64 { 48.0 }
fn default_mem_slope() -> f64 { 5.0 }
fn default_gc_freq() -> u32 { 8 }
fn default_true() -> bool { true }
fn default_profile_duration() -> u64 { 30 }
fn default_off_peak_start() -> u8 { 3 }
fn default_off_peak_end() -> u8 { 5 }
fn default_min_confidence() -> f64 { 0.80 }

fn default_remediations() -> Vec<RemediationAction> {
    vec![
        RemediationAction::EntityCull,
        RemediationAction::GarbageCollectionHint,
        RemediationAction::OffPeakRestart,
    ]
}

impl Default for IntelligencePolicy {
    fn default() -> Self {
        Self {
            mode: AutopilotMode::Advisory,
            sample_interval_secs: default_sample_interval(),
            window_samples: default_window_samples(),
            mspt_warning_ms: default_mspt_warning(),
            mspt_critical_ms: default_mspt_critical(),
            memory_leak_slope_mb_min: default_mem_slope(),
            gc_thrashing_frequency_per_min: default_gc_freq(),
            profiling_enabled: true,
            profiling_duration_secs: default_profile_duration(),
            allowed_remediations: default_remediations(),
            off_peak_start_hour: default_off_peak_start(),
            off_peak_end_hour: default_off_peak_end(),
            min_confidence: default_min_confidence(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntelligenceRegistry {
    #[serde(default)]
    pub global_policy: IntelligencePolicy,
    #[serde(default)]
    pub server_policies: HashMap<String, IntelligencePolicy>,
}

impl IntelligenceRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let path = &paths.intelligence_file;
        if !path.exists() {
            return Ok(Self::default());
        }

        let lock_file = paths.locks_dir.join("intelligence.lock");
        if let Some(parent) = lock_file.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file)
            .map_err(CraftError::Io)?;
        lock_handle.lock_shared().map_err(CraftError::Io)?;

        let content = fs::read_to_string(path).map_err(CraftError::Io)?;
        let _ = lock_handle.unlock();

        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse intelligence.toml: {}", e)))
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let path = &paths.intelligence_file;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }

        let lock_file = paths.locks_dir.join("intelligence.lock");
        if let Some(parent) = lock_file.parent() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
        let lock_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file)
            .map_err(CraftError::Io)?;
        lock_handle.lock_exclusive().map_err(CraftError::Io)?;

        let serialized = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize intelligence config: {}", e)))?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, serialized).map_err(CraftError::Io)?;
        fs::rename(&temp_path, path).map_err(CraftError::Io)?;

        let _ = lock_handle.unlock();
        Ok(())
    }

    pub fn get_policy(&self, server: &str) -> IntelligencePolicy {
        self.server_policies
            .get(server)
            .cloned()
            .unwrap_or_else(|| self.global_policy.clone())
    }

    pub fn set_policy(&mut self, server: String, policy: IntelligencePolicy) {
        self.server_policies.insert(server, policy);
    }
}

pub fn format_report_markdown(report: &DiagnosticReport) -> String {
    let mut md = String::with_capacity(2048);
    md.push_str(&format!("# Performance Diagnostic Report: {}\n\n", report.server_name));
    md.push_str(&format!("- **Timestamp**: {}\n", report.timestamp));
    md.push_str(&format!("- **Current TPS**: {:.2}\n", report.tps_current));
    md.push_str(&format!("- **Current MSPT**: {:.2} ms\n", report.mspt_current));
    md.push_str(&format!("- **Tick Latencies**: P50: {:.2} ms | P95: {:.2} ms | P99: {:.2} ms\n", report.mspt_p50, report.mspt_p95, report.mspt_p99));
    md.push_str(&format!("- **Memory RSS**: {:.2} MB\n", (report.memory_rss_bytes as f64) / (1024.0 * 1024.0)));
    md.push_str(&format!("- **Memory Gradient**: {:+.2} MB/min\n", report.memory_growth_rate_mb_min));
    if let Some(tte) = report.predicted_tte_seconds {
        let mins = tte / 60;
        let secs = tte % 60;
        md.push_str(&format!("- **Predicted Time-To-Exhaustion**: {}m {}s\n", mins, secs));
    } else {
        md.push_str("- **Predicted Time-To-Exhaustion**: None (stable)\n");
    }
    md.push_str(&format!("- **CPU Usage**: {:.1}%\n", report.cpu_usage_percent));
    md.push_str(&format!("- **Connected Players**: {}\n\n", report.active_players));

    md.push_str("## Active Anomalies\n\n");
    if report.anomalies.is_empty() {
        md.push_str("No operational anomalies detected. Server is operating within expected nominal parameters.\n\n");
    } else {
        md.push_str("| Type | Severity | Confidence | Metric | Threshold | Remediation |\n");
        md.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");
        for a in &report.anomalies {
            let rem_str = a.suggested_remediation.map(|r| r.to_string()).unwrap_or_else(|| "None".to_string());
            md.push_str(&format!(
                "| {} | {} | {:.0}% | {:.2} | {:.2} | {} |\n",
                a.anomaly_type, a.severity, a.confidence * 100.0, a.metric_value, a.threshold_value, rem_str
            ));
        }
        md.push_str("\n");
    }

    md.push_str("## Recommendations\n\n");
    if report.recommendations.is_empty() {
        md.push_str("- Maintain current configuration and monitoring cadence.\n");
    } else {
        for rec in &report.recommendations {
            md.push_str(&format!("- {}\n", rec));
        }
    }

    if let Some(profile_path) = &report.jfr_profile_file {
        md.push_str(&format!("\n**JFR Execution Profile**: `{}`\n", profile_path));
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_intelligence_policy_defaults() {
        let policy = IntelligencePolicy::default();
        assert_eq!(policy.mode, AutopilotMode::Advisory);
        assert_eq!(policy.sample_interval_secs, 5);
        assert_eq!(policy.window_samples, 120);
        assert_eq!(policy.mspt_warning_ms, 40.0);
        assert_eq!(policy.mspt_critical_ms, 48.0);
        assert_eq!(policy.min_confidence, 0.80);
    }

    #[test]
    fn test_intelligence_registry_roundtrip() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let mut reg = IntelligenceRegistry::load(&paths).unwrap();
        assert_eq!(reg.server_policies.len(), 0);

        let mut custom = IntelligencePolicy::default();
        custom.mode = AutopilotMode::Autonomous;
        custom.mspt_critical_ms = 45.0;
        reg.set_policy("survival".to_string(), custom);

        reg.save(&paths).unwrap();

        let loaded = IntelligenceRegistry::load(&paths).unwrap();
        assert_eq!(loaded.server_policies.len(), 1);
        let surv_pol = loaded.get_policy("survival");
        assert_eq!(surv_pol.mode, AutopilotMode::Autonomous);
        assert_eq!(surv_pol.mspt_critical_ms, 45.0);

        // Fallback for unconfigured server returns global policy
        let fallback = loaded.get_policy("unknown");
        assert_eq!(fallback.mode, AutopilotMode::Advisory);
    }
}
