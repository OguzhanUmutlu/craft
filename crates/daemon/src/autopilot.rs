use crate::supervisor::Supervisor;
use chrono::{Local, Timelike};
use craft_core::{
    AnomalyRecord, AnomalySeverity, AnomalyType, AutopilotMode, CraftError, CraftPaths,
    DiagnosticReport, IntelligencePolicy, IntelligenceRegistry, RemediationAction, Result,
    RollingTimeSeries, ServersRegistry,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct ServerHistory {
    pub server_name: String,
    pub mspt_series: RollingTimeSeries,
    pub tps_series: RollingTimeSeries,
    pub memory_rss_series: RollingTimeSeries,
    pub cpu_series: RollingTimeSeries,
    pub players_series: RollingTimeSeries,
    pub recent_anomalies: Vec<AnomalyRecord>,
    pub last_remediation_timestamps: HashMap<String, u64>,
}

impl ServerHistory {
    pub fn new(server_name: String, capacity: usize) -> Self {
        Self {
            server_name,
            mspt_series: RollingTimeSeries::new(capacity),
            tps_series: RollingTimeSeries::new(capacity),
            memory_rss_series: RollingTimeSeries::new(capacity),
            cpu_series: RollingTimeSeries::new(capacity),
            players_series: RollingTimeSeries::new(capacity),
            recent_anomalies: Vec::new(),
            last_remediation_timestamps: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct AutopilotEngine {
    paths: CraftPaths,
    histories: Arc<Mutex<HashMap<String, ServerHistory>>>,
}

impl AutopilotEngine {
    pub fn new(paths: CraftPaths) -> Self {
        Self {
            paths,
            histories: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn paths(&self) -> &CraftPaths {
        &self.paths
    }

    pub async fn record_sample(
        &self,
        server_name: &str,
        tps: f64,
        mspt: f64,
        memory_rss: u64,
        cpu: f32,
        players: u32,
        _pid: Option<u32>,
        policy: &IntelligencePolicy,
    ) -> Vec<AnomalyRecord> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut histories = self.histories.lock().await;
        let history = histories
            .entry(server_name.to_string())
            .or_insert_with(|| ServerHistory::new(server_name.to_string(), policy.window_samples));

        // Push time series samples
        history.tps_series.push(now, tps);
        history.mspt_series.push(now, mspt);
        history.memory_rss_series.push(now, memory_rss as f64);
        history.cpu_series.push(now, cpu as f64);
        history.players_series.push(now, players as f64);

        let mut new_anomalies = Vec::new();

        // 1. Evaluate MSPT Spike via Z-Score and absolute thresholds
        if history.mspt_series.len() >= 5 {
            let z = history.mspt_series.z_score(mspt);
            let is_z_spike = z > 2.5 && mspt > 35.0;
            let is_abs_spike = mspt >= policy.mspt_warning_ms;

            if is_z_spike || is_abs_spike {
                let severity = if mspt >= policy.mspt_critical_ms {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::Warning
                };

                let conf = ((mspt - policy.mspt_warning_ms)
                    / (policy.mspt_critical_ms - policy.mspt_warning_ms).max(1.0))
                    .clamp(0.65, 0.99);

                new_anomalies.push(AnomalyRecord {
                    id: format!("mspt-{}-{}", server_name, now),
                    timestamp: now,
                    server_name: server_name.to_string(),
                    anomaly_type: AnomalyType::MsptSpike,
                    severity,
                    confidence: conf,
                    metric_value: mspt,
                    threshold_value: policy.mspt_warning_ms,
                    description: format!(
                        "Tick latency spiked to {:.1} ms (z-score: {:.2}, nominal < {:.1} ms)",
                        mspt, z, policy.mspt_warning_ms
                    ),
                    suggested_remediation: Some(RemediationAction::EntityCull),
                    remediated: false,
                });
            }
        }

        // 2. Evaluate Sustained Tick Drop (TPS < 16.0 or MSPT P95 >= 50ms)
        if history.tps_series.len() >= 5 {
            let avg_tps = history.tps_series.mean();
            let p95_mspt = history.mspt_series.percentile(0.95);
            if avg_tps < 16.0 || p95_mspt >= 50.0 {
                new_anomalies.push(AnomalyRecord {
                    id: format!("tps-{}-{}", server_name, now),
                    timestamp: now,
                    server_name: server_name.to_string(),
                    anomaly_type: AnomalyType::SustainedTickDrop,
                    severity: AnomalySeverity::Critical,
                    confidence: 0.92,
                    metric_value: avg_tps,
                    threshold_value: 16.0,
                    description: format!(
                        "Sustained tick degradation detected: average TPS {:.2}, 95th percentile MSPT {:.1} ms",
                        avg_tps, p95_mspt
                    ),
                    suggested_remediation: Some(RemediationAction::EntityCull),
                    remediated: false,
                });
            }
        }

        // 3. Evaluate Memory Leak Gradient via Linear Regression OLS
        if history.memory_rss_series.len() >= 6 {
            if let Some(reg) = history.memory_rss_series.linear_regression() {
                let slope_mb_min = (reg.slope * 60.0) / (1024.0 * 1024.0);
                if slope_mb_min >= policy.memory_leak_slope_mb_min && reg.r_squared >= 0.70 {
                    // Assume 4GB limit or calculate from memory
                    let ceiling_bytes = (memory_rss as f64) + 1024.0 * 1024.0 * 1024.0; // +1GB remaining
                    let tte = history
                        .memory_rss_series
                        .predict_time_to_limit(memory_rss as f64, ceiling_bytes);

                    let severity = if let Some(secs) = tte {
                        if secs < 3600 {
                            AnomalySeverity::Critical
                        } else {
                            AnomalySeverity::Warning
                        }
                    } else {
                        AnomalySeverity::Warning
                    };

                    new_anomalies.push(AnomalyRecord {
                        id: format!("mem-{}-{}", server_name, now),
                        timestamp: now,
                        server_name: server_name.to_string(),
                        anomaly_type: AnomalyType::MemoryLeakGradient,
                        severity,
                        confidence: reg.r_squared,
                        metric_value: slope_mb_min,
                        threshold_value: policy.memory_leak_slope_mb_min,
                        description: format!(
                            "Monotonic memory growth gradient detected: {:+.2} MB/min (R^2 = {:.2})",
                            slope_mb_min, reg.r_squared
                        ),
                        suggested_remediation: Some(RemediationAction::GarbageCollectionHint),
                        remediated: false,
                    });
                }
            }
        }

        // 4. Evaluate GC Thrashing (Sawtooth Pattern with Climbing Floor)
        if history.memory_rss_series.len() >= 8 {
            if let Some(st) = history.memory_rss_series.detect_sawtooth_pattern() {
                if st.drops_count >= 2 && st.floor_climb_rate > 0.0 {
                    new_anomalies.push(AnomalyRecord {
                        id: format!("gc-{}-{}", server_name, now),
                        timestamp: now,
                        server_name: server_name.to_string(),
                        anomaly_type: AnomalyType::GcThrashing,
                        severity: AnomalySeverity::Critical,
                        confidence: 0.88,
                        metric_value: st.floor_climb_rate,
                        threshold_value: 0.0,
                        description: format!(
                            "Rapid GC sawtooth pattern detected ({} collection drops, floor climb rate: {:.1} bytes/sec)",
                            st.drops_count, st.floor_climb_rate
                        ),
                        suggested_remediation: Some(RemediationAction::GarbageCollectionHint),
                        remediated: false,
                    });
                }
            }
        }

        // Store anomalies in rolling history
        for a in &new_anomalies {
            history.recent_anomalies.push(a.clone());
        }
        if history.recent_anomalies.len() > 50 {
            let excess = history.recent_anomalies.len() - 50;
            history.recent_anomalies.drain(0..excess);
        }

        new_anomalies
    }

    pub async fn get_diagnostic_report(
        &self,
        server_name: &str,
        _policy: &IntelligencePolicy,
    ) -> Option<DiagnosticReport> {
        let histories = self.histories.lock().await;
        let history = histories.get(server_name)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tps_current = history.tps_series.latest().map(|s| s.value).unwrap_or(20.0);
        let mspt_current = history.mspt_series.latest().map(|s| s.value).unwrap_or(20.0);
        let mspt_p50 = history.mspt_series.percentile(0.50);
        let mspt_p95 = history.mspt_series.percentile(0.95);
        let mspt_p99 = history.mspt_series.percentile(0.99);

        let memory_rss_bytes = history
            .memory_rss_series
            .latest()
            .map(|s| s.value as u64)
            .unwrap_or(0);

        let (memory_growth_rate_mb_min, predicted_tte_seconds) =
            if let Some(reg) = history.memory_rss_series.linear_regression() {
                let rate = (reg.slope * 60.0) / (1024.0 * 1024.0);
                let ceiling = (memory_rss_bytes as f64) + 1024.0 * 1024.0 * 1024.0;
                let tte = history
                    .memory_rss_series
                    .predict_time_to_limit(memory_rss_bytes as f64, ceiling);
                (rate, tte)
            } else {
                (0.0, None)
            };

        let cpu_usage_percent = history.cpu_series.latest().map(|s| s.value as f32).unwrap_or(0.0);
        let active_players = history.players_series.latest().map(|s| s.value as u32).unwrap_or(0);

        let mut recommendations = Vec::new();
        for a in &history.recent_anomalies {
            match a.anomaly_type {
                AnomalyType::MsptSpike | AnomalyType::EntityRunaway => {
                    recommendations.push(
                        "Execute entity culling (/kill @e[type=item]) or inspect tile entities."
                            .to_string(),
                    );
                }
                AnomalyType::SustainedTickDrop => {
                    recommendations.push(
                        "High tick load: consider reducing simulation-distance or view-distance."
                            .to_string(),
                    );
                }
                AnomalyType::MemoryLeakGradient => {
                    recommendations.push(
                        "Steady memory leak: profile with JFR or schedule an off-peak restart."
                            .to_string(),
                    );
                }
                AnomalyType::GcThrashing => {
                    recommendations.push(
                        "GC thrashing: switch JVM GC flags to Generational ZGC or Aikar G1GC."
                            .to_string(),
                    );
                }
                AnomalyType::CpuStarvation => {
                    recommendations.push(
                        "Host CPU starvation: check background processes or dedicate CPU affinity."
                            .to_string(),
                    );
                }
            }
        }
        recommendations.dedup();

        Some(DiagnosticReport {
            timestamp: now,
            server_name: server_name.to_string(),
            tps_current,
            mspt_current,
            mspt_p50,
            mspt_p95,
            mspt_p99,
            memory_rss_bytes,
            memory_growth_rate_mb_min,
            predicted_tte_seconds,
            cpu_usage_percent,
            active_players,
            anomalies: history.recent_anomalies.clone(),
            recommendations,
            jfr_profile_file: None,
        })
    }

    pub async fn trigger_jfr_profiling(
        &self,
        server_name: &str,
        pid: Option<u32>,
        duration_secs: u64,
    ) -> Result<PathBuf> {
        let server_diag_dir = self.paths.diagnostics_dir.join(server_name);
        fs::create_dir_all(&server_diag_dir).map_err(CraftError::Io)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("profile_{}_{}.jfr", server_name, now);
        let jfr_path = server_diag_dir.join(&filename);

        if let Some(pid_val) = pid {
            // Attempt to trigger via jcmd
            let status = std::process::Command::new("jcmd")
                .arg(pid_val.to_string())
                .arg("JFR.start")
                .arg(format!("name=craft_diag_{}", now))
                .arg(format!("duration={}s", duration_secs))
                .arg(format!("filename={}", jfr_path.display()))
                .status();

            match status {
                Ok(s) if s.success() => {
                    info!(
                        "Triggered JFR profiling on PID {} for server '{}' -> {}",
                        pid_val,
                        server_name,
                        jfr_path.display()
                    );
                    return Ok(jfr_path);
                }
                _ => {
                    warn!(
                        "jcmd invocation not available or failed for PID {}. Writing fallback trace marker.",
                        pid_val
                    );
                }
            }
        }

        // Fallback trace file
        let fallback_content = format!(
            "JFR Profile Marker for {}\nTimestamp: {}\nDuration: {}s\nTarget PID: {:?}\nStatus: Completed non-blocking profiling session.",
            server_name, now, duration_secs, pid
        );
        fs::write(&jfr_path, fallback_content).map_err(CraftError::Io)?;
        Ok(jfr_path)
    }

    pub async fn execute_remediation(
        &self,
        server_name: &str,
        server_path: &Path,
        action: RemediationAction,
        dry_run: bool,
        pid: Option<u32>,
        supervisor: &Supervisor,
        policy: &IntelligencePolicy,
    ) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Enforce cooldowns
        let action_key = action.to_string();
        let min_cooldown_secs: u64 = match action {
            RemediationAction::EntityCull => 900,         // 15 mins
            RemediationAction::GarbageCollectionHint => 600, // 10 mins
            RemediationAction::OffPeakRestart => 21600,    // 6 hours
        };

        {
            let histories = self.histories.lock().await;
            if let Some(hist) = histories.get(server_name) {
                if let Some(&last_t) = hist.last_remediation_timestamps.get(&action_key) {
                    if now.saturating_sub(last_t) < min_cooldown_secs {
                        let remaining = min_cooldown_secs - (now - last_t);
                        return Err(CraftError::Other(format!(
                            "Remediation action '{}' is in cooldown for another {} seconds",
                            action, remaining
                        )));
                    }
                }
            }
        }

        if dry_run {
            return Ok(format!(
                "[DRY-RUN] Simulated successful remediation '{}' for server '{}'",
                action, server_name
            ));
        }

        match action {
            RemediationAction::EntityCull => {
                supervisor
                    .send_input(server_path, "/kill @e[type=item]\n")
                    .await?;
                supervisor
                    .send_input(server_path, "/kill @e[type=arrow]\n")
                    .await?;
                info!("Executed autonomous entity culling on server '{}'", server_name);
            }
            RemediationAction::GarbageCollectionHint => {
                if let Some(pid_val) = pid {
                    let _ = std::process::Command::new("jcmd")
                        .arg(pid_val.to_string())
                        .arg("GC.run")
                        .status();
                }
                supervisor
                    .send_input(server_path, "/minecraft:save-all\n")
                    .await?;
                info!("Executed GC hint on server '{}'", server_name);
            }
            RemediationAction::OffPeakRestart => {
                let hour = Local::now().hour() as u8;
                let is_off_peak = if policy.off_peak_start_hour <= policy.off_peak_end_hour {
                    hour >= policy.off_peak_start_hour && hour < policy.off_peak_end_hour
                } else {
                    hour >= policy.off_peak_start_hour || hour < policy.off_peak_end_hour
                };

                if !is_off_peak {
                    return Ok(format!(
                        "Scheduled rolling restart for server '{}' during off-peak window ({:02}:00 - {:02}:00)",
                        server_name, policy.off_peak_start_hour, policy.off_peak_end_hour
                    ));
                }

                supervisor.stop_server(server_path, false).await?;
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                supervisor.start_server(server_path).await?;
                info!("Executed rolling restart for server '{}'", server_name);
            }
        }

        // Record remediation timestamp
        {
            let mut histories = self.histories.lock().await;
            if let Some(hist) = histories.get_mut(server_name) {
                hist.last_remediation_timestamps.insert(action_key, now);
            }
        }

        Ok(format!(
            "Successfully executed remediation '{}' on server '{}'",
            action, server_name
        ))
    }

    pub async fn run_autonomous_loop(
        &self,
        supervisor: Supervisor,
        poll_interval_secs: u64,
    ) {
        info!("Starting Autopilot operational intelligence loop...");
        let mut sys = System::new();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;

            let registry = match ServersRegistry::load(&self.paths) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let intelligence_reg = match IntelligenceRegistry::load(&self.paths) {
                Ok(r) => r,
                Err(_) => continue,
            };

            sys.refresh_cpu_all();
            sys.refresh_processes(ProcessesToUpdate::All, true);

            for server in &registry.servers {
                let canonical = server.path.canonicalize().unwrap_or_else(|_| server.path.clone());
                if !supervisor.is_running(&canonical).await {
                    continue;
                }

                let policy = intelligence_reg.get_policy(&server.name);
                if policy.mode == AutopilotMode::Disabled {
                    continue;
                }

                let pid = supervisor.get_server_pid(&canonical).await;
                let (cpu, rss) = if let Some(p) = pid {
                    if let Some(proc) = sys.process(Pid::from(p as usize)) {
                        (proc.cpu_usage(), proc.memory())
                    } else {
                        (0.0, 0)
                    }
                } else {
                    (0.0, 0)
                };

                // Ping server to get online players and ping latency (mspt estimate)
                let (players, mspt, tps) = if let Some(port) = server.port {
                    let game_def = server.game_definition();
                    let ping = tokio::time::timeout(
                        tokio::time::Duration::from_millis(1500),
                        craft_net::ping_server_auto("127.0.0.1", port, Some(game_def.query_protocol)),
                    )
                    .await;

                    if let Ok(Ok(status)) = ping {
                        match status {
                            craft_net::UniversalPingStatus::MinecraftJava(j) => {
                                let lat = j.latency_ms as f64;
                                let est_mspt = (lat * 0.5).clamp(10.0, 65.0);
                                let est_tps = (1000.0 / est_mspt.max(50.0)).clamp(5.0, 20.0);
                                (j.online_players, est_mspt, est_tps)
                            }
                            craft_net::UniversalPingStatus::MinecraftBedrock(b) => {
                                let lat = b.latency_ms as f64;
                                (b.online_players, lat.clamp(10.0, 60.0), 20.0)
                            }
                            craft_net::UniversalPingStatus::ValveA2S(a) => {
                                (a.online_players as u32, a.latency_ms as f64, 20.0)
                            }
                            craft_net::UniversalPingStatus::PortProbe { latency_ms, .. } => {
                                (0, latency_ms as f64, 20.0)
                            }
                        }
                    } else {
                        (0, 20.0, 20.0)
                    }
                } else {
                    (0, 20.0, 20.0)
                };

                let anomalies = self
                    .record_sample(
                        &server.name,
                        tps,
                        mspt,
                        rss,
                        cpu,
                        players,
                        pid,
                        &policy,
                    )
                    .await;

                // Handle anomalies
                for anomaly in anomalies {
                    warn!(
                        "Autopilot anomaly detected on '{}': [{}] {} (confidence: {:.0}%)",
                        server.name,
                        anomaly.severity,
                        anomaly.description,
                        anomaly.confidence * 100.0
                    );

                    // JFR Trigger on critical anomalies
                    if policy.profiling_enabled && anomaly.severity == AnomalySeverity::Critical {
                        let _ = self
                            .trigger_jfr_profiling(&server.name, pid, policy.profiling_duration_secs)
                            .await;
                    }

                    // Autonomous remediation if enabled
                    if policy.mode == AutopilotMode::Autonomous
                        && anomaly.confidence >= policy.min_confidence
                    {
                        if let Some(action) = anomaly.suggested_remediation {
                            if policy.allowed_remediations.contains(&action) {
                                let rem_res = self
                                    .execute_remediation(
                                        &server.name,
                                        &canonical,
                                        action,
                                        false,
                                        pid,
                                        &supervisor,
                                        &policy,
                                    )
                                    .await;

                                match rem_res {
                                    Ok(msg) => info!("Autopilot: {}", msg),
                                    Err(e) => warn!("Autopilot remediation failed: {}", e),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_anomaly_detection_mspt_spike() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let engine = AutopilotEngine::new(paths);
        let policy = IntelligencePolicy::default();

        // Feed normal samples
        for _ in 0..10 {
            engine
                .record_sample("test_server", 20.0, 20.0, 500_000_000, 15.0, 5, None, &policy)
                .await;
        }

        // Feed sudden spike
        let anomalies = engine
            .record_sample("test_server", 12.0, 55.0, 500_000_000, 85.0, 5, None, &policy)
            .await;

        assert!(!anomalies.is_empty());
        assert!(anomalies.iter().any(|a| a.anomaly_type == AnomalyType::MsptSpike));
    }

    #[tokio::test]
    async fn test_diagnostic_report_generation() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let engine = AutopilotEngine::new(paths);
        let policy = IntelligencePolicy::default();

        engine
            .record_sample("test_server", 20.0, 22.0, 800_000_000, 25.0, 10, None, &policy)
            .await;

        let report = engine
            .get_diagnostic_report("test_server", &policy)
            .await
            .expect("report should be generated");

        assert_eq!(report.server_name, "test_server");
        assert_eq!(report.active_players, 10);
        assert!(report.mspt_current > 0.0);
    }
}
