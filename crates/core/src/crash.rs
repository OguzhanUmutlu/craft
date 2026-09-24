// crates/core/src/crash.rs
//
// Autonomous AI-Guided Static Analysis, Real-Time Memory Leak Detection &
// Automated Core Dump Triaging for Craft.
// Strictly zero emojis.

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of crash or incident being triaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashType {
    NativeCoreDump,
    JvmHsErr,
    MemoryLeak,
}

impl fmt::Display for CrashType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativeCoreDump => write!(f, "NativeCoreDump"),
            Self::JvmHsErr => write!(f, "JvmHsErr"),
            Self::MemoryLeak => write!(f, "MemoryLeak"),
        }
    }
}

impl std::str::FromStr for CrashType {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "core" | "coredump" | "native" | "nativecoredump" => Ok(Self::NativeCoreDump),
            "jvm" | "hserr" | "hs_err" | "jvmhserr" => Ok(Self::JvmHsErr),
            "leak" | "memoryleak" | "memleak" => Ok(Self::MemoryLeak),
            _ => Err(CraftError::Config(format!("Unknown crash type: {}", s))),
        }
    }
}

/// Severity classification for triaged incidents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CrashSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for CrashSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// An individual native or simulated heap allocation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationRecord {
    pub ptr: u64,
    pub size: usize,
    pub timestamp: u64,
    pub callsite_symbol: String,
    pub stack_depth: usize,
}

/// A detected orphan memory leak candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakCandidate {
    pub callsite_symbol: String,
    pub orphan_count: usize,
    pub total_leaked_bytes: u64,
    pub growth_rate_bytes_per_sec: f64,
    pub confidence_score: f64,
}

/// ELF Core Dump Note Types according to Linux kernel ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElfCoreNoteType {
    PrStatus,
    FpregSet,
    PrPsInfo,
    PrAuxv,
    SigInfo,
    File,
    Unknown(u32),
}

impl ElfCoreNoteType {
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::PrStatus,
            2 => Self::FpregSet,
            3 => Self::PrPsInfo,
            6 => Self::PrAuxv,
            0x53494749 => Self::SigInfo,
            0x46494c45 => Self::File,
            other => Self::Unknown(other),
        }
    }

    pub fn to_u32(&self) -> u32 {
        match self {
            Self::PrStatus => 1,
            Self::FpregSet => 2,
            Self::PrPsInfo => 3,
            Self::PrAuxv => 6,
            Self::SigInfo => 0x53494749,
            Self::File => 0x46494c45,
            Self::Unknown(val) => *val,
        }
    }
}

/// Basic ELF64 file header representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElfCoreDumpHeader {
    pub ei_class: u8,
    pub ei_data: u8,
    pub e_type: u16,
    pub e_machine: u16,
    pub e_phoff: u64,
    pub e_phnum: u16,
}

/// Parsed metadata extracted from an ELF core dump.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElfCoreParsedInfo {
    pub signal: i32,
    pub signal_name: String,
    pub fault_address: u64,
    pub fault_instruction_pointer: u64,
    pub process_name: String,
    pub pid: u32,
    pub thread_count: usize,
    pub memory_regions_count: usize,
}

/// Parsed metadata extracted from a JVM `hs_err_pid<pid>.log` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmCrashLogParsed {
    pub signal: String,
    pub jvm_version: String,
    pub problematic_frame: String,
    pub fault_address: String,
    pub thread_name: String,
    pub native_frames_count: usize,
    pub java_frames_count: usize,
    pub vm_operation: String,
}

/// A structured remediation step synthesized by AI or heuristic rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashRemediationAction {
    pub action_type: String,
    pub title: String,
    pub description: String,
    pub automated_command: Option<String>,
    pub risk_level: String,
}

/// Complete triage report for an incident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashTriageReport {
    pub id: String,
    pub server: Option<String>,
    pub timestamp: u64,
    pub crash_type: CrashType,
    pub severity: CrashSeverity,
    pub summary: String,
    pub root_cause_analysis: String,
    pub fault_location: String,
    pub leak_candidates: Vec<LeakCandidate>,
    pub remediation_playbook: Vec<CrashRemediationAction>,
}

/// Aggregated status summary of all crash reports and memory tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrashTriageStatusSummary {
    pub active_leaks: usize,
    pub total_reports: usize,
    pub critical_reports: usize,
    pub total_leaked_bytes: u64,
    pub last_triaged_timestamp: Option<u64>,
    pub recent_reports: Vec<CrashTriageReport>,
}

/// Synthetic or benchmark profiling metrics for crash parsing & leak analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrashTriageBenchmarkMetrics {
    pub processed_dumps: usize,
    pub avg_parse_micros: f64,
    pub p95_parse_micros: f64,
    pub dumps_per_sec: f64,
    pub alloc_diff_rate_ops_per_sec: f64,
}

/// Transactional persistent registry for triaged crash reports and leak status.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrashTriageRegistry {
    pub reports: Vec<CrashTriageReport>,
    pub status_summary: CrashTriageStatusSummary,
    #[serde(skip)]
    registry_path: PathBuf,
    #[serde(skip)]
    lock_path: PathBuf,
}

impl CrashTriageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_paths(registry_path: PathBuf, lock_path: PathBuf) -> Self {
        Self {
            reports: Vec::new(),
            status_summary: CrashTriageStatusSummary::default(),
            registry_path,
            lock_path,
        }
    }

    fn lock_file(&self) -> Result<File> {
        if let Some(parent) = self.lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)?;
        file.lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to acquire crash lock: {}", e)))?;
        Ok(file)
    }

    /// Loads the registry from disk under `~/.craft/crash/registry.json`.
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let mut reg = Self::with_paths(paths.crash_registry_file.clone(), paths.crash_lock.clone());
        if !reg.registry_path.exists() {
            return Ok(reg);
        }

        let _guard = reg.lock_file()?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(&reg.registry_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        if !content.trim().is_empty() {
            let data: CrashTriageRegistry = serde_json::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse crash registry: {}", e)))?;
            reg.reports = data.reports;
            reg.status_summary = data.status_summary;
        }

        Ok(reg)
    }

    /// Saves the registry to disk atomically under `~/.craft/crash/registry.json`.
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file_path = &paths.crash_registry_file;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let _guard = self.lock_file()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize crash registry: {}", e)))?;

        let tmp_path = file_path.with_extension("tmp");
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;

        tmp_file.write_all(json.as_bytes())?;
        tmp_file.sync_all()?;

        fs::rename(&tmp_path, file_path)?;
        Ok(())
    }

    /// Appends a new crash report to registry and updates status summary.
    pub fn add_report(&mut self, report: CrashTriageReport, paths: &CraftPaths) -> Result<()> {
        let report_file = paths.crash_report_path(&report.id);
        if let Some(parent) = report_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let report_json = serde_json::to_string_pretty(&report)
            .map_err(|e| CraftError::Config(format!("Failed to serialize report: {}", e)))?;
        fs::write(&report_file, report_json)?;

        self.reports.retain(|r| r.id != report.id);
        self.reports.push(report);
        self.recompute_summary();
        self.save(paths)
    }

    /// Retrieves an existing report by ID.
    pub fn get_report(&self, id: &str) -> Option<&CrashTriageReport> {
        self.reports.iter().find(|r| r.id == id)
    }

    /// Removes a report from registry and deletes its JSON file.
    pub fn remove_report(&mut self, id: &str, paths: &CraftPaths) -> Result<bool> {
        let initial_len = self.reports.len();
        self.reports.retain(|r| r.id != id);
        let removed = self.reports.len() < initial_len;

        if removed {
            let report_file = paths.crash_report_path(id);
            if report_file.exists() {
                let _ = fs::remove_file(report_file);
            }
            self.recompute_summary();
            self.save(paths)?;
        }
        Ok(removed)
    }

    /// Resets all metrics and reports in the registry.
    pub fn reset_metrics(&mut self, paths: &CraftPaths) -> Result<()> {
        self.reports.clear();
        self.status_summary = CrashTriageStatusSummary::default();
        self.save(paths)
    }

    /// Recomputes internal status summary.
    pub fn recompute_summary(&mut self) {
        let total_reports = self.reports.len();
        let mut critical_reports = 0;
        let mut total_leaked_bytes = 0;
        let mut active_leaks = 0;

        for r in &self.reports {
            if r.severity == CrashSeverity::Critical {
                critical_reports += 1;
            }
            if r.crash_type == CrashType::MemoryLeak {
                active_leaks += r.leak_candidates.len();
                for leak in &r.leak_candidates {
                    total_leaked_bytes += leak.total_leaked_bytes;
                }
            }
        }

        let last_triaged = self.reports.iter().map(|r| r.timestamp).max();

        let mut sorted = self.reports.clone();
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        sorted.truncate(10);

        self.status_summary = CrashTriageStatusSummary {
            active_leaks,
            total_reports,
            critical_reports,
            total_leaked_bytes,
            last_triaged_timestamp: last_triaged,
            recent_reports: sorted,
        };
    }
}

/// Helper function to generate current timestamp in seconds.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Helper function to format human-readable bytes without emojis.
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Render plain-text status summary table without emojis.
pub fn render_crash_status_text(summary: &CrashTriageStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("          Autonomous Crash Triaging & Real-Time Memory Leak Status             \n");
    out.push_str("================================================================================\n");
    out.push_str(&format!("  Total Triaged Reports : {}\n", summary.total_reports));
    out.push_str(&format!("  Critical Severity     : {}\n", summary.critical_reports));
    out.push_str(&format!("  Active Memory Leaks   : {}\n", summary.active_leaks));
    out.push_str(&format!("  Total Leaked Bytes    : {}\n", format_bytes(summary.total_leaked_bytes)));
    if let Some(ts) = summary.last_triaged_timestamp {
        out.push_str(&format!("  Last Triage Timestamp : {}s\n", ts));
    } else {
        out.push_str("  Last Triage Timestamp : None\n");
    }
    out.push_str("--------------------------------------------------------------------------------\n");
    out.push_str("Recent Triaged Reports:\n");
    if summary.recent_reports.is_empty() {
        out.push_str("  [INFO] No crash reports or memory leak incidents recorded.\n");
    } else {
        for (i, r) in summary.recent_reports.iter().enumerate() {
            let srv = r.server.as_deref().unwrap_or("global");
            out.push_str(&format!(
                "  [{}] ID: {} | Server: {} | Type: {} | Severity: [{}] | Location: {}\n",
                i + 1,
                r.id,
                srv,
                r.crash_type,
                r.severity,
                r.fault_location
            ));
            out.push_str(&format!("      Summary: {}\n", r.summary));
        }
    }
    out.push_str("================================================================================\n");
    out
}

/// Render plain-text triage report details without emojis.
pub fn render_crash_report_text(report: &CrashTriageReport) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str(&format!("                     Crash Triage Report: {}                    \n", report.id));
    out.push_str("================================================================================\n");
    out.push_str(&format!("  Server              : {}\n", report.server.as_deref().unwrap_or("N/A")));
    out.push_str(&format!("  Incident Type       : {}\n", report.crash_type));
    out.push_str(&format!("  Severity Rating     : [{}]\n", report.severity));
    out.push_str(&format!("  Fault Location      : {}\n", report.fault_location));
    out.push_str(&format!("  Incident Summary    : {}\n", report.summary));
    out.push_str("--------------------------------------------------------------------------------\n");
    out.push_str("Root Cause Analysis:\n");
    out.push_str(&format!("  {}\n", report.root_cause_analysis));

    if !report.leak_candidates.is_empty() {
        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str("Detected Memory Leak Candidates:\n");
        for (idx, leak) in report.leak_candidates.iter().enumerate() {
            out.push_str(&format!(
                "  [{}] Symbol: {} | Orphans: {} | Leaked: {} | Rate: {:.2} B/s | Confidence: {:.1}%\n",
                idx + 1,
                leak.callsite_symbol,
                leak.orphan_count,
                format_bytes(leak.total_leaked_bytes),
                leak.growth_rate_bytes_per_sec,
                leak.confidence_score * 100.0
            ));
        }
    }

    if !report.remediation_playbook.is_empty() {
        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str("AI-Synthesized Remediation Playbook:\n");
        for (idx, action) in report.remediation_playbook.iter().enumerate() {
            out.push_str(&format!(
                "  [{}] Action: {} [{}]\n",
                idx + 1,
                action.title,
                action.risk_level
            ));
            out.push_str(&format!("      {}\n", action.description));
            if let Some(cmd) = &action.automated_command {
                out.push_str(&format!("      Command: {}\n", cmd));
            }
        }
    }
    out.push_str("================================================================================\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_crash_type_parsing_and_display() {
        assert_eq!("core".parse::<CrashType>().unwrap(), CrashType::NativeCoreDump);
        assert_eq!("hs_err".parse::<CrashType>().unwrap(), CrashType::JvmHsErr);
        assert_eq!("leak".parse::<CrashType>().unwrap(), CrashType::MemoryLeak);
        assert_eq!(CrashType::NativeCoreDump.to_string(), "NativeCoreDump");
    }

    #[test]
    fn test_crash_severity_ordering() {
        assert!(CrashSeverity::Critical > CrashSeverity::High);
        assert!(CrashSeverity::High > CrashSeverity::Medium);
        assert!(CrashSeverity::Medium > CrashSeverity::Low);
    }

    #[test]
    fn test_crash_registry_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut registry = CrashTriageRegistry::with_paths(paths.crash_registry_file.clone(), paths.crash_lock.clone());
        let report = CrashTriageReport {
            id: "crash-test-01".to_string(),
            server: Some("survival".to_string()),
            timestamp: 1700000000,
            crash_type: CrashType::NativeCoreDump,
            severity: CrashSeverity::Critical,
            summary: "Synthetic SIGSEGV".to_string(),
            root_cause_analysis: "Null pointer dereference at 0x0".to_string(),
            fault_location: "native_handler+0x12".to_string(),
            leak_candidates: Vec::new(),
            remediation_playbook: vec![CrashRemediationAction {
                action_type: "HotPatch".to_string(),
                title: "Apply Patch".to_string(),
                description: "Patch null check in binary".to_string(),
                automated_command: Some("craft patch apply -s survival -p null_check".to_string()),
                risk_level: "Low".to_string(),
            }],
        };

        registry.add_report(report.clone(), &paths).unwrap();

        let loaded = CrashTriageRegistry::load(&paths).unwrap();
        assert_eq!(loaded.reports.len(), 1);
        assert_eq!(loaded.status_summary.total_reports, 1);
        assert_eq!(loaded.status_summary.critical_reports, 1);

        let retrieved = loaded.get_report("crash-test-01").unwrap();
        assert_eq!(retrieved.summary, "Synthetic SIGSEGV");

        // Format checks (zero emojis)
        let text = render_crash_status_text(&loaded.status_summary);
        assert!(!text.contains("🚨"));
        assert!(text.contains("Critical Severity     : 1"));

        let report_text = render_crash_report_text(retrieved);
        assert!(report_text.contains("Crash Triage Report: crash-test-01"));
        assert!(report_text.contains("AI-Synthesized Remediation Playbook:"));

        // Remove report
        let mut reg_mut = loaded;
        assert!(reg_mut.remove_report("crash-test-01", &paths).unwrap());
        assert_eq!(reg_mut.reports.len(), 0);
    }
}
