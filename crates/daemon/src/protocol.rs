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
    MigrationStartLive {
        plan: craft_core::LiveMigrationPlan,
    },
    MigrationGetStatus {
        migration_id: Option<String>,
    },
    MigrationAbort {
        migration_id: String,
        reason: Option<String>,
    },
    MigrationList,
    AnycastRouteManage {
        action: String,
        route: craft_core::AnycastRouteAnnouncement,
    },
    EbpfStartProfiling {
        server_name: String,
        probe_type: craft_core::EbpfProbeType,
        duration_secs: u64,
        sample_rate_hz: u32,
    },
    EbpfGetStatus {
        server_name: String,
    },
    EbpfGetFlameGraph {
        server_name: String,
        format: String,
    },
    EbpfGetGcTelemetry {
        server_name: String,
        limit: usize,
    },
    EbpfStopProfiling {
        server_name: String,
        probe_id: Option<String>,
    },
    SupplyChainVerify {
        artifact_path: PathBuf,
        attestation_path: Option<PathBuf>,
        strict: bool,
    },
    SupplyChainGetPolicy,
    SupplyChainSetPolicy {
        policy: craft_core::SupplyChainPolicy,
    },
    SupplyChainInspectAttestation {
        identifier: String,
    },
    HermeticBuildRun {
        build_dir: PathBuf,
        command: String,
        args: Vec<String>,
        allow_network: bool,
    },
    PqcGetStatus,
    PqcSetPolicy {
        policy: craft_core::pqc::PqcPolicy,
    },
    PqcGenerateKeyPair {
        suite: Option<craft_core::pqc::PqcCipherSuite>,
        algorithm: Option<craft_core::pqc::PqcSigningAlgorithm>,
    },
    PqcBenchmark {
        iterations: usize,
    },
    PqcMigrateNode {
        target_phase: craft_core::pqc::PqcMigrationPhase,
    },
    HsmGetStatus,
    HsmGenerateKey {
        label: String,
        key_type: craft_core::hsm::HsmKeyType,
    },
    HsmSign {
        label: String,
        data: Vec<u8>,
    },
    HsmAttest {
        nonce: Option<[u8; 32]>,
        pcr_mask: u32,
    },
    HsmZkProve {
        cluster_id: String,
    },
    HsmZkVerify {
        cluster_id: String,
        proof: craft_core::hsm::ZkMembershipProof,
    },
    CompactionGetStatus,
    CompactionTriggerNow,
    CompactionConfigureThp {
        mode: craft_core::ThpMode,
        defrag: craft_core::ThpDefragMode,
    },
    CompactionGetPoolStats,
    CompactionResetMetrics,
    XdpGetStatus,
    XdpAttachInterface {
        interface_name: String,
        mode: craft_core::XdpAttachMode,
    },
    XdpDetachInterface,
    XdpAddRule {
        rule: craft_core::XdpFilterRule,
    },
    XdpRemoveRule {
        rule_id: String,
    },
    XdpResetMetrics,
    PmuGetStatus,
    PmuStartSampling {
        target_pid: Option<u32>,
        sample_rate_hz: u32,
    },
    PmuStopSampling,
    PmuSampleNow {
        target_pid: Option<u32>,
    },
    PmuGetHotspots {
        limit: usize,
    },
    PmuRunBench {
        iterations: usize,
    },
    PmuResetMetrics,
    ShmGetStatus {
        server: Option<String>,
    },
    ShmCreateChannel {
        server: String,
        channel: String,
        slot_size: usize,
        slot_count: usize,
    },
    ShmCloseChannel {
        server: String,
        channel: String,
    },
    ShmWriteEvent {
        server: String,
        channel: String,
        payload: Vec<u8>,
    },
    ShmReadEvents {
        server: String,
        channel: String,
        limit: usize,
    },
    ShmRunBench {
        message_count: usize,
        payload_size: usize,
    },
    ShmResetMetrics {
        server: Option<String>,
    },
    PatchGetStatus {
        server: Option<String>,
    },
    PatchApply {
        server: String,
        patch_name: String,
        target_symbol: String,
        shadow_bytes_hex: String,
    },
    PatchRollback {
        server: String,
        patch_name: String,
    },
    PatchGetDiff {
        server: String,
        patch_name: String,
    },
    PatchRunBench {
        iterations: usize,
    },
    PatchResetMetrics {
        server: Option<String>,
    },
    VmGetStatus {
        vm_id: Option<String>,
    },
    VmSpawn {
        name: String,
        vcpus: u32,
        memory_mb: u64,
        vsock_cid: Option<u32>,
        virtio_devices: Vec<String>,
    },
    VmStop {
        vm_id: String,
        force: bool,
    },
    VmInspect {
        vm_id: String,
    },
    VmRunBench {
        concurrency: usize,
        iterations: usize,
    },
    VmResetMetrics {
        vm_id: Option<String>,
    },
    CrashGetStatus {
        server: Option<String>,
    },
    CrashTriageFile {
        server: Option<String>,
        file_path: String,
    },
    CrashListReports {
        server: Option<String>,
        limit: Option<usize>,
    },
    CrashGetReport {
        report_id: String,
    },
    CrashRunBench {
        iterations: usize,
    },
    CrashResetMetrics {
        server: Option<String>,
    },
    RdmaGetStatus {
        server: Option<String>,
    },
    RdmaRegisterMr {
        server: Option<String>,
        size: usize,
        read_only: bool,
    },
    RdmaConnectPeer {
        server: Option<String>,
        peer_address: String,
        qp_num: u32,
    },
    RdmaListPeers {
        server: Option<String>,
    },
    RdmaRunBench {
        iterations: usize,
        buffer_size: usize,
    },
    RdmaResetMetrics {
        server: Option<String>,
    },
    SmartNicGetStatus {
        server: Option<String>,
    },
    SmartNicInstallRule {
        server: Option<String>,
        rule: craft_core::SmartNicOffloadRule,
    },
    SmartNicRemoveRule {
        server: Option<String>,
        rule_id: String,
    },
    SmartNicListRules {
        server: Option<String>,
    },
    SmartNicRunBench {
        iterations: usize,
        packet_size: usize,
    },
    SmartNicResetMetrics {
        server: Option<String>,
    },
    MemFabricGetStatus {
        server: Option<String>,
    },
    MemFabricAllocatePage {
        server: Option<String>,
        page_id: String,
        size: usize,
        tier: craft_core::memfabric::MemoryTier,
        dimension: Option<String>,
    },
    MemFabricEvictDimension {
        server: Option<String>,
        dimension: String,
        target_node: Option<String>,
    },
    MemFabricListPages {
        server: Option<String>,
    },
    MemFabricRunBench {
        iterations: usize,
        page_size: usize,
    },
    MemFabricResetMetrics {
        server: Option<String>,
    },
    NvmeGetStatus {
        server: Option<String>,
    },
    NvmeCreateNamespace {
        nsid: u32,
        size_mb: u64,
        block_size: u32,
        server_id: Option<String>,
        dimension: Option<String>,
    },
    NvmeDeleteNamespace {
        nsid: u32,
    },
    NvmeListNamespaces {
        server: Option<String>,
    },
    NvmeListSubsystems,
    NvmeRunBench {
        block_size: usize,
        iterations: usize,
    },
    NvmeResetMetrics,
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
    MigrationStarted {
        plan: craft_core::LiveMigrationPlan,
    },
    MigrationStatus {
        plans: Vec<craft_core::LiveMigrationPlan>,
    },
    MigrationAborted {
        plan: craft_core::LiveMigrationPlan,
        message: String,
    },
    MigrationListResult {
        plans: Vec<craft_core::LiveMigrationPlan>,
    },
    AnycastRouteManageResult {
        success: bool,
        message: String,
        routes: Vec<craft_core::AnycastRouteAnnouncement>,
    },
    EbpfProfilingStarted {
        descriptor: craft_core::EbpfProbeDescriptor,
    },
    EbpfStatusResult {
        descriptor: Option<craft_core::EbpfProbeDescriptor>,
        socket_telemetry: Option<craft_net::SocketBufferTelemetry>,
        syscall_aggregations: std::collections::HashMap<String, (u64, f64)>,
    },
    EbpfFlameGraphResult {
        server_name: String,
        format: String,
        content: String,
        root_node: craft_core::FlameGraphNode,
    },
    EbpfGcTelemetryResult {
        server_name: String,
        events: Vec<craft_core::JvmGcEvent>,
    },
    EbpfProfilingStopped {
        descriptor: craft_core::EbpfProbeDescriptor,
        message: String,
    },
    SupplyChainVerdict {
        verdict: craft_core::VerificationVerdict,
    },
    SupplyChainPolicyResult {
        policy: craft_core::SupplyChainPolicy,
        trust_anchors_count: usize,
    },
    SupplyChainAttestationResult {
        attestation: Option<craft_core::InTotoStatement>,
    },
    HermeticBuildResult {
        manifest: craft_core::HermeticBuildManifest,
    },
    PqcStatus {
        summary: craft_core::pqc::PqcStatusSummary,
    },
    PqcPolicyResult {
        policy: craft_core::pqc::PqcPolicy,
        message: String,
    },
    PqcKeyPairResult {
        keypair: craft_core::pqc::PqcKeyPair,
    },
    PqcBenchmarkResult {
        report: craft_core::pqc::PqcBenchmarkReport,
    },
    PqcMigrationResult {
        new_phase: craft_core::pqc::PqcMigrationPhase,
        message: String,
    },
    HsmStatus {
        summary: craft_core::hsm::HsmStatusSummary,
    },
    HsmKeyResult {
        key: craft_core::hsm::HsmKeyHandle,
    },
    HsmSignResult {
        signature: Vec<u8>,
    },
    HsmAttestResult {
        quote: craft_core::hsm::PcrQuote,
    },
    HsmZkProveResult {
        proof: craft_core::hsm::ZkMembershipProof,
    },
    HsmZkVerifyResult {
        valid: bool,
        message: String,
    },
    CompactionStatusResult {
        summary: craft_core::CompactionStatusSummary,
    },
    CompactionCycleResult {
        result: craft_core::CompactionCycleResult,
    },
    CompactionThpConfigured {
        status: craft_core::ThpStatus,
        message: String,
    },
    CompactionPoolStatsResult {
        stats: craft_core::PagePoolStats,
    },
    CompactionMetricsResetResult {
        message: String,
    },
    XdpStatusResult {
        status: craft_core::XdpInterfaceStatus,
    },
    XdpRuleModified {
        status: craft_core::XdpInterfaceStatus,
        message: String,
    },
    XdpMetricsResetResult {
        message: String,
    },
    PmuStatusResult {
        summary: craft_core::pmu::PmuMetricsSummary,
    },
    PmuSampleResult {
        sample: craft_core::pmu::PmuSampleRecord,
    },
    PmuHotspotsResult {
        hotspots: Vec<craft_core::pmu::HotspotSymbol>,
    },
    PmuBenchResult {
        report: craft_net::MemoryChurnReport,
    },
    PmuMetricsResetResult {
        message: String,
    },
    ShmStatusResult {
        summary: craft_core::shm::ShmStatusSummary,
    },
    ShmChannelCreatedResult {
        meta: craft_core::shm::ShmSegmentMeta,
    },
    ShmChannelClosedResult {
        closed: bool,
    },
    ShmEventWrittenResult {
        sequence: u64,
    },
    ShmEventsReadResult {
        events: Vec<Vec<u8>>,
    },
    ShmBenchResult {
        metrics: craft_core::shm::ShmBenchmarkMetrics,
    },
    ShmMetricsResetResult {
        message: String,
    },
    PatchStatusResult {
        summary: craft_core::patch::PatchStatusSummary,
    },
    PatchAppliedResult {
        manifest: craft_core::patch::PatchManifest,
    },
    PatchRolledBackResult {
        rolled_back: bool,
    },
    PatchDiffResult {
        diff: String,
    },
    PatchBenchResult {
        metrics: craft_core::patch::PatchBenchmarkMetrics,
    },
    PatchMetricsResetResult {
        message: String,
    },
    VmStatusResult {
        summary: craft_core::vm::MicroVmStatusSummary,
    },
    VmSpawnResult {
        descriptor: craft_core::vm::MicroVmDescriptor,
    },
    VmStopResult {
        stopped: bool,
    },
    VmInspectResult {
        descriptor: craft_core::vm::MicroVmDescriptor,
    },
    VmBenchResult {
        metrics: craft_core::vm::MicroVmBenchmarkMetrics,
    },
    VmResetMetricsResult {
        message: String,
    },
    CrashStatusResult {
        summary: craft_core::crash::CrashTriageStatusSummary,
    },
    CrashTriageReportResult {
        report: craft_core::crash::CrashTriageReport,
    },
    CrashReportsListResult {
        reports: Vec<craft_core::crash::CrashTriageReport>,
    },
    CrashBenchResult {
        metrics: craft_core::crash::CrashTriageBenchmarkMetrics,
    },
    CrashMetricsResetResult {
        message: String,
    },
    RdmaStatusResult {
        summary: craft_core::rdma::RdmaStatusSummary,
    },
    RdmaMrResult {
        mr: craft_core::rdma::MemoryRegionDescriptor,
    },
    RdmaPeerResult {
        peer: craft_core::rdma::RdmaPeerEndpoint,
    },
    RdmaPeersListResult {
        peers: Vec<craft_core::rdma::RdmaPeerEndpoint>,
    },
    RdmaBenchResult {
        metrics: craft_core::rdma::RdmaBenchmarkMetrics,
    },
    RdmaMetricsResetResult {
        message: String,
    },
    SmartNicStatusResult {
        summary: craft_core::SmartNicStatusSummary,
        devices: Vec<craft_core::SmartNicDeviceInfo>,
    },
    SmartNicRuleResult {
        rule: craft_core::SmartNicOffloadRule,
    },
    SmartNicRemoveResult {
        success: bool,
    },
    SmartNicRulesListResult {
        rules: Vec<craft_core::SmartNicOffloadRule>,
    },
    SmartNicBenchResult {
        metrics: craft_core::SmartNicBenchmarkMetrics,
    },
    SmartNicResetResult {
        message: String,
    },
    MemFabricStatusResult {
        summary: craft_core::memfabric::MemFabricStatusSummary,
        nodes: Vec<craft_core::memfabric::MemFabricNodeInfo>,
    },
    MemFabricPageResult {
        page: craft_core::memfabric::RemotePageDescriptor,
    },
    MemFabricEvictResult {
        pages_evicted: usize,
        bytes_freed: u64,
        dimension: String,
    },
    MemFabricPagesListResult {
        pages: Vec<craft_core::memfabric::RemotePageDescriptor>,
    },
    MemFabricBenchResult {
        metrics: craft_core::memfabric::MemFabricBenchmarkMetrics,
    },
    MemFabricResetResult {
        message: String,
    },
    NvmeStatusResult {
        summary: craft_core::nvme::NvmeStatusSummary,
        subsystems: Vec<craft_core::nvme::NvmeSubsystemDescriptor>,
    },
    NvmeNamespaceResult {
        namespace: craft_core::nvme::NvmeNamespaceDescriptor,
    },
    NvmeDeleteResult {
        nsid: u32,
        success: bool,
        message: String,
    },
    NvmeNamespacesListResult {
        namespaces: Vec<craft_core::nvme::NvmeNamespaceDescriptor>,
    },
    NvmeSubsystemsListResult {
        subsystems: Vec<craft_core::nvme::NvmeSubsystemDescriptor>,
    },
    NvmeBenchResult {
        metrics: craft_core::nvme::NvmeBenchmarkMetrics,
    },
    NvmeResetResult {
        message: String,
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
