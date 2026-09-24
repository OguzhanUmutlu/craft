// crates/daemon/src/crash_service.rs
//
// Autonomous Crash Triaging, Real-Time Memory Leak Detection & Supervisor Service.
// Strictly zero emojis.

use craft_core::crash::{
    CrashSeverity, CrashTriageBenchmarkMetrics, CrashTriageRegistry, CrashTriageReport,
    CrashTriageStatusSummary, CrashType,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_net::crash_triage::{
    benchmark_crash_triage, AiTriageAdvisor, ElfCoreDumpParser, JvmHsErrParser,
};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

static INSTANCE: OnceLock<Arc<CrashTriageService>> = OnceLock::new();

pub struct CrashTriageService {
    paths: CraftPaths,
    total_triaged: Arc<AtomicU64>,
    critical_triaged: Arc<AtomicU64>,
    active_leaks: Arc<AtomicU64>,
    total_leaked_bytes: Arc<AtomicU64>,
    last_parse_duration_ms: Arc<AtomicU64>,
}

impl CrashTriageService {
    pub fn new(paths: CraftPaths) -> Self {
        let service = Self {
            paths: paths.clone(),
            total_triaged: Arc::new(AtomicU64::new(0)),
            critical_triaged: Arc::new(AtomicU64::new(0)),
            active_leaks: Arc::new(AtomicU64::new(0)),
            total_leaked_bytes: Arc::new(AtomicU64::new(0)),
            last_parse_duration_ms: Arc::new(AtomicU64::new(0)),
        };

        if let Ok(reg) = CrashTriageRegistry::load(&paths) {
            service.total_triaged.store(reg.status_summary.total_reports as u64, Ordering::Relaxed);
            service.critical_triaged.store(reg.status_summary.critical_reports as u64, Ordering::Relaxed);
            service.active_leaks.store(reg.status_summary.active_leaks as u64, Ordering::Relaxed);
            service.total_leaked_bytes.store(reg.status_summary.total_leaked_bytes, Ordering::Relaxed);
        }

        service
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    /// Retrieves status summary, optionally filtered by server name.
    pub fn get_status(&self, server: Option<&str>) -> Result<CrashTriageStatusSummary> {
        let reg = CrashTriageRegistry::load(&self.paths)?;
        if let Some(srv) = server {
            let filtered: Vec<CrashTriageReport> = reg
                .reports
                .iter()
                .filter(|r| r.server.as_deref() == Some(srv))
                .cloned()
                .collect();
            let total = filtered.len();
            let critical = filtered.iter().filter(|r| r.severity == CrashSeverity::Critical).count();
            let mut leaks = 0;
            let mut leaked_bytes = 0;
            for r in &filtered {
                if r.crash_type == CrashType::MemoryLeak {
                    leaks += r.leak_candidates.len();
                    for l in &r.leak_candidates {
                        leaked_bytes += l.total_leaked_bytes;
                    }
                }
            }
            let last_ts = filtered.iter().map(|r| r.timestamp).max();

            Ok(CrashTriageStatusSummary {
                active_leaks: leaks,
                total_reports: total,
                critical_reports: critical,
                total_leaked_bytes: leaked_bytes,
                last_triaged_timestamp: last_ts,
                recent_reports: filtered,
            })
        } else {
            Ok(reg.status_summary)
        }
    }

    /// Triages a crash dump or log file (ELF core dump or JVM hs_err) and persists the report.
    pub fn triage_file(&self, server: Option<&str>, file_path: &str) -> Result<CrashTriageReport> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(CraftError::Other(format!("Crash file not found: {}", file_path)));
        }

        let t0 = Instant::now();
        let bytes = fs::read(path).map_err(|e| CraftError::Io(e))?;

        let report = if bytes.len() >= 4 && &bytes[0..4] == b"\x7fELF" {
            // ELF Core Dump
            let parsed_elf = ElfCoreDumpParser::parse_bytes(&bytes)?;
            AiTriageAdvisor::triage_core_dump(&parsed_elf, server)
        } else {
            // Attempt JVM hs_err text parsing
            let content = match std::str::from_utf8(&bytes) {
                Ok(c) => c,
                Err(_) => {
                    return Err(CraftError::Other(
                        "File is neither a valid ELF64 core dump nor UTF-8 JVM hs_err log".to_string(),
                    ));
                }
            };
            let parsed_jvm = JvmHsErrParser::parse_str(content)?;
            AiTriageAdvisor::triage_jvm_crash(&parsed_jvm, server)
        };

        let elapsed_ms = t0.elapsed().as_millis() as u64;
        self.last_parse_duration_ms.store(elapsed_ms, Ordering::Relaxed);

        // Update Registry
        let mut reg = CrashTriageRegistry::load(&self.paths)?;
        reg.add_report(report.clone(), &self.paths)?;

        // Update atomics
        self.total_triaged.fetch_add(1, Ordering::Relaxed);
        if report.severity == CrashSeverity::Critical {
            self.critical_triaged.fetch_add(1, Ordering::Relaxed);
        }

        Ok(report)
    }

    /// Lists triaged reports, optionally filtered and limited.
    pub fn list_reports(&self, server: Option<&str>, limit: Option<usize>) -> Result<Vec<CrashTriageReport>> {
        let reg = CrashTriageRegistry::load(&self.paths)?;
        let mut filtered: Vec<CrashTriageReport> = if let Some(srv) = server {
            reg.reports.into_iter().filter(|r| r.server.as_deref() == Some(srv)).collect()
        } else {
            reg.reports
        };

        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if let Some(lim) = limit {
            filtered.truncate(lim);
        }

        Ok(filtered)
    }

    /// Retrieves an existing report by ID.
    pub fn get_report(&self, report_id: &str) -> Result<Option<CrashTriageReport>> {
        let reg = CrashTriageRegistry::load(&self.paths)?;
        Ok(reg.get_report(report_id).cloned())
    }

    /// Runs synthetic high-concurrency crash triaging benchmark.
    pub fn run_bench(&self, iterations: usize) -> Result<CrashTriageBenchmarkMetrics> {
        let count = if iterations == 0 { 25 } else { iterations };
        Ok(benchmark_crash_triage(count))
    }

    /// Resets all metrics and stored crash reports.
    pub fn reset_metrics(&self, _server: Option<&str>) -> Result<()> {
        let mut reg = CrashTriageRegistry::load(&self.paths)?;
        reg.reset_metrics(&self.paths)?;

        self.total_triaged.store(0, Ordering::Relaxed);
        self.critical_triaged.store(0, Ordering::Relaxed);
        self.active_leaks.store(0, Ordering::Relaxed);
        self.total_leaked_bytes.store(0, Ordering::Relaxed);
        self.last_parse_duration_ms.store(0, Ordering::Relaxed);

        Ok(())
    }

    /// Scans registered servers for untriaged `core*` and `hs_err*.log` files.
    pub fn auto_scan_servers(&self) -> Result<Vec<CrashTriageReport>> {
        let mut new_reports = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.paths.servers_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let server_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                    if let Ok(files) = fs::read_dir(&path) {
                        for file in files.flatten() {
                            let fname = file.file_name().to_string_lossy().to_string();
                            if fname.starts_with("hs_err_pid") && fname.ends_with(".log") {
                                if let Ok(report) = self.triage_file(Some(server_name), file.path().to_str().unwrap_or_default()) {
                                    new_reports.push(report);
                                }
                            } else if fname == "core" || fname.starts_with("core.") {
                                if let Ok(report) = self.triage_file(Some(server_name), file.path().to_str().unwrap_or_default()) {
                                    new_reports.push(report);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(new_reports)
    }

    /// Generates Prometheus exposition metrics for crash triaging.
    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        out.push_str("# HELP craft_crash_triaged_total Total number of crash reports triaged\n");
        out.push_str("# TYPE craft_crash_triaged_total counter\n");
        out.push_str(&format!(
            "craft_crash_triaged_total {}\n",
            self.total_triaged.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP craft_crash_critical_total Total number of critical severity incidents\n");
        out.push_str("# TYPE craft_crash_critical_total counter\n");
        out.push_str(&format!(
            "craft_crash_critical_total {}\n",
            self.critical_triaged.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP craft_crash_leaks_detected Number of active memory leak candidates\n");
        out.push_str("# TYPE craft_crash_leaks_detected gauge\n");
        out.push_str(&format!(
            "craft_crash_leaks_detected {}\n",
            self.active_leaks.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP craft_crash_leaked_bytes_total Total estimated leaked memory in bytes\n");
        out.push_str("# TYPE craft_crash_leaked_bytes_total gauge\n");
        out.push_str(&format!(
            "craft_crash_leaked_bytes_total {}\n",
            self.total_leaked_bytes.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP craft_crash_parse_duration_ms Duration of the last triage parse in milliseconds\n");
        out.push_str("# TYPE craft_crash_parse_duration_ms gauge\n");
        out.push_str(&format!(
            "craft_crash_parse_duration_ms {}\n",
            self.last_parse_duration_ms.load(Ordering::Relaxed)
        ));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_net::crash_triage::{ElfCoreDumpParser, JvmHsErrParser, SIGSEGV};
    use tempfile::tempdir;

    #[test]
    fn test_crash_triage_service_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = CrashTriageService::new(paths.clone());

        // 1. Initial status
        let status = service.get_status(None).unwrap();
        assert_eq!(status.total_reports, 0);

        // 2. Write synthetic core dump to temp file
        let core_bytes = ElfCoreDumpParser::build_synthetic_elf_core(SIGSEGV, 54321, 0x00007f1234560000, 0x0);
        let core_file = temp.path().join("core.54321");
        fs::write(&core_file, core_bytes).unwrap();

        // 3. Triage file
        let report = service.triage_file(Some("survival"), core_file.to_str().unwrap()).unwrap();
        assert_eq!(report.severity, CrashSeverity::Critical);
        assert_eq!(report.server.as_deref(), Some("survival"));

        // 4. Verify updated status
        let updated_status = service.get_status(None).unwrap();
        assert_eq!(updated_status.total_reports, 1);
        assert_eq!(updated_status.critical_reports, 1);

        // 5. Write and triage synthetic hs_err
        let hs_err = JvmHsErrParser::build_synthetic_hs_err("SIGSEGV", "C  [libfoo.so+0x100]", "0x0");
        let hs_file = temp.path().join("hs_err_pid123.log");
        fs::write(&hs_file, hs_err).unwrap();

        let report2 = service.triage_file(Some("lobby"), hs_file.to_str().unwrap()).unwrap();
        assert_eq!(report2.severity, CrashSeverity::Critical);

        // 6. List reports
        let reports = service.list_reports(None, None).unwrap();
        assert_eq!(reports.len(), 2);

        // 7. Prometheus metrics
        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_crash_triaged_total 2"));
        assert!(metrics.contains("craft_crash_critical_total 2"));

        // 8. Reset metrics
        service.reset_metrics(None).unwrap();
        let reset_status = service.get_status(None).unwrap();
        assert_eq!(reset_status.total_reports, 0);
    }
}
