pub mod anvil;
pub mod audit;
pub mod autoscale_config;
pub mod backup_config;
pub mod cache;
pub mod cluster_config;
pub mod config;
pub mod crypto;
pub mod dr_config;
pub mod ebpf;
pub mod edge_config;
pub mod error;
pub mod forecasting;
pub mod game;
pub mod hsm;
pub mod intelligence_config;
pub mod java;
pub mod log_index;
pub mod mesh_config;
pub mod migration;
pub mod modpack_ci;
pub mod nbt;
pub mod numa;
pub mod optimizer;
pub mod path;
pub mod pqc;
pub mod process;
pub mod properties;
pub mod raft;
pub mod rbac;
pub mod remote_config;
pub mod rollout;
pub mod sdn;
pub mod supply_chain;
pub mod timeseries;
pub mod trash;
pub mod cgroups;
pub mod tracing_core;
pub mod version;
pub mod webhook_config;

pub use anvil::{
    chunk_coords_to_index, compress_payload, decompress_payload, region_coords_from_chunk,
    region_filename, AnvilBenchmarkReport, AnvilCacheStats, AnvilChunkCache, AnvilConfig,
    AnvilIoEngine, AnvilRegistry, AnvilStatusSummary, CachedChunk, ChunkCompressionScheme,
    ChunkData, ChunkKey, ChunkLocation, ChunkSectorInfo, CompactionStats, IoBatchRead,
    IoBatchWrite, IoEngineType, IoReadResult, IoWriteResult, PrefetchSummary, RegionDetails,
    RegionFileReader, RegionFileWriter, RegionFileHeader, RegionInspection, HEADER_BYTES,
    HEADER_SECTORS, SECTOR_BYTES, TOTAL_CHUNKS,
};
pub use audit::{AuditLedger, AuditLogEntry, AuditVerificationResult, DEFAULT_AUDIT_SECRET, GENESIS_HASH};
pub use autoscale_config::{AutoscaleRegistry, ServerAutoscalePolicy};
pub use cgroups::{
    CgroupStatSnapshot, CgroupV2Driver, QuotaRegistry, QuotaUsageSummary, ServerPriority,
    ServerResourceLimit, TenantQuota,
};
pub use crypto::{derive_key, ChaCha20, ChaCha20Poly1305, Poly1305};
pub use dr_config::{DrFailoverReport, DrPlan, DrRecoveryResult, DrRunbook, DrSimulationResult};
pub use ebpf::{
    EbpfProbeDescriptor, EbpfProbeStatus, EbpfProbeType, EbpfRegistry, FlameGraphNode, GcPhase,
    JvmGcEvent, SyscallInterceptionRecord, ThreadContentionFrame,
};
pub use forecasting::{
    generate_forecast_sparkline, load_workload_samples, save_workload_samples, CostOptimizationModel,
    CostOptimizationReport, DayOfWeekProfile, DiurnalHourProfile, ForecastPoint,
    ForecastingRegistry, HourlyWorkloadSample, ResourceTier, ResourceThrottlingPlan,
    SeasonalForecaster, WorkloadForecast, WorkloadPolicy, DEFAULT_RAM_GIB_HOURLY_COST,
    DEFAULT_VCPU_HOURLY_COST,
};
pub use hsm::{
    generate_aik_keypair, generate_pcr_quote, verify_pcr_quote, HsmBackendType, HsmEngine,
    HsmKeyHandle, HsmKeyType, HsmRegistry, HsmSlotInfo, HsmStatusSummary, HsmTokenSession,
    PcrQuote, TpmPcrBank, ZkMembershipEngine, ZkMembershipProof, U256, TPM_PCR_COUNT,
};
pub use edge_config::{
    BackboneCondition, BackboneStatus, CrossRegionChatEnvelope, EdgeNode, EdgeRegistry,
    GeoRoutingPolicy, InventorySnapshot, ItemStackSnapshot, LatencyPlaybook, PlaybookPreset,
    PlayerSessionHandoff, PotionEffectSnapshot, RoutingStrategy, DEFAULT_CHAT_SECRET,
};
pub use intelligence_config::{
    format_report_markdown, AnomalyRecord, AnomalySeverity, AnomalyType, AutopilotMode,
    DiagnosticReport, IntelligencePolicy, IntelligenceRegistry, RemediationAction,
};
pub use log_index::{
    demangle_stack_trace, CulpritType, IncidentTimeline, InvertedIndexBlock, LogEntry, LogLevel,
    LogQuery, LogSearchResult, StackFrame,
};
pub use mesh_config::{
    MeshPolicy, MeshRegistry, MeshTarget, MeshTargetKind, ReplicationQuorum,
};
pub use migration::{
    build_server_checkpoint_manifest, evaluate_pre_copy_convergence, AnycastRouteAnnouncement,
    AnycastRouteStatus, DirtyMemoryTracker, LiveMigrationPlan, MemoryPageChunk, MigrationRegistry,
    MigrationStage, PlayerSessionDescriptor, PreCopyRound, ServerCheckpointManifest,
};
pub use modpack_ci::{
    BinaryDeltaHeader, DeltaOp, DeltaPatchManifest, ModSide, ModpackBuildManifest,
    ModpackComponent, ModpackRecord, ModpackRegistry, DEFAULT_DELTA_BLOCK_SIZE, DELTA_MAGIC,
    DELTA_VERSION,
};
pub use numa::{
    format_cpu_range_string, parse_cpu_range_string, CpuAffinityManager, HugepageManager,
    HugepageSummary, KernelBootParams, NumaBenchmarkReport, NumaNode, NumaPolicy, NumaRegistry,
    NumaStatusSummary, NumaTopology, ServerPinningConfig,
};
pub use raft::{
    calculate_fencing_token, chunk_snapshot_data, compute_crc32, fencing_token_parts,
    reassemble_snapshot_chunks, ArbitrationWeight, DistributedLock, JointConsensusPhase,
    LearnerSyncProgress, MembershipChangeType, MultiRaftPartition, MultiRaftRegistry,
    PartitionRoutingKey, PersistentRaftState, QuorumStatus, RaftLogEntry, RaftNode,
    RaftPayload, RaftRegistry, RaftRole, RaftSnapshot, RaftSnapshotMeta, SnapshotChunk,
    WalCompactionPolicy, GROUP_CONTROL_PLANE, GROUP_DISTRIBUTED_LOCKS, GROUP_WORLD_BASE,
};
pub use rbac::{Permission, RbacRegistry, Role, UserAccount};
pub use rollout::{
    CanaryHealthCriteria, FleetHealingAction, FleetHealthStatus, NodeHealth, RolloutPlan,
    RolloutRecord, RolloutRegistry, RolloutStage, RolloutStrategy,
};
pub use pqc::{
    barrett_reduce, derive_pqc_prng, fqmul, montgomery_reduce, x25519_keypair, x25519_scalar_mult,
    HybridCiphertext, HybridKeyExchange, HybridPublicKey, HybridSecretKey, MlDsa65,
    MlDsa65PublicKey, MlDsa65SecretKey, MlKem1024, MlKem1024PublicKey, MlKem1024SecretKey,
    MlKem768, MlKem768PublicKey, MlKem768SecretKey, Poly, PqcBenchmarkReport, PqcCipherSuite,
    PqcEnforcementMode, PqcKeyPair, PqcMigrationPhase, PqcPolicy, PqcRegistry,
    PqcSigningAlgorithm, PqcStatusSummary, KYBER_N, KYBER_Q, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SECRET_KEY_BYTES, MLDSA65_SIGNATURE_BYTES, MLKEM1024_CIPHERTEXT_BYTES,
    MLKEM1024_PUBLIC_KEY_BYTES, MLKEM1024_SECRET_KEY_BYTES, MLKEM768_CIPHERTEXT_BYTES,
    MLKEM768_PUBLIC_KEY_BYTES, MLKEM768_SECRET_KEY_BYTES,
};
pub use sdn::{
    FilterAction, FilterProtocol, IsolationZone, LocalNodeConfig, MicrosegmentationPolicy,
    MicrosegmentationRule, SdnMeshConfig, SdnRegistry, WireguardPeer,
};
pub use supply_chain::{
    compute_file_sha256, create_inclusion_proof, dsse_pae, evaluate_supply_chain,
    sign_in_toto_statement, BuildCompleteness, BuildInvocation, BuildMaterial, BuildMetadata,
    BuilderInfo, ConfigSource, DsseEnvelope, DsseSignature, EnforcementMode,
    HermeticBuildManifest, InTotoStatement, InclusionProof, SigstoreBundle, SlsaLevel,
    SlsaPredicate, Subject, SupplyChainPolicy, SupplyChainRegistry, TransparencyLogEntry,
    TrustAnchor, VerificationMaterial, VerificationVerdict, DSSE_PAYLOAD_TYPE,
    IN_TOTO_STATEMENT_V1, SLSA_PREDICATE_V02,
};
pub use timeseries::{RegressionResult, RollingTimeSeries, SawtoothMetrics, TimeSeriesSample};
pub use tracing_core::{
    generate_random_bytes, ActiveSpan, OtlpJsonExporter, RecordedSpan, SamplerStrategy,
    SpanAttributeValue, SpanEvent, SpanId, SpanKind, SpanLink, SpanRingBuffer, SpanStatus,
    SpanStatusCode, TraceContext, TraceId, TraceSampler, TraceTree, TraceTreeNode, Tracer,
    TracingConfig, TracingRegistry, TracingStatusSummary,
};

pub use backup_config::{
    AutoBackupPolicy, GDriveBackupConfig, GDriveBackupTarget, GlobalBackupRegistry,
    LocalBackupTarget, S3BackupConfig, S3BackupTarget,
};
pub use cache::{format_size, parse_size, CacheEntryMeta, CacheStats, CacheStore};
pub use cluster_config::{ClusterNode, ClusterRole, ClustersRegistry, ServerCluster};
pub use config::{
    default_game_id, get_default_world, get_dimension_worlds, set_default_world, set_end_world,
    set_nether_world, GlobalSettings, ServerConfig, ServersRegistry,
};
pub use error::{CraftError, Result};
pub use game::{
    find_game, get_supported_games, ConfigFormat, ContentCapabilities, GameDefinition,
    QueryProtocolKind, RuntimeKind, TransportProtocol,
};
pub use java::{find_best_java, get_jar_java_version, get_java_installations, JavaInstallation};
pub use nbt::{NbtFile, NbtTag};
pub use optimizer::{GcStrategy, MemoryOptimizer, OptimizationProfile, OptimizationRecommendation};
pub use path::CraftPaths;
pub use process::{
    auto_heal_server_file, auto_heal_server_jar, get_server_running_pid, is_process_running,
    is_server_locked, kill_process, read_pid_file, remove_pid_file, write_pid_file,
    ServerLockGuard,
};
pub use properties::{PropertyCategory, PropertyLine, ServerProperties};
pub use remote_config::{RemoteAuthType, RemoteHostConfig, RemoteOsType, RemotesRegistry};
pub use trash::{TrashItem, TrashManager, TrashManifest};
pub use version::{
    compare_versions, is_stable_version, sort_versions_descending, ReleaseTier, VersionToken,
};
pub use webhook_config::{WebhookEndpoint, WebhookEvent, WebhookKind, WebhooksRegistry};

pub const CRAFT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Truncates a string to at most `max_chars` Unicode scalar values without slicing across UTF-8 boundaries.
pub fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

/// Truncates a string to at most `max_chars` characters, appending an ellipsis ("...") if truncated.
/// The resulting string length in characters will not exceed `max_chars` (unless `max_chars < 3`).
pub fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let keep_chars = max_chars.saturating_sub(3);
        let truncated = truncate_str(s, keep_chars);
        format!("{}...", truncated)
    }
}

/// Parses a semantic version string (e.g. "1.0.1" or "v1.2.3-alpha") into (major, minor, patch)
pub fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let clean = v.trim().trim_start_matches('v');
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() >= 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.split('-').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_semver("v1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_semver("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_semver("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("invalid"), None);
        assert!(parse_semver("1.0.0").unwrap() < parse_semver("1.0.1").unwrap());
        assert!(parse_semver("1.0.1").unwrap() < parse_semver("1.0.2").unwrap());
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Cyrillic string where byte index 37 falls inside a 2-byte character 'и'
        let cyrillic = "Плагин для серверов Minecraft и других игр";
        // Ensure truncate_str does not panic
        let t = truncate_str(cyrillic, 37);
        assert!(t.len() <= cyrillic.len());

        // Ensure truncate_ellipsis does not panic and ends with ellipsis
        let el = truncate_ellipsis(cyrillic, 40);
        assert!(el.ends_with("..."));
        assert_eq!(el.chars().count(), 40);

        // Multi-byte CJK and 4-byte Unicode characters
        let multibyte_str = "Minecraft Server \u{10348} Best Plugins & Performance 日本語";
        let em = truncate_ellipsis(multibyte_str, 25);
        assert!(em.ends_with("..."));
        assert_eq!(em.chars().count(), 25);

        // Short string should not be truncated
        assert_eq!(truncate_ellipsis("short", 10), "short");
        assert_eq!(truncate_str("hello", 10), "hello");
    }
}
