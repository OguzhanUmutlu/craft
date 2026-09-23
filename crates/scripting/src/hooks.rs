use crate::config::CustomServerConfig;
use crate::engine::LuaEngine;
use craft_core::{CraftPaths, Result, ServersRegistry};
use mlua::LuaSerdeExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LifecycleEvent {
    ServerStart,
    ServerStop,
    ServerCrash,
    BackupStart,
    BackupComplete,
    CircuitTrip,
    StorageLow,
    AnomalyDetected,
    RolloutStart,
    RolloutCanaryPromoted,
    RolloutRollback,
    RolloutComplete,
    FleetNodeHealed,
    IncidentDetected,
    LogAlertTriggered,
    WorkloadSurgePredicted,
    CostOptimizationApplied,
    ProactiveWakeTriggered,
    ModpackBuildCompleted,
    ModpackDeltaPublished,
    ClientSyncRequested,
}

impl LifecycleEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ServerStart => "on_server_start",
            Self::ServerStop => "on_server_stop",
            Self::ServerCrash => "on_server_crash",
            Self::BackupStart => "on_backup_start",
            Self::BackupComplete => "on_backup_complete",
            Self::CircuitTrip => "on_circuit_trip",
            Self::StorageLow => "on_storage_low",
            Self::AnomalyDetected => "on_anomaly_detected",
            Self::RolloutStart => "on_rollout_start",
            Self::RolloutCanaryPromoted => "on_rollout_canary_promoted",
            Self::RolloutRollback => "on_rollout_rollback",
            Self::RolloutComplete => "on_rollout_complete",
            Self::FleetNodeHealed => "on_fleet_node_healed",
            Self::IncidentDetected => "on_incident_detected",
            Self::LogAlertTriggered => "on_log_alert_triggered",
            Self::WorkloadSurgePredicted => "on_workload_surge_predicted",
            Self::CostOptimizationApplied => "on_cost_optimization_applied",
            Self::ProactiveWakeTriggered => "on_proactive_wake_triggered",
            Self::ModpackBuildCompleted => "on_modpack_build_completed",
            Self::ModpackDeltaPublished => "on_modpack_delta_published",
            Self::ClientSyncRequested => "on_client_sync_requested",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.trim().to_lowercase().replace('-', "_");
        match normalized.as_str() {
            "on_server_start" | "server_start" | "serverstart" | "start" => Some(Self::ServerStart),
            "on_server_stop" | "server_stop" | "serverstop" | "stop" => Some(Self::ServerStop),
            "on_server_crash" | "server_crash" | "servercrash" | "crash" => Some(Self::ServerCrash),
            "on_backup_start" | "backup_start" | "backupstart" => Some(Self::BackupStart),
            "on_backup_complete" | "backup_complete" | "backupcomplete" | "backup" => Some(Self::BackupComplete),
            "on_circuit_trip" | "circuit_trip" | "circuittrip" | "circuit" => Some(Self::CircuitTrip),
            "on_storage_low" | "storage_low" | "storagelow" | "storage" => Some(Self::StorageLow),
            "on_anomaly_detected" | "anomaly_detected" | "anomalydetected" | "anomaly" => Some(Self::AnomalyDetected),
            "on_rollout_start" | "rollout_start" | "rolloutstart" => Some(Self::RolloutStart),
            "on_rollout_canary_promoted" | "rollout_canary_promoted" | "canary_promoted" => Some(Self::RolloutCanaryPromoted),
            "on_rollout_rollback" | "rollout_rollback" | "rollback" => Some(Self::RolloutRollback),
            "on_rollout_complete" | "rollout_complete" | "rolloutcomplete" => Some(Self::RolloutComplete),
            "on_fleet_node_healed" | "fleet_node_healed" | "node_healed" | "heal" => Some(Self::FleetNodeHealed),
            "on_incident_detected" | "incident_detected" | "incident" => Some(Self::IncidentDetected),
            "on_log_alert_triggered" | "log_alert_triggered" | "log_alert" => Some(Self::LogAlertTriggered),
            "on_workload_surge_predicted" | "workload_surge_predicted" | "surge_predicted" | "surge" => Some(Self::WorkloadSurgePredicted),
            "on_cost_optimization_applied" | "cost_optimization_applied" | "cost_optimization" | "cost" => Some(Self::CostOptimizationApplied),
            "on_proactive_wake_triggered" | "proactive_wake_triggered" | "proactive_wake" | "wake" => Some(Self::ProactiveWakeTriggered),
            "on_modpack_build_completed" | "modpack_build_completed" | "modpack_build" => Some(Self::ModpackBuildCompleted),
            "on_modpack_delta_published" | "modpack_delta_published" | "delta_published" => Some(Self::ModpackDeltaPublished),
            "on_client_sync_requested" | "client_sync_requested" | "client_sync" => Some(Self::ClientSyncRequested),
            _ => None,
        }
    }

    pub fn all() -> &'static [LifecycleEvent] {
        &[
            Self::ServerStart,
            Self::ServerStop,
            Self::ServerCrash,
            Self::BackupStart,
            Self::BackupComplete,
            Self::CircuitTrip,
            Self::StorageLow,
            Self::AnomalyDetected,
            Self::RolloutStart,
            Self::RolloutCanaryPromoted,
            Self::RolloutRollback,
            Self::RolloutComplete,
            Self::FleetNodeHealed,
            Self::IncidentDetected,
            Self::LogAlertTriggered,
            Self::WorkloadSurgePredicted,
            Self::CostOptimizationApplied,
            Self::ProactiveWakeTriggered,
            Self::ModpackBuildCompleted,
            Self::ModpackDeltaPublished,
            Self::ClientSyncRequested,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookContext {
    pub event: String,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crashes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healed_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit_exception: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_players: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_savings_estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizon_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modpack_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modpack_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub savings_percent: Option<f64>,
}

impl HookContext {
    pub fn new(event: LifecycleEvent) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            event: event.as_str().to_string(),
            timestamp,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub name: String,
    pub event: String,
    pub scope: String,
    pub path: PathBuf,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub hook_name: String,
    pub path: PathBuf,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

pub struct HookBus;

impl HookBus {
    pub fn hooks_dir(paths: &CraftPaths) -> PathBuf {
        paths.home.join("hooks")
    }

    pub fn ensure_hooks_dir(paths: &CraftPaths) -> Result<PathBuf> {
        let dir = Self::hooks_dir(paths);
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    pub fn discover_hooks(paths: &CraftPaths, server_filter: Option<&str>) -> Vec<HookDefinition> {
        let mut list = Vec::new();
        let global_dir = Self::hooks_dir(paths);

        if global_dir.exists() {
            // Check for multi-event hooks.lua
            let unified_file = global_dir.join("hooks.lua");
            list.push(HookDefinition {
                name: "hooks.lua".to_string(),
                event: "*".to_string(),
                scope: "global".to_string(),
                active: unified_file.is_file(),
                path: unified_file,
            });

            // Check for individual event files
            for event in LifecycleEvent::all() {
                let filename = format!("{}.lua", event.as_str());
                let script_path = global_dir.join(&filename);
                list.push(HookDefinition {
                    name: filename,
                    event: event.as_str().to_string(),
                    scope: "global".to_string(),
                    active: script_path.is_file(),
                    path: script_path,
                });
            }
        }

        // Discover per-server hooks
        if let Ok(reg) = ServersRegistry::load(paths) {
            for server in reg.servers {
                if let Some(filter) = server_filter {
                    if server.name != filter {
                        continue;
                    }
                }
                let server_hooks = server.path.join("hooks.lua");
                list.push(HookDefinition {
                    name: format!("{}/hooks.lua", server.name),
                    event: "*".to_string(),
                    scope: format!("server:{}", server.name),
                    active: server_hooks.is_file(),
                    path: server_hooks,
                });
            }
        }

        list
    }

    pub fn dispatch(
        paths: &CraftPaths,
        event: LifecycleEvent,
        context: &HookContext,
        timeout_secs: u64,
    ) -> Vec<HookResult> {
        let mut results = Vec::new();
        let global_dir = Self::hooks_dir(paths);
        let event_fn_name = event.as_str();

        // 1. Dispatch global multi-event hooks.lua
        let unified_global = global_dir.join("hooks.lua");
        if unified_global.is_file() {
            let res = Self::execute_hook_script(
                paths,
                &unified_global,
                Some(event_fn_name),
                context,
                timeout_secs,
            );
            results.push(res);
        }

        // 2. Dispatch global event-specific script (e.g. on_server_crash.lua)
        let specific_global = global_dir.join(format!("{}.lua", event_fn_name));
        if specific_global.is_file() {
            let res = Self::execute_hook_script(
                paths,
                &specific_global,
                None,
                context,
                timeout_secs,
            );
            results.push(res);
        }

        // 3. Dispatch per-server hooks.lua if context contains server info
        if let Some(ref server_path_str) = context.server_path {
            let server_dir = PathBuf::from(server_path_str);
            let server_hooks = server_dir.join("hooks.lua");
            if server_hooks.is_file() {
                let res = Self::execute_hook_script(
                    paths,
                    &server_hooks,
                    Some(event_fn_name),
                    context,
                    timeout_secs,
                );
                results.push(res);
            }
        }

        // Log results to ~/.craft/logs/hooks.log
        Self::log_results(paths, event, context, &results);

        results
    }

    pub fn dispatch_async(
        paths: CraftPaths,
        event: LifecycleEvent,
        context: HookContext,
        timeout_secs: u64,
    ) {
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                Self::dispatch(&paths, event, &context, timeout_secs);
            })
            .await;
        });
    }

    fn execute_hook_script(
        paths: &CraftPaths,
        script_path: &Path,
        func_name_opt: Option<&str>,
        context: &HookContext,
        timeout_secs: u64,
    ) -> HookResult {
        let start = Instant::now();
        let hook_name = script_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown_hook".to_string());

        let res = (|| -> Result<()> {
            let mut server_dir = None;
            let mut custom_cfg = None;
            if let Some(ref sp) = context.server_path {
                let p = PathBuf::from(sp);
                custom_cfg = CustomServerConfig::load_from_dir(&p).ok().flatten();
                server_dir = Some(p);
            }

            let engine = LuaEngine::new_full(
                Some(paths),
                server_dir.as_deref(),
                custom_cfg.as_ref(),
                context.pid,
            )?;

            let lua = engine.lua();
            let ctx_val = lua
                .to_value(context)
                .map_err(|e| craft_core::CraftError::Other(format!("Failed to serialize ctx: {}", e)))?;
            lua.globals()
                .set("ctx", ctx_val)
                .map_err(|e| craft_core::CraftError::Other(e.to_string()))?;

            let content = fs::read_to_string(script_path)
                .map_err(|e| craft_core::CraftError::Other(format!("Failed to read script: {}", e)))?;

            let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs.max(1));
            let _ = lua.set_hook(mlua::HookTriggers::default().every_nth_instruction(5000), move |_, _| {
                if Instant::now() >= deadline {
                    Err(mlua::Error::RuntimeError(format!(
                        "Hook execution timed out after {}s",
                        timeout_secs
                    )))
                } else {
                    Ok(mlua::VmState::Continue)
                }
            });

            lua.load(&content)
                .set_name(script_path.to_string_lossy())
                .exec()
                .map_err(|e| craft_core::CraftError::Other(format!("Lua execution error: {}", e)))?;

            if let Some(func_name) = func_name_opt {
                let globals = lua.globals();
                if let Ok(func) = globals.get::<mlua::Function>(func_name) {
                    let ctx_arg = globals
                        .get::<mlua::Value>("ctx")
                        .map_err(|e| craft_core::CraftError::Other(e.to_string()))?;
                    func.call::<()>(ctx_arg)
                        .map_err(|e| craft_core::CraftError::Other(format!("Function {} error: {}", func_name, e)))?;
                }
            }

            lua.remove_hook();

            Ok(())
        })();

        let duration_ms = start.elapsed().as_millis() as u64;
        match res {
            Ok(()) => HookResult {
                hook_name,
                path: script_path.to_path_buf(),
                success: true,
                duration_ms,
                error: None,
            },
            Err(e) => {
                warn!("Hook '{}' failed: {}", script_path.display(), e);
                HookResult {
                    hook_name,
                    path: script_path.to_path_buf(),
                    success: false,
                    duration_ms,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    fn log_results(
        paths: &CraftPaths,
        event: LifecycleEvent,
        context: &HookContext,
        results: &[HookResult],
    ) {
        if results.is_empty() {
            return;
        }

        let logs_dir = paths.home.join("logs");
        let _ = fs::create_dir_all(&logs_dir);
        let log_file = logs_dir.join("hooks.log");

        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_file) {
            let ts = chrono::Utc::now().to_rfc3339();
            let server_label = context
                .server_name
                .as_deref()
                .unwrap_or(context.server_path.as_deref().unwrap_or("global"));

            for r in results {
                let status = if r.success { "[OK]" } else { "[FAIL]" };
                let err_str = r
                    .error
                    .as_deref()
                    .map(|e| format!(" error=\"{}\"", e))
                    .unwrap_or_default();
                let _ = writeln!(
                    f,
                    "{} {} event={} server={} hook={} duration_ms={}{}",
                    ts, status, event.as_str(), server_label, r.hook_name, r.duration_ms, err_str
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rollout_lifecycle_events_parsing() {
        assert_eq!(LifecycleEvent::from_name("on_rollout_start"), Some(LifecycleEvent::RolloutStart));
        assert_eq!(LifecycleEvent::from_name("canary_promoted"), Some(LifecycleEvent::RolloutCanaryPromoted));
        assert_eq!(LifecycleEvent::from_name("rollback"), Some(LifecycleEvent::RolloutRollback));
        assert_eq!(LifecycleEvent::from_name("rollout_complete"), Some(LifecycleEvent::RolloutComplete));
        assert_eq!(LifecycleEvent::from_name("node_healed"), Some(LifecycleEvent::FleetNodeHealed));

        let mut ctx = HookContext::new(LifecycleEvent::RolloutStart);
        ctx.cluster_name = Some("survival-cluster".to_string());
        ctx.rollout_id = Some("rollout-123".to_string());
        ctx.target_version = Some("1.21.1".to_string());

        assert_eq!(ctx.event, "on_rollout_start");
        assert_eq!(ctx.cluster_name.as_deref(), Some("survival-cluster"));
        assert_eq!(ctx.rollout_id.as_deref(), Some("rollout-123"));
    }

    #[test]
    fn test_incident_lifecycle_events_and_context() {
        assert_eq!(LifecycleEvent::from_name("on_incident_detected"), Some(LifecycleEvent::IncidentDetected));
        assert_eq!(LifecycleEvent::from_name("incident"), Some(LifecycleEvent::IncidentDetected));
        assert_eq!(LifecycleEvent::from_name("on_log_alert_triggered"), Some(LifecycleEvent::LogAlertTriggered));
        assert_eq!(LifecycleEvent::from_name("log_alert"), Some(LifecycleEvent::LogAlertTriggered));

        let mut ctx = HookContext::new(LifecycleEvent::IncidentDetected);
        ctx.server_name = Some("lobby-01".to_string());
        ctx.incident_id = Some("inc-lobby-01-20260923".to_string());
        ctx.culprit_exception = Some("java.lang.NullPointerException".to_string());
        ctx.log_level = Some("FATAL".to_string());

        assert_eq!(ctx.event, "on_incident_detected");
        assert_eq!(ctx.incident_id.as_deref(), Some("inc-lobby-01-20260923"));
        assert_eq!(ctx.culprit_exception.as_deref(), Some("java.lang.NullPointerException"));
        assert_eq!(ctx.log_level.as_deref(), Some("FATAL"));
    }

    #[test]
    fn test_workload_lifecycle_events_and_context() {
        assert_eq!(LifecycleEvent::from_name("on_workload_surge_predicted"), Some(LifecycleEvent::WorkloadSurgePredicted));
        assert_eq!(LifecycleEvent::from_name("surge"), Some(LifecycleEvent::WorkloadSurgePredicted));
        assert_eq!(LifecycleEvent::from_name("on_cost_optimization_applied"), Some(LifecycleEvent::CostOptimizationApplied));
        assert_eq!(LifecycleEvent::from_name("cost"), Some(LifecycleEvent::CostOptimizationApplied));
        assert_eq!(LifecycleEvent::from_name("on_proactive_wake_triggered"), Some(LifecycleEvent::ProactiveWakeTriggered));
        assert_eq!(LifecycleEvent::from_name("wake"), Some(LifecycleEvent::ProactiveWakeTriggered));

        let mut ctx = HookContext::new(LifecycleEvent::WorkloadSurgePredicted);
        ctx.server_name = Some("survival-eu".to_string());
        ctx.predicted_players = Some(48.5);
        ctx.cost_savings_estimate = Some(15.75);
        ctx.horizon_minutes = Some(20);
        ctx.scaling_action = Some("ProactiveWake".to_string());

        assert_eq!(ctx.event, "on_workload_surge_predicted");
        assert_eq!(ctx.server_name.as_deref(), Some("survival-eu"));
        assert_eq!(ctx.predicted_players, Some(48.5));
        assert_eq!(ctx.cost_savings_estimate, Some(15.75));
        assert_eq!(ctx.horizon_minutes, Some(20));
        assert_eq!(ctx.scaling_action.as_deref(), Some("ProactiveWake"));
    }
}
