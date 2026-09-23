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
    SearchLogs { query: craft_core::LogQuery },
    GetIncidentForensics { server_name: String, incident_id: Option<String> },
    ListIncidents { server_name: Option<String> },
    IngestLogsNow { server_name: Option<String> },
    GetWorkloadForecast { server_name: String, horizon_hours: u32 },
    GetCostOptimizationReport { server_name: Option<String> },
    SetWorkloadPolicy { policy: craft_core::WorkloadPolicy },
    TriggerProactiveScalingNow { server_name: String },
    BuildModpack {
        name: String,
        version: String,
        loader: String,
        mc_version: String,
        base_path: String,
    },
    GenerateDelta {
        pack_name: String,
        source_version: String,
        target_version: String,
    },
    GetModpackStatus {
        pack_name: String,
    },
    GetModpackChunk {
        file_path: String,
        range_header: Option<String>,
    },
    GetSdnTopology,
    ApplySdnPolicy {
        policy: craft_core::MicrosegmentationPolicy,
    },
    RotateSdnKeys,
    GetPeerStatus {
        node_id: String,
    },
    GetRaftStatus,
    ProposeRaftCommand {
        payload: craft_core::RaftPayload,
    },
    AcquireDistributedLock {
        lock_name: String,
        holder_id: String,
        lease_secs: u64,
    },
    ReleaseDistributedLock {
        lock_name: String,
        holder_id: String,
    },
    StepDownRaftLeader,
    TransferRaftLeadership {
        target_node_id: String,
    },
    GetRaftLogs {
        limit: Option<usize>,
    },
    RaftGetMultiRaftStatus {
        group_id: Option<u64>,
    },
    RaftReconfigureMembership {
        group_id: u64,
        change_type: craft_core::MembershipChangeType,
        node: craft_core::RaftNode,
    },
    RaftTriggerCompaction {
        group_id: u64,
        force: bool,
    },
    RaftRoutePartitionKey {
        key: String,
    },
    RaftManagePartition {
        action: String,
        partition: Option<craft_core::MultiRaftPartition>,
        group_id: Option<u64>,
    },
    GetServerQuota {
        server: String,
    },
    SetServerQuota {
        limits: craft_core::ServerResourceLimit,
    },
    GetTenantQuota {
        tenant: String,
    },
    SetTenantQuota {
        quota: craft_core::TenantQuota,
    },
    ListQuotaUsage {
        tenant: Option<String>,
    },
    EnforceFairShareNow,
    GetTracingStatus,
    QueryTraces {
        service: Option<String>,
        name: Option<String>,
        min_duration_micros: Option<u64>,
        error_only: bool,
        limit: Option<usize>,
    },
    GetTraceDetails {
        trace_id: String,
    },
    ExportTracesNow {
        limit: Option<usize>,
    },
    SetTracingConfig {
        config: craft_core::TracingConfig,
    },
    GetAnvilStatus,
    InspectRegion {
        server_path: PathBuf,
        region_file: String,
    },
    PrefetchChunks {
        server_path: PathBuf,
        world: String,
        center_x: i32,
        center_z: i32,
        radius: u32,
    },
    BenchmarkAnvil {
        chunks: usize,
    },
    SetAnvilConfig {
        config: craft_core::AnvilConfig,
    },
    GetNumaStatus,
    PinServerCores {
        server_name: String,
        cpus: Vec<usize>,
        numa_node: Option<u32>,
        policy: craft_core::NumaPolicy,
    },
    SetNumaPolicy {
        server_name: String,
        policy: craft_core::NumaPolicy,
    },
    BenchmarkNumaMemory {
        node_id: u32,
        size_mb: usize,
    },
    GetDpdkStatus {
        bench_count: Option<usize>,
    },
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
    LogSearchResults { result: craft_core::LogSearchResult },
    IncidentForensics { timeline: craft_core::IncidentTimeline },
    IncidentList { incidents: Vec<IncidentSummary> },
    IngestResult { indexed_lines: usize, blocks_created: usize, duration_ms: u64 },
    WorkloadForecastResult { forecast: craft_core::WorkloadForecast },
    CostOptimizationReportResult { report: craft_core::CostOptimizationReport },
    WorkloadPolicyResult { policies: Vec<craft_core::WorkloadPolicy> },
    ProactiveScalingResult { message: String, applied_action: String },
    ModpackBuildResult { manifest: craft_core::ModpackBuildManifest },
    DeltaResult { delta_manifest: craft_core::DeltaPatchManifest },
    ModpackStatus {
        versions: Vec<craft_core::ModpackBuildManifest>,
        deltas: Vec<craft_core::DeltaPatchManifest>,
    },
    ModpackChunk { chunk: crate::modpack_service::ModpackChunkResponse },
    SdnTopologyResult { topology: crate::sdn_service::SdnTopologySummary },
    SdnPolicyResult { message: String, rules_count: usize },
    SdnKeyRotationResult { summary: crate::sdn_service::KeyRotationSummary },
    SdnPeerStatusResult { peer: Option<craft_net::WireguardPeerMetrics> },
    RaftStatusResult {
        status: crate::raft_engine::RaftStatusSummary,
    },
    RaftCommandProposedResult {
        term: u64,
        index: u64,
    },
    DistributedLockAcquiredResult {
        lock: craft_core::DistributedLock,
    },
    DistributedLockReleasedResult {
        message: String,
    },
    RaftStepDownResult {
        message: String,
    },
    RaftLeadershipTransferredResult {
        message: String,
    },
    RaftLogsResult {
        entries: Vec<craft_core::RaftLogEntry>,
    },
    RaftMultiRaftStatusResult {
        registry: craft_core::MultiRaftRegistry,
        statuses: std::collections::HashMap<u64, crate::raft_engine::RaftStatusSummary>,
        learner_progress: std::collections::HashMap<String, craft_core::LearnerSyncProgress>,
    },
    RaftReconfigureMembershipResult {
        success: bool,
        phase: craft_core::JointConsensusPhase,
        message: String,
    },
    RaftCompactionResult {
        group_id: u64,
        last_included_index: u64,
        entries_compacted: u64,
        snapshot_bytes: u64,
        duration_ms: u64,
    },
    RaftPartitionRouteResult {
        key: String,
        group_id: u64,
        partition_name: String,
        leader_node_id: Option<String>,
    },
    RaftManagePartitionResult {
        success: bool,
        message: String,
        partitions: Vec<craft_core::MultiRaftPartition>,
    },
    ServerQuotaResult {
        summary: craft_core::QuotaUsageSummary,
    },
    TenantQuotaResult {
        quota: craft_core::TenantQuota,
        allocated_memory_mb: u64,
        allocated_cpu_percent: u32,
        server_count: usize,
    },
    QuotaUsageListResult {
        items: Vec<craft_core::QuotaUsageSummary>,
    },
    FairShareEnforcedResult {
        rebalanced_count: usize,
        message: String,
    },
    TracingStatusResult {
        status: craft_core::TracingStatusSummary,
    },
    TracesQueryResult {
        spans: Vec<craft_core::RecordedSpan>,
    },
    TraceDetailsResult {
        trace_tree: Option<craft_core::TraceTree>,
    },
    TracesExportedResult {
        exported_count: usize,
        destination: String,
    },
    TracingConfigResult {
        config: craft_core::TracingConfig,
    },
    AnvilStatusResult {
        status: craft_core::AnvilStatusSummary,
    },
    AnvilRegionInspectionResult {
        details: craft_core::RegionDetails,
    },
    AnvilPrefetchResult {
        summary: craft_core::PrefetchSummary,
    },
    AnvilBenchmarkResult {
        report: craft_core::AnvilBenchmarkReport,
    },
    AnvilConfigResult {
        config: craft_core::AnvilConfig,
    },
    NumaStatusResult {
        summary: craft_core::NumaStatusSummary,
    },
    PinServerCoresResult {
        config: craft_core::ServerPinningConfig,
        message: String,
    },
    NumaPolicyResult {
        config: craft_core::ServerPinningConfig,
        message: String,
    },
    NumaBenchmarkResult {
        report: craft_core::NumaBenchmarkReport,
    },
    DpdkStatusResult {
        stats: craft_net::DpdkDriverStats,
    },
    Error { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentSummary {
    pub incident_id: String,
    pub server_name: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub culprit_exception: String,
    pub suspected_plugin: Option<String>,
    pub frames_count: usize,
    pub authenticity_valid: bool,
}
