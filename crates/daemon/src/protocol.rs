use crate::circuit_breaker::CircuitBreakerInfo;
use crate::scheduler::BackupScheduleInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    Ping,
    StartServer { path: PathBuf },
    StopServer { path: PathBuf, force: bool },
    GetRunning,
    AttachConsole { path: PathBuf },
    SendInput { path: PathBuf, input: String },
    DetachConsole { path: PathBuf },
    ShutdownDaemon,
    GetCircuitBreakers,
    ResetCircuitBreaker { path: PathBuf },
    GetBackupSchedules,
    HibernateServer { server_name: String },
    WakeServer { server_name: String },
    GetAutoscaleStatus,
    GetIntelligenceStatus { server: Option<String> },
    TriggerDiagnosticRun { server: String, duration_secs: u64 },
    ExecuteRemediation { server: String, action: craft_core::RemediationAction, dry_run: bool },
    UpdateIntelligencePolicy { server: String, policy: craft_core::IntelligencePolicy },
    GetEdgeMeshStatus,
    RegisterEdgeNode { node: craft_core::EdgeNode },
    RemoveEdgeNode { name: String },
    TriggerEdgeHandoff { handoff: craft_core::PlayerSessionHandoff },
    ConsumeEdgeHandoff { token: String },
    BroadcastEdgeChat { envelope: craft_core::CrossRegionChatEnvelope },
    ApplyLatencyPlaybook { server_name: String, preset: String },
    GetTickProfile { server_name: String },
    GetPacketStats { server_name: String },
    GetLatencyHistogram { server_name: String },
    StartClusterRollout { plan: craft_core::RolloutPlan },
    GetClusterRolloutStatus { cluster: String },
    AbortClusterRollout { rollout_id: String, reason: String },
    GetFleetHealth { cluster: String },
    ExecuteFleetHeal { cluster: String, action: craft_core::FleetHealingAction },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleServerStatus {
    pub server_name: String,
    pub enabled: bool,
    pub is_sleeping: bool,
    pub idle_timeout_mins: u64,
    pub idle_seconds: u64,
    pub player_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Pong,
    Success { message: String },
    AlreadyRunning { path: PathBuf },
    NotRunning { path: PathBuf },
    RunningList { paths: Vec<PathBuf> },
    LogBacklog { path: PathBuf, data: String },
    LogChunk { path: PathBuf, data: String },
    CircuitBreakersList { items: Vec<CircuitBreakerInfo> },
    BackupSchedulesList { items: Vec<BackupScheduleInfo> },
    AutoscaleStatusList { items: Vec<AutoscaleServerStatus> },
    IntelligenceReports { items: Vec<craft_core::DiagnosticReport> },
    DiagnosticRunCompleted { report: craft_core::DiagnosticReport, markdown: String },
    RemediationResult { message: String },
    EdgeMeshStatus { nodes: Vec<craft_core::EdgeNode>, backbone: Vec<craft_core::BackboneCondition> },
    EdgeHandoffResult { success: bool, message: String, handoff: Option<craft_core::PlayerSessionHandoff> },
    EdgeChatBroadcastResult { delivered_nodes: usize },
    LatencyPlaybookApplied { server_name: String, view_distance: u32, simulation_distance: u32, message: String },
    TickProfile { summary: craft_net::TickProfileSummary, sparkline: String },
    PacketStats { summary: craft_net::PacketRateSummary },
    LatencyHistogram { histogram: craft_net::LatencyHistogram, chart_lines: Vec<String> },
    ClusterRolloutStarted { rollout_id: String },
    ClusterRolloutStatus { record: Option<craft_core::RolloutRecord> },
    ClusterRolloutAborted { message: String },
    FleetHealth { status: craft_core::FleetHealthStatus },
    FleetHealResult { message: String },
    Error { error: String },
}
