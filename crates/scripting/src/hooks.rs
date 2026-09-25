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
    SdnMeshReconfigured,
    SdnPacketDropped,
    SdnCertRotated,
    RaftLeaderElected,
    RaftSplitBrainDetected,
    RaftLockContended,
    ResourceQuotaExceeded,
    CgroupThrottled,
    FairShareAdjusted,
    TraceSpanRecorded,
    OtlpExportFailed,
    TraceSamplingSurge,
    ChunkPrefetchCompleted,
    AnvilCacheSaturated,
    AnvilIoError,
    NumaMigrationTriggered,
    DpdkPacketFloodAlert,
    CorePinningAdjusted,
    RaftMembershipReconfigured,
    RaftCompactionCompleted,
    MultiRaftPartitionCreated,
    LiveMigrationInitiated,
    LiveMigrationFreezeStarted,
    LiveMigrationCompleted,
    LiveMigrationRolledBack,
    EbpfProbeAttached,
    JvmSafepointSpikeDetected,
    GcPauseThresholdExceeded,
    ThreadContentionSurge,
    SupplyChainVerified,
    SupplyChainViolationBlocked,
    HermeticBuildCompleted,
    PqcHandshakeCompleted,
    PqcPolicyMigrated,
    PqcDegradedFallbackDetected,
    HsmTokenInserted,
    EnclaveAttestationVerified,
    ZkMembershipValidated,
    MemoryCompactionCompleted,
    HighMemoryFragmentationDetected,
    ThpAllocationStallAlert,
    XdpDdosAttackMitigated,
    XdpFlowRateLimitExceeded,
    XdpInterfaceAttached,
    PmuHotspotDetected,
    CacheMissThresholdExceeded,
    BranchMispredictionSurge,
    ShmChannelOpened,
    ShmChannelClosed,
    ShmLeaseExpired,
    ShmThroughputThresholdExceeded,
    PatchApplied,
    PatchRolledBack,
    PatchSafetyCheckFailed,
    MicroVmSpawned,
    MicroVmTerminated,
    MicroVmIsolationAlert,
    CrashTriageCompleted,
    MemoryLeakDetected,
    CriticalFaultRemediated,
    RdmaLinkEstablished,
    RdmaFailoverTriggered,
    RdmaLatencySpike,
    SmartNicOffloadInstalled,
    SmartNicTcamSaturated,
    SmartNicFallbackEngaged,
    MemFabricPageFaultResolved,
    MemFabricDimensionPaged,
    MemFabricPoolSaturated,
    NvmeNamespaceCreated,
    NvmeNamespaceDeleted,
    NvmeMultipathFailoverTriggered,
    NvmePoolCapacityAlert,
    VpnTunnelEstablished,
    VpnKeyRotated,
    VpnPeerConnected,
    VpnSecurityDegradedAlert,
    BftBlockCommitted,
    BftQuorumFormed,
    BftValidatorSlashed,
    BftViewTimeout,
    KernelMicroStallDetected,
    RealtimePriorityEscalated,
    IrqStormShielded,
    JitterThresholdExceeded,
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
            Self::SdnMeshReconfigured => "on_sdn_mesh_reconfigured",
            Self::SdnPacketDropped => "on_sdn_packet_dropped",
            Self::SdnCertRotated => "on_sdn_cert_rotated",
            Self::RaftLeaderElected => "on_raft_leader_elected",
            Self::RaftSplitBrainDetected => "on_raft_split_brain_detected",
            Self::RaftLockContended => "on_raft_lock_contended",
            Self::ResourceQuotaExceeded => "on_resource_quota_exceeded",
            Self::CgroupThrottled => "on_cgroup_throttled",
            Self::FairShareAdjusted => "on_fair_share_adjusted",
            Self::TraceSpanRecorded => "on_trace_span_recorded",
            Self::OtlpExportFailed => "on_otlp_export_failed",
            Self::TraceSamplingSurge => "on_trace_sampling_surge",
            Self::ChunkPrefetchCompleted => "on_chunk_prefetch_completed",
            Self::AnvilCacheSaturated => "on_anvil_cache_saturated",
            Self::AnvilIoError => "on_anvil_io_error",
            Self::NumaMigrationTriggered => "on_numa_migration_triggered",
            Self::DpdkPacketFloodAlert => "on_dpdk_packet_flood_alert",
            Self::CorePinningAdjusted => "on_core_pinning_adjusted",
            Self::RaftMembershipReconfigured => "on_raft_membership_reconfigured",
            Self::RaftCompactionCompleted => "on_raft_compaction_completed",
            Self::MultiRaftPartitionCreated => "on_multiraft_partition_created",
            Self::LiveMigrationInitiated => "on_live_migration_initiated",
            Self::LiveMigrationFreezeStarted => "on_live_migration_freeze_started",
            Self::LiveMigrationCompleted => "on_live_migration_completed",
            Self::LiveMigrationRolledBack => "on_live_migration_rolled_back",
            Self::EbpfProbeAttached => "on_ebpf_probe_attached",
            Self::JvmSafepointSpikeDetected => "on_jvm_safepoint_spike_detected",
            Self::GcPauseThresholdExceeded => "on_gc_pause_threshold_exceeded",
            Self::ThreadContentionSurge => "on_thread_contention_surge",
            Self::SupplyChainVerified => "on_supply_chain_verified",
            Self::SupplyChainViolationBlocked => "on_supply_chain_violation_blocked",
            Self::HermeticBuildCompleted => "on_hermetic_build_completed",
            Self::PqcHandshakeCompleted => "on_pqc_handshake_completed",
            Self::PqcPolicyMigrated => "on_pqc_policy_migrated",
            Self::PqcDegradedFallbackDetected => "on_pqc_degraded_fallback_detected",
            Self::HsmTokenInserted => "on_hsm_token_inserted",
            Self::EnclaveAttestationVerified => "on_enclave_attestation_verified",
            Self::ZkMembershipValidated => "on_zk_membership_validated",
            Self::MemoryCompactionCompleted => "on_memory_compaction_completed",
            Self::HighMemoryFragmentationDetected => "on_high_memory_fragmentation_detected",
            Self::ThpAllocationStallAlert => "on_thp_allocation_stall_alert",
            Self::XdpDdosAttackMitigated => "on_xdp_ddos_attack_mitigated",
            Self::XdpFlowRateLimitExceeded => "on_xdp_flow_rate_limit_exceeded",
            Self::XdpInterfaceAttached => "on_xdp_interface_attached",
            Self::PmuHotspotDetected => "on_pmu_hotspot_detected",
            Self::CacheMissThresholdExceeded => "on_cache_miss_threshold_exceeded",
            Self::BranchMispredictionSurge => "on_branch_misprediction_surge",
            Self::ShmChannelOpened => "on_shm_channel_opened",
            Self::ShmChannelClosed => "on_shm_channel_closed",
            Self::ShmLeaseExpired => "on_shm_lease_expired",
            Self::ShmThroughputThresholdExceeded => "on_shm_throughput_threshold_exceeded",
            Self::PatchApplied => "on_patch_applied",
            Self::PatchRolledBack => "on_patch_rolled_back",
            Self::PatchSafetyCheckFailed => "on_patch_safety_check_failed",
            Self::MicroVmSpawned => "on_microvm_spawned",
            Self::MicroVmTerminated => "on_microvm_terminated",
            Self::MicroVmIsolationAlert => "on_microvm_isolation_alert",
            Self::CrashTriageCompleted => "on_crash_triage_completed",
            Self::MemoryLeakDetected => "on_memory_leak_detected",
            Self::CriticalFaultRemediated => "on_critical_fault_remediated",
            Self::RdmaLinkEstablished => "on_rdma_link_established",
            Self::RdmaFailoverTriggered => "on_rdma_failover_triggered",
            Self::RdmaLatencySpike => "on_rdma_latency_spike",
            Self::SmartNicOffloadInstalled => "on_smartnic_offload_installed",
            Self::SmartNicTcamSaturated => "on_smartnic_tcam_saturated",
            Self::SmartNicFallbackEngaged => "on_smartnic_fallback_engaged",
            Self::MemFabricPageFaultResolved => "on_memfabric_page_fault_resolved",
            Self::MemFabricDimensionPaged => "on_memfabric_dimension_paged",
            Self::MemFabricPoolSaturated => "on_memfabric_pool_saturated",
            Self::NvmeNamespaceCreated => "on_nvme_namespace_created",
            Self::NvmeNamespaceDeleted => "on_nvme_namespace_deleted",
            Self::NvmeMultipathFailoverTriggered => "on_nvme_multipath_failover",
            Self::NvmePoolCapacityAlert => "on_nvme_pool_capacity_alert",
            Self::VpnTunnelEstablished => "on_vpn_tunnel_established",
            Self::VpnKeyRotated => "on_vpn_key_rotated",
            Self::VpnPeerConnected => "on_vpn_peer_connected",
            Self::VpnSecurityDegradedAlert => "on_vpn_security_degraded_alert",
            Self::BftBlockCommitted => "on_bft_block_committed",
            Self::BftQuorumFormed => "on_bft_quorum_formed",
            Self::BftValidatorSlashed => "on_bft_validator_slashed",
            Self::BftViewTimeout => "on_bft_view_timeout",
            Self::KernelMicroStallDetected => "on_kernel_microstall_detected",
            Self::RealtimePriorityEscalated => "on_realtime_priority_escalated",
            Self::IrqStormShielded => "on_irq_storm_shielded",
            Self::JitterThresholdExceeded => "on_jitter_threshold_exceeded",
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
            "on_sdn_mesh_reconfigured" | "sdn_mesh_reconfigured" | "sdn_reconfigure" => Some(Self::SdnMeshReconfigured),
            "on_sdn_packet_dropped" | "sdn_packet_dropped" | "packet_dropped" | "packet_drop" => Some(Self::SdnPacketDropped),
            "on_sdn_cert_rotated" | "sdn_cert_rotated" | "cert_rotated" => Some(Self::SdnCertRotated),
            "on_raft_leader_elected" | "raft_leader_elected" | "leader_elected" => Some(Self::RaftLeaderElected),
            "on_raft_split_brain_detected" | "raft_split_brain_detected" | "split_brain_detected" | "split_brain" => Some(Self::RaftSplitBrainDetected),
            "on_raft_lock_contended" | "raft_lock_contended" | "lock_contended" => Some(Self::RaftLockContended),
            "on_resource_quota_exceeded" | "resource_quota_exceeded" | "quota_exceeded" => Some(Self::ResourceQuotaExceeded),
            "on_cgroup_throttled" | "cgroup_throttled" | "throttled" => Some(Self::CgroupThrottled),
            "on_fair_share_adjusted" | "fair_share_adjusted" | "fair_share" => Some(Self::FairShareAdjusted),
            "on_trace_span_recorded" | "trace_span_recorded" | "span_recorded" => Some(Self::TraceSpanRecorded),
            "on_otlp_export_failed" | "otlp_export_failed" | "export_failed" => Some(Self::OtlpExportFailed),
            "on_trace_sampling_surge" | "trace_sampling_surge" | "sampling_surge" => Some(Self::TraceSamplingSurge),
            "on_chunk_prefetch_completed" | "chunk_prefetch_completed" | "prefetch_completed" => Some(Self::ChunkPrefetchCompleted),
            "on_anvil_cache_saturated" | "anvil_cache_saturated" | "cache_saturated" => Some(Self::AnvilCacheSaturated),
            "on_anvil_io_error" | "anvil_io_error" | "anvil_error" => Some(Self::AnvilIoError),
            "on_numa_migration_triggered" | "numa_migration_triggered" | "numa_migration" => Some(Self::NumaMigrationTriggered),
            "on_dpdk_packet_flood_alert" | "dpdk_packet_flood_alert" | "dpdk_flood" => Some(Self::DpdkPacketFloodAlert),
            "on_core_pinning_adjusted" | "core_pinning_adjusted" | "pinning_adjusted" => Some(Self::CorePinningAdjusted),
            "on_raft_membership_reconfigured" | "raft_membership_reconfigured" | "reconfigure" => Some(Self::RaftMembershipReconfigured),
            "on_raft_compaction_completed" | "raft_compaction_completed" | "compaction" => Some(Self::RaftCompactionCompleted),
            "on_multiraft_partition_created" | "multiraft_partition_created" | "partition_created" => Some(Self::MultiRaftPartitionCreated),
            "on_live_migration_initiated" | "live_migration_initiated" | "migration_initiated" => Some(Self::LiveMigrationInitiated),
            "on_live_migration_freeze_started" | "live_migration_freeze_started" | "freeze_started" => Some(Self::LiveMigrationFreezeStarted),
            "on_live_migration_completed" | "live_migration_completed" | "migration_completed" => Some(Self::LiveMigrationCompleted),
            "on_live_migration_rolled_back" | "live_migration_rolled_back" | "migration_rolled_back" => Some(Self::LiveMigrationRolledBack),
            "on_ebpf_probe_attached" | "ebpf_probe_attached" | "probe_attached" => Some(Self::EbpfProbeAttached),
            "on_jvm_safepoint_spike_detected" | "jvm_safepoint_spike_detected" | "safepoint_spike" => Some(Self::JvmSafepointSpikeDetected),
            "on_gc_pause_threshold_exceeded" | "gc_pause_threshold_exceeded" | "gc_threshold" | "gc_pause" => Some(Self::GcPauseThresholdExceeded),
            "on_thread_contention_surge" | "thread_contention_surge" | "contention_surge" | "lock_surge" => Some(Self::ThreadContentionSurge),
            "on_supply_chain_verified" | "supply_chain_verified" | "verified" => Some(Self::SupplyChainVerified),
            "on_supply_chain_violation_blocked" | "supply_chain_violation_blocked" | "violation_blocked" => Some(Self::SupplyChainViolationBlocked),
            "on_hermetic_build_completed" | "hermetic_build_completed" | "hermetic_build" => Some(Self::HermeticBuildCompleted),
            "on_pqc_handshake_completed" | "pqc_handshake_completed" | "pqc_handshake" => Some(Self::PqcHandshakeCompleted),
            "on_pqc_policy_migrated" | "pqc_policy_migrated" | "pqc_migrate" => Some(Self::PqcPolicyMigrated),
            "on_pqc_degraded_fallback_detected" | "pqc_degraded_fallback_detected" | "pqc_fallback" => Some(Self::PqcDegradedFallbackDetected),
            "on_hsm_token_inserted" | "hsm_token_inserted" | "token_inserted" => Some(Self::HsmTokenInserted),
            "on_enclave_attestation_verified" | "enclave_attestation_verified" | "attestation_verified" => Some(Self::EnclaveAttestationVerified),
            "on_zk_membership_validated" | "zk_membership_validated" | "membership_validated" => Some(Self::ZkMembershipValidated),
            "on_memory_compaction_completed" | "memory_compaction_completed" | "memory_compaction" => Some(Self::MemoryCompactionCompleted),
            "on_high_memory_fragmentation_detected" | "high_memory_fragmentation_detected" | "high_fragmentation" => Some(Self::HighMemoryFragmentationDetected),
            "on_thp_allocation_stall_alert" | "thp_allocation_stall_alert" | "thp_stall" => Some(Self::ThpAllocationStallAlert),
            "on_xdp_ddos_attack_mitigated" | "xdp_ddos_attack_mitigated" | "ddos_mitigated" | "ddos" => Some(Self::XdpDdosAttackMitigated),
            "on_xdp_flow_rate_limit_exceeded" | "xdp_flow_rate_limit_exceeded" | "flow_rate_limited" | "rate_limited" => Some(Self::XdpFlowRateLimitExceeded),
            "on_xdp_interface_attached" | "xdp_interface_attached" | "interface_attached" | "xdp_attached" => Some(Self::XdpInterfaceAttached),
            "on_pmu_hotspot_detected" | "pmu_hotspot_detected" | "pmu_hotspot" => Some(Self::PmuHotspotDetected),
            "on_cache_miss_threshold_exceeded" | "cache_miss_threshold_exceeded" | "cache_miss" => Some(Self::CacheMissThresholdExceeded),
            "on_branch_misprediction_surge" | "branch_misprediction_surge" | "branch_surge" => Some(Self::BranchMispredictionSurge),
            "on_shm_channel_opened" | "shm_channel_opened" | "shm_opened" => Some(Self::ShmChannelOpened),
            "on_shm_channel_closed" | "shm_channel_closed" | "shm_closed" => Some(Self::ShmChannelClosed),
            "on_shm_lease_expired" | "shm_lease_expired" | "shm_expired" => Some(Self::ShmLeaseExpired),
            "on_shm_throughput_threshold_exceeded" | "shm_throughput_threshold_exceeded" | "shm_throughput" => Some(Self::ShmThroughputThresholdExceeded),
            "on_patch_applied" | "patch_applied" | "patch_apply" => Some(Self::PatchApplied),
            "on_patch_rolled_back" | "patch_rolled_back" | "patch_rollback" => Some(Self::PatchRolledBack),
            "on_patch_safety_check_failed" | "patch_safety_check_failed" | "patch_safety_failed" => Some(Self::PatchSafetyCheckFailed),
            "on_microvm_spawned" | "microvm_spawned" | "vm_spawned" | "vm_start" => Some(Self::MicroVmSpawned),
            "on_microvm_terminated" | "microvm_terminated" | "vm_terminated" | "vm_stop" => Some(Self::MicroVmTerminated),
            "on_microvm_isolation_alert" | "microvm_isolation_alert" | "vm_alert" | "isolation_alert" => Some(Self::MicroVmIsolationAlert),
            "on_crash_triage_completed" | "crash_triage_completed" | "crash_triage" => Some(Self::CrashTriageCompleted),
            "on_memory_leak_detected" | "memory_leak_detected" | "leak_detected" | "leak" => Some(Self::MemoryLeakDetected),
            "on_critical_fault_remediated" | "critical_fault_remediated" | "fault_remediated" => Some(Self::CriticalFaultRemediated),
            "on_rdma_link_established" | "rdma_link_established" | "rdma_link" | "rdma_connect" => Some(Self::RdmaLinkEstablished),
            "on_rdma_failover_triggered" | "rdma_failover_triggered" | "rdma_failover" => Some(Self::RdmaFailoverTriggered),
            "on_rdma_latency_spike" | "rdma_latency_spike" | "rdma_spike" => Some(Self::RdmaLatencySpike),
            "on_smartnic_offload_installed" | "smartnic_offload_installed" | "smartnic_installed" => Some(Self::SmartNicOffloadInstalled),
            "on_smartnic_tcam_saturated" | "smartnic_tcam_saturated" | "tcam_saturated" => Some(Self::SmartNicTcamSaturated),
            "on_smartnic_fallback_engaged" | "smartnic_fallback_engaged" | "smartnic_fallback" => Some(Self::SmartNicFallbackEngaged),
            "on_memfabric_page_fault_resolved" | "memfabric_page_fault_resolved" | "page_fault_resolved" => Some(Self::MemFabricPageFaultResolved),
            "on_memfabric_dimension_paged" | "memfabric_dimension_paged" | "dimension_paged" => Some(Self::MemFabricDimensionPaged),
            "on_memfabric_pool_saturated" | "memfabric_pool_saturated" | "memfabric_saturated" => Some(Self::MemFabricPoolSaturated),
            "on_nvme_namespace_created" | "nvme_namespace_created" | "namespace_created" => Some(Self::NvmeNamespaceCreated),
            "on_nvme_namespace_deleted" | "nvme_namespace_deleted" | "namespace_deleted" => Some(Self::NvmeNamespaceDeleted),
            "on_nvme_multipath_failover" | "nvme_multipath_failover" | "multipath_failover" => Some(Self::NvmeMultipathFailoverTriggered),
            "on_nvme_pool_capacity_alert" | "nvme_pool_capacity_alert" | "nvme_pool_alert" => Some(Self::NvmePoolCapacityAlert),
            "on_vpn_tunnel_established" | "vpn_tunnel_established" | "vpn_established" | "tunnel_established" => Some(Self::VpnTunnelEstablished),
            "on_vpn_key_rotated" | "vpn_key_rotated" | "vpn_rekey" | "key_rotated" => Some(Self::VpnKeyRotated),
            "on_vpn_peer_connected" | "vpn_peer_connected" | "peer_connected" => Some(Self::VpnPeerConnected),
            "on_vpn_security_degraded_alert" | "vpn_security_degraded_alert" | "vpn_degraded" | "security_degraded" => Some(Self::VpnSecurityDegradedAlert),
            "on_bft_block_committed" | "bft_block_committed" | "bft_block" | "block_committed" => Some(Self::BftBlockCommitted),
            "on_bft_quorum_formed" | "bft_quorum_formed" | "bft_quorum" | "quorum_formed" => Some(Self::BftQuorumFormed),
            "on_bft_validator_slashed" | "bft_validator_slashed" | "validator_slashed" | "bft_slash" => Some(Self::BftValidatorSlashed),
            "on_bft_view_timeout" | "bft_view_timeout" | "view_timeout" | "pacemaker_timeout" => Some(Self::BftViewTimeout),
            "on_kernel_microstall_detected" | "kernel_microstall_detected" | "microstall_detected" | "microstall" => Some(Self::KernelMicroStallDetected),
            "on_realtime_priority_escalated" | "realtime_priority_escalated" | "priority_escalated" | "realtime_escalated" => Some(Self::RealtimePriorityEscalated),
            "on_irq_storm_shielded" | "irq_storm_shielded" | "irq_shielded" | "irq_storm" => Some(Self::IrqStormShielded),
            "on_jitter_threshold_exceeded" | "jitter_threshold_exceeded" | "jitter_exceeded" | "jitter_threshold" => Some(Self::JitterThresholdExceeded),
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
            Self::SdnMeshReconfigured,
            Self::SdnPacketDropped,
            Self::SdnCertRotated,
            Self::RaftLeaderElected,
            Self::RaftSplitBrainDetected,
            Self::RaftLockContended,
            Self::ResourceQuotaExceeded,
            Self::CgroupThrottled,
            Self::FairShareAdjusted,
            Self::TraceSpanRecorded,
            Self::OtlpExportFailed,
            Self::TraceSamplingSurge,
            Self::ChunkPrefetchCompleted,
            Self::AnvilCacheSaturated,
            Self::AnvilIoError,
            Self::NumaMigrationTriggered,
            Self::DpdkPacketFloodAlert,
            Self::CorePinningAdjusted,
            Self::RaftMembershipReconfigured,
            Self::RaftCompactionCompleted,
            Self::MultiRaftPartitionCreated,
            Self::LiveMigrationInitiated,
            Self::LiveMigrationFreezeStarted,
            Self::LiveMigrationCompleted,
            Self::LiveMigrationRolledBack,
            Self::EbpfProbeAttached,
            Self::JvmSafepointSpikeDetected,
            Self::GcPauseThresholdExceeded,
            Self::ThreadContentionSurge,
            Self::SupplyChainVerified,
            Self::SupplyChainViolationBlocked,
            Self::HermeticBuildCompleted,
            Self::PqcHandshakeCompleted,
            Self::PqcPolicyMigrated,
            Self::PqcDegradedFallbackDetected,
            Self::HsmTokenInserted,
            Self::EnclaveAttestationVerified,
            Self::ZkMembershipValidated,
            Self::MemoryCompactionCompleted,
            Self::HighMemoryFragmentationDetected,
            Self::ThpAllocationStallAlert,
            Self::XdpDdosAttackMitigated,
            Self::XdpFlowRateLimitExceeded,
            Self::XdpInterfaceAttached,
            Self::PmuHotspotDetected,
            Self::CacheMissThresholdExceeded,
            Self::BranchMispredictionSurge,
            Self::ShmChannelOpened,
            Self::ShmChannelClosed,
            Self::ShmLeaseExpired,
            Self::ShmThroughputThresholdExceeded,
            Self::PatchApplied,
            Self::PatchRolledBack,
            Self::PatchSafetyCheckFailed,
            Self::MicroVmSpawned,
            Self::MicroVmTerminated,
            Self::MicroVmIsolationAlert,
            Self::CrashTriageCompleted,
            Self::MemoryLeakDetected,
            Self::CriticalFaultRemediated,
            Self::RdmaLinkEstablished,
            Self::RdmaFailoverTriggered,
            Self::RdmaLatencySpike,
            Self::SmartNicOffloadInstalled,
            Self::SmartNicTcamSaturated,
            Self::SmartNicFallbackEngaged,
            Self::MemFabricPageFaultResolved,
            Self::MemFabricDimensionPaged,
            Self::MemFabricPoolSaturated,
            Self::NvmeNamespaceCreated,
            Self::NvmeNamespaceDeleted,
            Self::NvmeMultipathFailoverTriggered,
            Self::NvmePoolCapacityAlert,
            Self::VpnTunnelEstablished,
            Self::VpnKeyRotated,
            Self::VpnPeerConnected,
            Self::VpnSecurityDegradedAlert,
            Self::BftBlockCommitted,
            Self::BftQuorumFormed,
            Self::BftValidatorSlashed,
            Self::BftViewTimeout,
            Self::KernelMicroStallDetected,
            Self::RealtimePriorityEscalated,
            Self::IrqStormShielded,
            Self::JitterThresholdExceeded,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdn_peer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdn_zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped_packets: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_expires_in_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raft_term: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raft_leader_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raft_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fencing_token: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_current_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_throttled_usec: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_duration_micros: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_z: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefetch_radius: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefetched_chunks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_used_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_limit_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numa_node: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_cpus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpdk_pps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jitter_micros: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raft_group_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership_phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compacted_entries: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gc_phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safepoint_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contention_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slsa_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_reasons: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_migrated_pages: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_hugepages: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragmentation_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thp_alloc_stalls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xdp_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop_rate_pps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xdp_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmpi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bmpi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_segment_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_channel_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_messages_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_latency_ns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_target_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_apply_duration_micros: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_rejection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_vcpus: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_memory_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_cold_start_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_isolation_alert_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_report_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leak_candidate_callsite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaked_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdma_peer_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdma_transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdma_latency_nanos: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdma_bandwidth_gbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartnic_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartnic_rule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartnic_tcam_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartnic_offload_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_page_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_vaddr: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_dimension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_latency_nanos: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_pages_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_bytes_freed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_dram_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memfabric_nvram_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_nqn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_nsid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_iops: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_latency_micros: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_pool_utilization_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_tunnel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_crypto_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_throughput_gbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_renegotiation_micros: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bft_view: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bft_block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bft_validator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bft_slash_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bft_commit_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub microstall_duration_us: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_priority: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub irq_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolated_cpus: Option<String>,
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

    pub fn for_raft_membership(group_id: u64, phase: &str, node_id: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RaftMembershipReconfigured);
        ctx.raft_group_id = Some(group_id);
        ctx.membership_phase = Some(phase.to_string());
        ctx.details = Some(format!("Membership reconfigured for node '{}' in group {}", node_id, group_id));
        ctx
    }

    pub fn for_raft_compaction(group_id: u64, compacted_entries: u64, snapshot_bytes: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RaftCompactionCompleted);
        ctx.raft_group_id = Some(group_id);
        ctx.compacted_entries = Some(compacted_entries);
        ctx.snapshot_bytes = Some(snapshot_bytes);
        ctx.details = Some(format!("Compacted {} entries ({} bytes) in group {}", compacted_entries, snapshot_bytes, group_id));
        ctx
    }

    pub fn for_multiraft_partition(group_id: u64, partition_name: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MultiRaftPartitionCreated);
        ctx.raft_group_id = Some(group_id);
        ctx.partition_name = Some(partition_name.to_string());
        ctx.details = Some(format!("Partition '{}' (group {}) registered", partition_name, group_id));
        ctx
    }

    pub fn for_live_migration_initiated(
        migration_id: &str,
        server_name: &str,
        source_node: &str,
        target_node: &str,
    ) -> Self {
        let mut ctx = Self::new(LifecycleEvent::LiveMigrationInitiated);
        ctx.migration_id = Some(migration_id.to_string());
        ctx.server_name = Some(server_name.to_string());
        ctx.source_node = Some(source_node.to_string());
        ctx.target_node = Some(target_node.to_string());
        ctx.details = Some(format!(
            "Live migration '{}' initiated for server '{}' from '{}' to '{}'",
            migration_id, server_name, source_node, target_node
        ));
        ctx
    }

    pub fn for_live_migration_completed(
        migration_id: &str,
        server_name: &str,
        freeze_ms: u64,
        dirty_bytes: u64,
    ) -> Self {
        let mut ctx = Self::new(LifecycleEvent::LiveMigrationCompleted);
        ctx.migration_id = Some(migration_id.to_string());
        ctx.server_name = Some(server_name.to_string());
        ctx.freeze_ms = Some(freeze_ms);
        ctx.dirty_bytes = Some(dirty_bytes);
        ctx.details = Some(format!(
            "Live migration '{}' for server '{}' completed with {}ms freeze duration",
            migration_id, server_name, freeze_ms
        ));
        ctx
    }

    pub fn for_ebpf_probe_attached(server_name: &str, probe_id: &str, probe_type: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::EbpfProbeAttached);
        ctx.server_name = Some(server_name.to_string());
        ctx.probe_id = Some(probe_id.to_string());
        ctx.probe_type = Some(probe_type.to_string());
        ctx.details = Some(format!("eBPF probe '{}' attached to '{}' (type: {})", probe_id, server_name, probe_type));
        ctx
    }

    pub fn for_gc_pause_exceeded(server_name: &str, phase: &str, pause_ms: f64, safepoint_ms: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::GcPauseThresholdExceeded);
        ctx.server_name = Some(server_name.to_string());
        ctx.gc_phase = Some(phase.to_string());
        ctx.pause_ms = Some(pause_ms);
        ctx.safepoint_ms = Some(safepoint_ms);
        ctx.details = Some(format!("JVM GC pause exceeded threshold: {:.2}ms (phase: {}, safepoint: {:.2}ms)", pause_ms, phase, safepoint_ms));
        ctx
    }

    pub fn for_thread_contention(server_name: &str, lock_symbol: &str, contention_ms: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ThreadContentionSurge);
        ctx.server_name = Some(server_name.to_string());
        ctx.lock_symbol = Some(lock_symbol.to_string());
        ctx.contention_ms = Some(contention_ms);
        ctx.details = Some(format!("Thread lock contention surge on '{}': {:.2}ms", lock_symbol, contention_ms));
        ctx
    }

    pub fn for_memory_compaction(migrated: u64, hugepages: u64, frag_index: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MemoryCompactionCompleted);
        ctx.compaction_migrated_pages = Some(migrated);
        ctx.compaction_hugepages = Some(hugepages);
        ctx.fragmentation_index = Some(frag_index);
        ctx.details = Some(format!(
            "Memory compaction cycle coalesced {} pages into {} 2MB hugepages (frag index: {:.2})",
            migrated, hugepages, frag_index
        ));
        ctx
    }

    pub fn for_high_memory_fragmentation(frag_index: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::HighMemoryFragmentationDetected);
        ctx.fragmentation_index = Some(frag_index);
        ctx.details = Some(format!(
            "High memory fragmentation ratio detected: {:.2}",
            frag_index
        ));
        ctx
    }

    pub fn for_thp_allocation_stall(stalls: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ThpAllocationStallAlert);
        ctx.thp_alloc_stalls = Some(stalls);
        ctx.details = Some(format!(
            "Transparent hugepage allocation stall alert: {} stalls observed",
            stalls
        ));
        ctx
    }

    pub fn for_xdp_mitigation(attack_type: &str, src_ip: &str, pps: f64, action: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::XdpDdosAttackMitigated);
        ctx.attack_type = Some(attack_type.to_string());
        ctx.src_ip = Some(src_ip.to_string());
        ctx.drop_rate_pps = Some(pps);
        ctx.xdp_action = Some(action.to_string());
        ctx.details = Some(format!(
            "eBPF XDP anti-DDoS mitigation triggered: attack '{}' from IP '{}' dropped ({:.2} pps, action: {})",
            attack_type, src_ip, pps, action
        ));
        ctx
    }

    pub fn for_xdp_attached(iface: &str, mode: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::XdpInterfaceAttached);
        ctx.xdp_interface = Some(iface.to_string());
        ctx.details = Some(format!("eBPF XDP firewall attached to interface '{}' (mode: {})", iface, mode));
        ctx
    }

    pub fn for_pmu_hotspot(symbol: &str, percentage: f64, ipc: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::PmuHotspotDetected);
        ctx.hotspot_symbol = Some(symbol.to_string());
        ctx.ipc = Some(ipc);
        ctx.details = Some(format!(
            "Hardware PMU hotspot detected in '{}': {:.2}% sample share (IPC: {:.3})",
            symbol, percentage * 100.0, ipc
        ));
        ctx
    }

    pub fn for_cache_miss_threshold(cmpi: f64, threshold: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::CacheMissThresholdExceeded);
        ctx.cmpi = Some(cmpi);
        ctx.details = Some(format!(
            "Hardware PMU cache miss threshold exceeded: CMPI {:.6} > {:.6}",
            cmpi, threshold
        ));
        ctx
    }

    pub fn for_branch_misprediction(bmpi: f64, threshold: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::BranchMispredictionSurge);
        ctx.bmpi = Some(bmpi);
        ctx.details = Some(format!(
            "Hardware PMU branch misprediction surge: BMPI {:.6} > {:.6}",
            bmpi, threshold
        ));
        ctx
    }

    pub fn for_shm_channel_opened(segment: &str, channel: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ShmChannelOpened);
        ctx.shm_segment_name = Some(segment.to_string());
        ctx.shm_channel_type = Some(channel.to_string());
        ctx.details = Some(format!("POSIX shared memory channel '{}' opened ({})", segment, channel));
        ctx
    }

    pub fn for_shm_channel_closed(segment: &str, channel: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ShmChannelClosed);
        ctx.shm_segment_name = Some(segment.to_string());
        ctx.shm_channel_type = Some(channel.to_string());
        ctx.details = Some(format!("POSIX shared memory channel '{}' closed ({})", segment, channel));
        ctx
    }

    pub fn for_shm_lease_expired(segment: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ShmLeaseExpired);
        ctx.shm_segment_name = Some(segment.to_string());
        ctx.details = Some(format!("POSIX shared memory segment '{}' lease expired, reclaimed by watchdog", segment));
        ctx
    }

    pub fn for_shm_throughput(segment: &str, msgs_per_sec: f64, latency_ns: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::ShmThroughputThresholdExceeded);
        ctx.shm_segment_name = Some(segment.to_string());
        ctx.shm_messages_per_sec = Some(msgs_per_sec);
        ctx.shm_latency_ns = Some(latency_ns);
        ctx.details = Some(format!(
            "Shared memory channel '{}' throughput: {:.1} msgs/sec, latency: {:.2} ns",
            segment, msgs_per_sec, latency_ns
        ));
        ctx
    }

    pub fn with_shm_context(mut self, segment: &str, channel: &str, msgs_per_sec: f64, latency_ns: f64) -> Self {
        self.shm_segment_name = Some(segment.to_string());
        self.shm_channel_type = Some(channel.to_string());
        self.shm_messages_per_sec = Some(msgs_per_sec);
        self.shm_latency_ns = Some(latency_ns);
        self
    }

    pub fn for_patch_applied(patch_name: &str, target_symbol: &str, duration_micros: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::PatchApplied);
        ctx.patch_name = Some(patch_name.to_string());
        ctx.patch_target_symbol = Some(target_symbol.to_string());
        ctx.patch_apply_duration_micros = Some(duration_micros);
        ctx.details = Some(format!(
            "Dynamic patch '{}' applied to symbol '{}' in {} us",
            patch_name, target_symbol, duration_micros
        ));
        ctx
    }

    pub fn for_patch_rolled_back(patch_name: &str, target_symbol: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::PatchRolledBack);
        ctx.patch_name = Some(patch_name.to_string());
        ctx.patch_target_symbol = Some(target_symbol.to_string());
        ctx.details = Some(format!(
            "Dynamic patch '{}' rolled back from symbol '{}'",
            patch_name, target_symbol
        ));
        ctx
    }

    pub fn for_patch_safety_failed(patch_name: &str, target_symbol: &str, reason: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::PatchSafetyCheckFailed);
        ctx.patch_name = Some(patch_name.to_string());
        ctx.patch_target_symbol = Some(target_symbol.to_string());
        ctx.patch_rejection_reason = Some(reason.to_string());
        ctx.details = Some(format!(
            "Dynamic patch '{}' on symbol '{}' rejected by CFG dominance check: {}",
            patch_name, target_symbol, reason
        ));
        ctx
    }

    pub fn for_vm_spawned(vm_id: &str, vm_name: &str, vcpus: u32, memory_mb: u64, cold_start_ms: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MicroVmSpawned);
        ctx.vm_id = Some(vm_id.to_string());
        ctx.vm_name = Some(vm_name.to_string());
        ctx.vm_vcpus = Some(vcpus);
        ctx.vm_memory_mb = Some(memory_mb);
        ctx.vm_cold_start_ms = Some(cold_start_ms);
        ctx.details = Some(format!(
            "MicroVM '{}' ({}) spawned with {} vCPUs, {} MB in {:.2} ms",
            vm_name, vm_id, vcpus, memory_mb, cold_start_ms
        ));
        ctx
    }

    pub fn for_vm_terminated(vm_id: &str, vm_name: &str, uptime_seconds: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MicroVmTerminated);
        ctx.vm_id = Some(vm_id.to_string());
        ctx.vm_name = Some(vm_name.to_string());
        ctx.details = Some(format!(
            "MicroVM '{}' ({}) terminated after {}s uptime",
            vm_name, vm_id, uptime_seconds
        ));
        ctx
    }

    pub fn for_vm_isolation_alert(vm_id: &str, reason: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MicroVmIsolationAlert);
        ctx.vm_id = Some(vm_id.to_string());
        ctx.vm_isolation_alert_reason = Some(reason.to_string());
        ctx.details = Some(format!(
            "MicroVM '{}' isolation alert: {}",
            vm_id, reason
        ));
        ctx
    }

    pub fn for_crash_triage(report_id: &str, crash_type: &str, severity: &str, root_cause: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::CrashTriageCompleted);
        ctx.crash_report_id = Some(report_id.to_string());
        ctx.crash_type = Some(crash_type.to_string());
        ctx.crash_severity = Some(severity.to_string());
        ctx.root_cause = Some(root_cause.to_string());
        ctx.details = Some(format!(
            "Crash report '{}' triaged: {} ({}) - {}",
            report_id, crash_type, severity, root_cause
        ));
        ctx
    }

    pub fn for_memory_leak(callsite: &str, leaked_bytes: u64, root_cause: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MemoryLeakDetected);
        ctx.leak_candidate_callsite = Some(callsite.to_string());
        ctx.leaked_bytes = Some(leaked_bytes);
        ctx.root_cause = Some(root_cause.to_string());
        ctx.details = Some(format!(
            "Memory leak detected at {}: {} bytes leaked ({})",
            callsite, leaked_bytes, root_cause
        ));
        ctx
    }

    pub fn for_critical_fault_remediated(report_id: &str, action: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::CriticalFaultRemediated);
        ctx.crash_report_id = Some(report_id.to_string());
        ctx.remediation_status = Some(action.to_string());
        ctx.details = Some(format!(
            "Critical crash fault '{}' remediated with action: {}",
            report_id, action
        ));
        ctx
    }

    pub fn for_rdma_link_established(peer_node: &str, transport: &str, bandwidth_gbps: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RdmaLinkEstablished);
        ctx.rdma_peer_node = Some(peer_node.to_string());
        ctx.rdma_transport = Some(transport.to_string());
        ctx.rdma_bandwidth_gbps = Some(bandwidth_gbps);
        ctx.details = Some(format!(
            "RDMA link established with peer '{}' via {} ({:.1} Gbps)",
            peer_node, transport, bandwidth_gbps
        ));
        ctx
    }

    pub fn for_rdma_failover(peer_node: &str, reason: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RdmaFailoverTriggered);
        ctx.rdma_peer_node = Some(peer_node.to_string());
        ctx.details = Some(format!(
            "RDMA failover to TCP fallback triggered for peer '{}': {}",
            peer_node, reason
        ));
        ctx
    }

    pub fn for_rdma_latency_spike(peer_node: &str, latency_nanos: u64, threshold_nanos: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RdmaLatencySpike);
        ctx.rdma_peer_node = Some(peer_node.to_string());
        ctx.rdma_latency_nanos = Some(latency_nanos);
        ctx.details = Some(format!(
            "RDMA transfer latency spike on peer '{}': {} ns > {} ns threshold",
            peer_node, latency_nanos, threshold_nanos
        ));
        ctx
    }

    pub fn for_smartnic_offload_installed(device_id: &str, rule_id: &str, mode: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::SmartNicOffloadInstalled);
        ctx.smartnic_device_id = Some(device_id.to_string());
        ctx.smartnic_rule_id = Some(rule_id.to_string());
        ctx.smartnic_offload_mode = Some(mode.to_string());
        ctx.details = Some(format!(
            "SmartNIC offload rule '{}' installed on device '{}' in mode '{}'",
            rule_id, device_id, mode
        ));
        ctx
    }

    pub fn for_smartnic_tcam_saturated(device_id: &str, tcam_percent: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::SmartNicTcamSaturated);
        ctx.smartnic_device_id = Some(device_id.to_string());
        ctx.smartnic_tcam_percent = Some(tcam_percent);
        ctx.details = Some(format!(
            "SmartNIC device '{}' TCAM capacity saturated at {:.1}%",
            device_id, tcam_percent
        ));
        ctx
    }

    pub fn for_smartnic_fallback_engaged(device_id: &str, fallback_mode: &str, reason: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::SmartNicFallbackEngaged);
        ctx.smartnic_device_id = Some(device_id.to_string());
        ctx.smartnic_offload_mode = Some(fallback_mode.to_string());
        ctx.details = Some(format!(
            "SmartNIC fallback engaged on '{}' -> {}: {}",
            device_id, fallback_mode, reason
        ));
        ctx
    }

    pub fn for_memfabric_page_fault(
        page_id: &str,
        vaddr: u64,
        latency_nanos: u64,
        node_id: &str,
    ) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MemFabricPageFaultResolved);
        ctx.memfabric_page_id = Some(page_id.to_string());
        ctx.memfabric_vaddr = Some(vaddr);
        ctx.memfabric_latency_nanos = Some(latency_nanos);
        ctx.target_node = Some(node_id.to_string());
        ctx.details = Some(format!(
            "MemFabric resolved userfaultfd page fault for '{}' (0x{:x}) in {} ns via node '{}'",
            page_id, vaddr, latency_nanos, node_id
        ));
        ctx
    }

    pub fn for_memfabric_dimension_paged(
        dimension: &str,
        pages_evicted: usize,
        bytes_freed: u64,
        target_node: &str,
    ) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MemFabricDimensionPaged);
        ctx.memfabric_dimension = Some(dimension.to_string());
        ctx.memfabric_pages_count = Some(pages_evicted);
        ctx.memfabric_bytes_freed = Some(bytes_freed);
        ctx.target_node = Some(target_node.to_string());
        ctx.details = Some(format!(
            "MemFabric evicted dimension '{}' ({} pages, {} bytes freed) to NVRAM node '{}'",
            dimension, pages_evicted, bytes_freed, target_node
        ));
        ctx
    }

    pub fn for_memfabric_pool_saturated(dram_util_pct: f64, nvram_util_pct: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::MemFabricPoolSaturated);
        ctx.memfabric_dram_percent = Some(dram_util_pct);
        ctx.memfabric_nvram_percent = Some(nvram_util_pct);
        ctx.details = Some(format!(
            "MemFabric cluster pool saturated: DRAM at {:.1}%, NVRAM at {:.1}%",
            dram_util_pct, nvram_util_pct
        ));
        ctx
    }

    pub fn for_nvme_namespace_created(
        nsid: u32,
        size_mb: u64,
        server_id: Option<&str>,
        dimension: Option<&str>,
    ) -> Self {
        let mut ctx = Self::new(LifecycleEvent::NvmeNamespaceCreated);
        ctx.nvme_nsid = Some(nsid);
        ctx.server_name = server_id.map(|s| s.to_string());
        ctx.details = Some(format!(
            "NVMe storage namespace NSID {} created ({} MB, server: {:?}, dimension: {:?})",
            nsid, size_mb, server_id, dimension
        ));
        ctx
    }

    pub fn for_nvme_namespace_deleted(nsid: u32) -> Self {
        let mut ctx = Self::new(LifecycleEvent::NvmeNamespaceDeleted);
        ctx.nvme_nsid = Some(nsid);
        ctx.details = Some(format!("NVMe storage namespace NSID {} deleted from flash pool", nsid));
        ctx
    }

    pub fn for_nvme_multipath_failover(nqn: &str, active_tr: &str, prev_tr: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::NvmeMultipathFailoverTriggered);
        ctx.nvme_nqn = Some(nqn.to_string());
        ctx.nvme_transport = Some(active_tr.to_string());
        ctx.details = Some(format!(
            "NVMe multipath failover triggered on '{}': switched from {} to {}",
            nqn, prev_tr, active_tr
        ));
        ctx
    }

    pub fn for_nvme_pool_capacity_alert(util_pct: f64, allocated_bytes: u64, total_bytes: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::NvmePoolCapacityAlert);
        ctx.nvme_pool_utilization_percent = Some(util_pct);
        ctx.details = Some(format!(
            "NVMe flash block pool capacity alert: {:.1}% utilized ({} / {} bytes)",
            util_pct, allocated_bytes, total_bytes
        ));
        ctx
    }

    pub fn for_vpn_tunnel_established(tunnel_id: &str, crypto_mode: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::VpnTunnelEstablished);
        ctx.vpn_tunnel_id = Some(tunnel_id.to_string());
        ctx.vpn_crypto_mode = Some(crypto_mode.to_string());
        ctx.details = Some(format!(
            "WireGuard PQXDH VPN tunnel '{}' established with cipher mode {}",
            tunnel_id, crypto_mode
        ));
        ctx
    }

    pub fn for_vpn_key_rotated(tunnel_id: &str, peer_id: &str, reneg_micros: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::VpnKeyRotated);
        ctx.vpn_tunnel_id = Some(tunnel_id.to_string());
        ctx.vpn_peer_id = Some(peer_id.to_string());
        ctx.vpn_renegotiation_micros = Some(reneg_micros);
        ctx.details = Some(format!(
            "Zero-loss PQXDH key rotated for tunnel '{}' peer '{}' in {} us",
            tunnel_id, peer_id, reneg_micros
        ));
        ctx
    }

    pub fn for_vpn_peer_connected(tunnel_id: &str, peer_id: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::VpnPeerConnected);
        ctx.vpn_tunnel_id = Some(tunnel_id.to_string());
        ctx.vpn_peer_id = Some(peer_id.to_string());
        ctx.details = Some(format!(
            "WireGuard PQXDH mesh peer '{}' connected to tunnel '{}'",
            peer_id, tunnel_id
        ));
        ctx
    }

    pub fn for_vpn_security_degraded(tunnel_id: &str, reason: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::VpnSecurityDegradedAlert);
        ctx.vpn_tunnel_id = Some(tunnel_id.to_string());
        ctx.details = Some(format!(
            "VPN security degraded alert on tunnel '{}': {}",
            tunnel_id, reason
        ));
        ctx
    }

    pub fn for_bft_block_committed(view: u64, block_hash: &str, latency_ms: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::BftBlockCommitted);
        ctx.bft_view = Some(view);
        ctx.bft_block_hash = Some(block_hash.to_string());
        ctx.bft_commit_latency_ms = Some(latency_ms);
        ctx.details = Some(format!(
            "BFT consensus block {} committed at view {} in {:.2}ms",
            block_hash, view, latency_ms
        ));
        ctx
    }

    pub fn for_bft_quorum_formed(view: u64, block_hash: &str, signers_count: usize) -> Self {
        let mut ctx = Self::new(LifecycleEvent::BftQuorumFormed);
        ctx.bft_view = Some(view);
        ctx.bft_block_hash = Some(block_hash.to_string());
        ctx.details = Some(format!(
            "BFT 2f+1 Quorum Certificate formed for view {} block {} with {} signers",
            view, block_hash, signers_count
        ));
        ctx
    }

    pub fn for_bft_validator_slashed(validator_id: &str, reason: &str, view: u64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::BftValidatorSlashed);
        ctx.bft_validator_id = Some(validator_id.to_string());
        ctx.bft_slash_reason = Some(reason.to_string());
        ctx.bft_view = Some(view);
        ctx.details = Some(format!(
            "BFT validator '{}' slashed at view {}: {}",
            validator_id, view, reason
        ));
        ctx
    }

    pub fn for_bft_view_timeout(view: u64, old_leader: &str, new_leader: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::BftViewTimeout);
        ctx.bft_view = Some(view);
        ctx.details = Some(format!(
            "BFT view pacemaker timeout at view {}: leader rotated from '{}' to '{}'",
            view, old_leader, new_leader
        ));
        ctx
    }

    pub fn for_kernel_microstall_detected(server: &str, duration_us: u64, cause: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::KernelMicroStallDetected);
        ctx.server_name = Some(server.to_string());
        ctx.microstall_duration_us = Some(duration_us);
        ctx.details = Some(format!(
            "Kernel micro-stall detected on server '{}': {}us caused by {}",
            server, duration_us, cause
        ));
        ctx
    }

    pub fn for_realtime_priority_escalated(server: &str, priority: u32, cores: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::RealtimePriorityEscalated);
        ctx.server_name = Some(server.to_string());
        ctx.realtime_priority = Some(priority);
        ctx.isolated_cpus = Some(cores.to_string());
        ctx.details = Some(format!(
            "Real-time SCHED_FIFO priority {} escalated for server '{}' isolated on cores {}",
            priority, server, cores
        ));
        ctx
    }

    pub fn for_irq_storm_shielded(irq: u32, irq_name: &str, target_cpus: &str) -> Self {
        let mut ctx = Self::new(LifecycleEvent::IrqStormShielded);
        ctx.irq_number = Some(irq);
        ctx.isolated_cpus = Some(target_cpus.to_string());
        ctx.details = Some(format!(
            "Hardware IRQ storm {} ({}) shielded away to housekeeping cores {}",
            irq, irq_name, target_cpus
        ));
        ctx
    }

    pub fn for_jitter_threshold_exceeded(server: &str, jitter_us: f64, threshold_us: f64) -> Self {
        let mut ctx = Self::new(LifecycleEvent::JitterThresholdExceeded);
        ctx.server_name = Some(server.to_string());
        ctx.jitter_micros = Some(jitter_us);
        ctx.details = Some(format!(
            "Kernel jitter threshold exceeded on server '{}': {:.2}us > {:.2}us",
            server, jitter_us, threshold_us
        ));
        ctx
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
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    Self::dispatch(&paths, event, &context, timeout_secs);
                })
                .await;
            });
        } else {
            std::thread::spawn(move || {
                Self::dispatch(&paths, event, &context, timeout_secs);
            });
        }
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

    #[test]
    fn test_sdn_lifecycle_events_and_context() {
        assert_eq!(LifecycleEvent::from_name("on_sdn_mesh_reconfigured"), Some(LifecycleEvent::SdnMeshReconfigured));
        assert_eq!(LifecycleEvent::from_name("sdn_reconfigure"), Some(LifecycleEvent::SdnMeshReconfigured));
        assert_eq!(LifecycleEvent::from_name("on_sdn_packet_dropped"), Some(LifecycleEvent::SdnPacketDropped));
        assert_eq!(LifecycleEvent::from_name("packet_drop"), Some(LifecycleEvent::SdnPacketDropped));
        assert_eq!(LifecycleEvent::from_name("on_sdn_cert_rotated"), Some(LifecycleEvent::SdnCertRotated));
        assert_eq!(LifecycleEvent::from_name("cert_rotated"), Some(LifecycleEvent::SdnCertRotated));

        let mut ctx = HookContext::new(LifecycleEvent::SdnPacketDropped);
        ctx.sdn_peer_name = Some("lobby-eu".to_string());
        ctx.sdn_zone = Some("BackendWorld".to_string());
        ctx.dropped_packets = Some(142);
        ctx.cert_expires_in_days = Some(89);

        assert_eq!(ctx.event, "on_sdn_packet_dropped");
        assert_eq!(ctx.sdn_peer_name.as_deref(), Some("lobby-eu"));
        assert_eq!(ctx.sdn_zone.as_deref(), Some("BackendWorld"));
        assert_eq!(ctx.dropped_packets, Some(142));
        assert_eq!(ctx.cert_expires_in_days, Some(89));
    }

    #[test]
    fn test_raft_lifecycle_events_and_context() {
        assert_eq!(
            LifecycleEvent::from_name("on_raft_leader_elected"),
            Some(LifecycleEvent::RaftLeaderElected)
        );
        assert_eq!(
            LifecycleEvent::from_name("leader_elected"),
            Some(LifecycleEvent::RaftLeaderElected)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_raft_split_brain_detected"),
            Some(LifecycleEvent::RaftSplitBrainDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("split_brain"),
            Some(LifecycleEvent::RaftSplitBrainDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_raft_lock_contended"),
            Some(LifecycleEvent::RaftLockContended)
        );
        assert_eq!(
            LifecycleEvent::from_name("lock_contended"),
            Some(LifecycleEvent::RaftLockContended)
        );

        let mut ctx = HookContext::new(LifecycleEvent::RaftLeaderElected);
        ctx.raft_term = Some(3);
        ctx.raft_leader_id = Some("node-primary".to_string());
        ctx.raft_role = Some("Leader".to_string());
        ctx.lock_name = Some("global-lock".to_string());
        ctx.fencing_token = Some(12884901889);

        assert_eq!(ctx.event, "on_raft_leader_elected");
        assert_eq!(ctx.raft_term, Some(3));
        assert_eq!(ctx.raft_leader_id.as_deref(), Some("node-primary"));
        assert_eq!(ctx.raft_role.as_deref(), Some("Leader"));
        assert_eq!(ctx.lock_name.as_deref(), Some("global-lock"));
        assert_eq!(ctx.fencing_token, Some(12884901889));
    }

    #[test]
    fn test_tracing_lifecycle_events_and_context() {
        assert_eq!(
            LifecycleEvent::from_name("on_trace_span_recorded"),
            Some(LifecycleEvent::TraceSpanRecorded)
        );
        assert_eq!(
            LifecycleEvent::from_name("span_recorded"),
            Some(LifecycleEvent::TraceSpanRecorded)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_otlp_export_failed"),
            Some(LifecycleEvent::OtlpExportFailed)
        );
        assert_eq!(
            LifecycleEvent::from_name("export_failed"),
            Some(LifecycleEvent::OtlpExportFailed)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_trace_sampling_surge"),
            Some(LifecycleEvent::TraceSamplingSurge)
        );

        let mut ctx = HookContext::new(LifecycleEvent::TraceSpanRecorded);
        ctx.trace_id = Some("4bf92f3577b34da6a3ce929d0e0e4736".to_string());
        ctx.span_id = Some("00f067aa0ba902b7".to_string());
        ctx.span_name = Some("craft.server.tick".to_string());
        ctx.span_duration_micros = Some(45000);
        ctx.span_status = Some("Error".to_string());

        assert_eq!(ctx.event, "on_trace_span_recorded");
        assert_eq!(ctx.trace_id.as_deref(), Some("4bf92f3577b34da6a3ce929d0e0e4736"));
        assert_eq!(ctx.span_id.as_deref(), Some("00f067aa0ba902b7"));
        assert_eq!(ctx.span_name.as_deref(), Some("craft.server.tick"));
        assert_eq!(ctx.span_duration_micros, Some(45000));
        assert_eq!(ctx.span_status.as_deref(), Some("Error"));
    }

    #[test]
    fn test_anvil_lifecycle_events_and_context() {
        assert_eq!(
            LifecycleEvent::from_name("on_chunk_prefetch_completed"),
            Some(LifecycleEvent::ChunkPrefetchCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("prefetch_completed"),
            Some(LifecycleEvent::ChunkPrefetchCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_anvil_cache_saturated"),
            Some(LifecycleEvent::AnvilCacheSaturated)
        );
        assert_eq!(
            LifecycleEvent::from_name("cache_saturated"),
            Some(LifecycleEvent::AnvilCacheSaturated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_anvil_io_error"),
            Some(LifecycleEvent::AnvilIoError)
        );

        let mut ctx = HookContext::new(LifecycleEvent::ChunkPrefetchCompleted);
        ctx.chunk_x = Some(10);
        ctx.chunk_z = Some(-5);
        ctx.prefetch_radius = Some(4);
        ctx.prefetched_chunks = Some(49);
        ctx.cache_used_bytes = Some(1048576);
        ctx.cache_limit_bytes = Some(67108864);

        assert_eq!(ctx.event, "on_chunk_prefetch_completed");
        assert_eq!(ctx.chunk_x, Some(10));
        assert_eq!(ctx.chunk_z, Some(-5));
        assert_eq!(ctx.prefetch_radius, Some(4));
        assert_eq!(ctx.prefetched_chunks, Some(49));
        assert_eq!(ctx.cache_used_bytes, Some(1048576));
        assert_eq!(ctx.cache_limit_bytes, Some(67108864));
    }

    #[test]
    fn test_numa_dpdk_lifecycle_events_and_context() {
        assert_eq!(
            LifecycleEvent::from_name("on_numa_migration_triggered"),
            Some(LifecycleEvent::NumaMigrationTriggered)
        );
        assert_eq!(
            LifecycleEvent::from_name("numa_migration"),
            Some(LifecycleEvent::NumaMigrationTriggered)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_dpdk_packet_flood_alert"),
            Some(LifecycleEvent::DpdkPacketFloodAlert)
        );
        assert_eq!(
            LifecycleEvent::from_name("dpdk_flood"),
            Some(LifecycleEvent::DpdkPacketFloodAlert)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_core_pinning_adjusted"),
            Some(LifecycleEvent::CorePinningAdjusted)
        );
        assert_eq!(
            LifecycleEvent::from_name("pinning_adjusted"),
            Some(LifecycleEvent::CorePinningAdjusted)
        );

        let mut ctx = HookContext::new(LifecycleEvent::NumaMigrationTriggered);
        ctx.numa_node = Some(1);
        ctx.pinned_cpus = Some("4-7".to_string());
        ctx.dpdk_pps = Some(1250000);
        ctx.jitter_micros = Some(0.42);

        assert_eq!(ctx.event, "on_numa_migration_triggered");
        assert_eq!(ctx.numa_node, Some(1));
        assert_eq!(ctx.pinned_cpus.as_deref(), Some("4-7"));
        assert_eq!(ctx.dpdk_pps, Some(1250000));
        assert_eq!(ctx.jitter_micros, Some(0.42));

        // Test Phase 29 Multi-Raft lifecycle events
        assert_eq!(
            LifecycleEvent::from_name("on_raft_membership_reconfigured"),
            Some(LifecycleEvent::RaftMembershipReconfigured)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_raft_compaction_completed"),
            Some(LifecycleEvent::RaftCompactionCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_multiraft_partition_created"),
            Some(LifecycleEvent::MultiRaftPartitionCreated)
        );

        let raft_ctx = HookContext::for_raft_compaction(100, 500, 10240);
        assert_eq!(raft_ctx.event, "on_raft_compaction_completed");
        assert_eq!(raft_ctx.raft_group_id, Some(100));
        assert_eq!(raft_ctx.compacted_entries, Some(500));
        assert_eq!(raft_ctx.snapshot_bytes, Some(10240));

        // Test Phase 30 Live Migration lifecycle events
        assert_eq!(
            LifecycleEvent::from_name("on_live_migration_initiated"),
            Some(LifecycleEvent::LiveMigrationInitiated)
        );
        assert_eq!(
            LifecycleEvent::from_name("live_migration_initiated"),
            Some(LifecycleEvent::LiveMigrationInitiated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_live_migration_freeze_started"),
            Some(LifecycleEvent::LiveMigrationFreezeStarted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_live_migration_completed"),
            Some(LifecycleEvent::LiveMigrationCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_live_migration_rolled_back"),
            Some(LifecycleEvent::LiveMigrationRolledBack)
        );

        let mig_ctx = HookContext::for_live_migration_completed("mig-123", "survival", 42, 10485760);
        assert_eq!(mig_ctx.event, "on_live_migration_completed");
        assert_eq!(mig_ctx.migration_id.as_deref(), Some("mig-123"));
        assert_eq!(mig_ctx.server_name.as_deref(), Some("survival"));
        assert_eq!(mig_ctx.freeze_ms, Some(42));
        assert_eq!(mig_ctx.dirty_bytes, Some(10485760));

        // Test Phase 31 eBPF & JVM GC lifecycle events
        assert_eq!(
            LifecycleEvent::from_name("on_ebpf_probe_attached"),
            Some(LifecycleEvent::EbpfProbeAttached)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_jvm_safepoint_spike_detected"),
            Some(LifecycleEvent::JvmSafepointSpikeDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_gc_pause_threshold_exceeded"),
            Some(LifecycleEvent::GcPauseThresholdExceeded)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_thread_contention_surge"),
            Some(LifecycleEvent::ThreadContentionSurge)
        );

        let ebpf_ctx = HookContext::for_ebpf_probe_attached("hub", "probe-abc", "syscall_read");
        assert_eq!(ebpf_ctx.event, "on_ebpf_probe_attached");
        assert_eq!(ebpf_ctx.probe_id.as_deref(), Some("probe-abc"));
        assert_eq!(ebpf_ctx.probe_type.as_deref(), Some("syscall_read"));

        let gc_ctx = HookContext::for_gc_pause_exceeded("hub", "young_gen", 45.2, 5.1);
        assert_eq!(gc_ctx.event, "on_gc_pause_threshold_exceeded");
        assert_eq!(gc_ctx.pause_ms, Some(45.2));
        assert_eq!(gc_ctx.safepoint_ms, Some(5.1));

        let lock_ctx = HookContext::for_thread_contention("hub", "MinecraftServer.tick()", 18.5);
        assert_eq!(lock_ctx.event, "on_thread_contention_surge");
        assert_eq!(lock_ctx.lock_symbol.as_deref(), Some("MinecraftServer.tick()"));
        assert_eq!(lock_ctx.contention_ms, Some(18.5));

        // Test Phase 36 XDP Firewall lifecycle events
        assert_eq!(
            LifecycleEvent::from_name("on_xdp_ddos_attack_mitigated"),
            Some(LifecycleEvent::XdpDdosAttackMitigated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_xdp_flow_rate_limit_exceeded"),
            Some(LifecycleEvent::XdpFlowRateLimitExceeded)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_xdp_interface_attached"),
            Some(LifecycleEvent::XdpInterfaceAttached)
        );

        let xdp_ctx = HookContext::for_xdp_mitigation("syn_flood", "198.51.100.42", 50000.0, "XDP_DROP");
        assert_eq!(xdp_ctx.event, "on_xdp_ddos_attack_mitigated");
        assert_eq!(xdp_ctx.attack_type.as_deref(), Some("syn_flood"));
        assert_eq!(xdp_ctx.src_ip.as_deref(), Some("198.51.100.42"));
        assert_eq!(xdp_ctx.drop_rate_pps, Some(50000.0));
        assert_eq!(xdp_ctx.xdp_action.as_deref(), Some("XDP_DROP"));
    }

    #[test]
    fn test_microvm_lifecycle_events_and_context() {
        assert_eq!(
            LifecycleEvent::from_name("on_microvm_spawned"),
            Some(LifecycleEvent::MicroVmSpawned)
        );
        assert_eq!(
            LifecycleEvent::from_name("vm_spawned"),
            Some(LifecycleEvent::MicroVmSpawned)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_microvm_terminated"),
            Some(LifecycleEvent::MicroVmTerminated)
        );
        assert_eq!(
            LifecycleEvent::from_name("vm_stop"),
            Some(LifecycleEvent::MicroVmTerminated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_microvm_isolation_alert"),
            Some(LifecycleEvent::MicroVmIsolationAlert)
        );

        let spawn_ctx = HookContext::for_vm_spawned("vm-101", "sandbox-alpha", 2, 512, 18.5);
        assert_eq!(spawn_ctx.event, "on_microvm_spawned");
        assert_eq!(spawn_ctx.vm_id.as_deref(), Some("vm-101"));
        assert_eq!(spawn_ctx.vm_name.as_deref(), Some("sandbox-alpha"));
        assert_eq!(spawn_ctx.vm_vcpus, Some(2));
        assert_eq!(spawn_ctx.vm_memory_mb, Some(512));
        assert_eq!(spawn_ctx.vm_cold_start_ms, Some(18.5));

        let term_ctx = HookContext::for_vm_terminated("vm-101", "sandbox-alpha", 120);
        assert_eq!(term_ctx.event, "on_microvm_terminated");
        assert_eq!(term_ctx.vm_id.as_deref(), Some("vm-101"));

        let alert_ctx = HookContext::for_vm_isolation_alert("vm-101", "Unmapped MMIO write attempted at 0xdeadbeef");
        assert_eq!(alert_ctx.event, "on_microvm_isolation_alert");
        assert_eq!(alert_ctx.vm_id.as_deref(), Some("vm-101"));
        assert_eq!(
            alert_ctx.vm_isolation_alert_reason.as_deref(),
            Some("Unmapped MMIO write attempted at 0xdeadbeef")
        );
    }

    #[test]
    fn test_crash_triage_lifecycle_events() {
        assert_eq!(LifecycleEvent::CrashTriageCompleted.as_str(), "on_crash_triage_completed");
        assert_eq!(LifecycleEvent::MemoryLeakDetected.as_str(), "on_memory_leak_detected");
        assert_eq!(LifecycleEvent::CriticalFaultRemediated.as_str(), "on_critical_fault_remediated");

        assert_eq!(
            LifecycleEvent::from_name("on_crash_triage_completed"),
            Some(LifecycleEvent::CrashTriageCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("crash_triage"),
            Some(LifecycleEvent::CrashTriageCompleted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_memory_leak_detected"),
            Some(LifecycleEvent::MemoryLeakDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("leak"),
            Some(LifecycleEvent::MemoryLeakDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_critical_fault_remediated"),
            Some(LifecycleEvent::CriticalFaultRemediated)
        );

        let triage_ctx = HookContext::for_crash_triage(
            "report-41",
            "JvmCrash",
            "Critical",
            "JVM fatal error in C2 CompilerThread: SIGSEGV",
        );
        assert_eq!(triage_ctx.event, "on_crash_triage_completed");
        assert_eq!(triage_ctx.crash_report_id.as_deref(), Some("report-41"));
        assert_eq!(triage_ctx.crash_type.as_deref(), Some("JvmCrash"));
        assert_eq!(triage_ctx.crash_severity.as_deref(), Some("Critical"));

        let leak_ctx = HookContext::for_memory_leak("world::load_chunks", 10485760, "Chunk leak");
        assert_eq!(leak_ctx.event, "on_memory_leak_detected");
        assert_eq!(leak_ctx.leak_candidate_callsite.as_deref(), Some("world::load_chunks"));
        assert_eq!(leak_ctx.leaked_bytes, Some(10485760));

        let rem_ctx = HookContext::for_critical_fault_remediated("report-41", "ApplyHotfix");
        assert_eq!(rem_ctx.event, "on_critical_fault_remediated");
        assert_eq!(rem_ctx.remediation_status.as_deref(), Some("ApplyHotfix"));
    }

    #[test]
    fn test_rdma_lifecycle_events() {
        assert_eq!(LifecycleEvent::RdmaLinkEstablished.as_str(), "on_rdma_link_established");
        assert_eq!(LifecycleEvent::RdmaFailoverTriggered.as_str(), "on_rdma_failover_triggered");
        assert_eq!(LifecycleEvent::RdmaLatencySpike.as_str(), "on_rdma_latency_spike");

        assert_eq!(
            LifecycleEvent::from_name("on_rdma_link_established"),
            Some(LifecycleEvent::RdmaLinkEstablished)
        );
        assert_eq!(
            LifecycleEvent::from_name("rdma_link"),
            Some(LifecycleEvent::RdmaLinkEstablished)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_rdma_failover_triggered"),
            Some(LifecycleEvent::RdmaFailoverTriggered)
        );
        assert_eq!(
            LifecycleEvent::from_name("rdma_failover"),
            Some(LifecycleEvent::RdmaFailoverTriggered)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_rdma_latency_spike"),
            Some(LifecycleEvent::RdmaLatencySpike)
        );
        assert_eq!(
            LifecycleEvent::from_name("rdma_spike"),
            Some(LifecycleEvent::RdmaLatencySpike)
        );

        let link_ctx = HookContext::for_rdma_link_established("node-2", "RoCEv2", 100.0);
        assert_eq!(link_ctx.event, "on_rdma_link_established");
        assert_eq!(link_ctx.rdma_peer_node.as_deref(), Some("node-2"));
        assert_eq!(link_ctx.rdma_transport.as_deref(), Some("RoCEv2"));
        assert_eq!(link_ctx.rdma_bandwidth_gbps, Some(100.0));

        let failover_ctx = HookContext::for_rdma_failover("node-2", "Excessive CRC errors");
        assert_eq!(failover_ctx.event, "on_rdma_failover_triggered");
        assert_eq!(failover_ctx.rdma_peer_node.as_deref(), Some("node-2"));
        assert!(failover_ctx.details.unwrap().contains("Excessive CRC errors"));

        let spike_ctx = HookContext::for_rdma_latency_spike("node-2", 4500, 2000);
        assert_eq!(spike_ctx.event, "on_rdma_latency_spike");
        assert_eq!(spike_ctx.rdma_peer_node.as_deref(), Some("node-2"));
        assert_eq!(spike_ctx.rdma_latency_nanos, Some(4500));
    }

    #[test]
    fn test_smartnic_lifecycle_events() {
        assert_eq!(
            LifecycleEvent::from_name("on_smartnic_offload_installed"),
            Some(LifecycleEvent::SmartNicOffloadInstalled)
        );
        assert_eq!(
            LifecycleEvent::from_name("smartnic_installed"),
            Some(LifecycleEvent::SmartNicOffloadInstalled)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_smartnic_tcam_saturated"),
            Some(LifecycleEvent::SmartNicTcamSaturated)
        );
        assert_eq!(
            LifecycleEvent::from_name("tcam_saturated"),
            Some(LifecycleEvent::SmartNicTcamSaturated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_smartnic_fallback_engaged"),
            Some(LifecycleEvent::SmartNicFallbackEngaged)
        );
        assert_eq!(
            LifecycleEvent::from_name("smartnic_fallback"),
            Some(LifecycleEvent::SmartNicFallbackEngaged)
        );

        let inst_ctx = HookContext::for_smartnic_offload_installed("smartnic-0", "slp-rule", "hardware_asic");
        assert_eq!(inst_ctx.event, "on_smartnic_offload_installed");
        assert_eq!(inst_ctx.smartnic_device_id.as_deref(), Some("smartnic-0"));
        assert_eq!(inst_ctx.smartnic_rule_id.as_deref(), Some("slp-rule"));
        assert_eq!(inst_ctx.smartnic_offload_mode.as_deref(), Some("hardware_asic"));

        let sat_ctx = HookContext::for_smartnic_tcam_saturated("smartnic-0", 97.5);
        assert_eq!(sat_ctx.event, "on_smartnic_tcam_saturated");
        assert_eq!(sat_ctx.smartnic_device_id.as_deref(), Some("smartnic-0"));
        assert_eq!(sat_ctx.smartnic_tcam_percent, Some(97.5));

        let fall_ctx = HookContext::for_smartnic_fallback_engaged("smartnic-0", "driver", "TCAM saturated");
        assert_eq!(fall_ctx.event, "on_smartnic_fallback_engaged");
        assert_eq!(fall_ctx.smartnic_device_id.as_deref(), Some("smartnic-0"));
        assert_eq!(fall_ctx.smartnic_offload_mode.as_deref(), Some("driver"));
    }

    #[test]
    fn test_memfabric_lifecycle_events() {
        assert_eq!(
            LifecycleEvent::from_name("on_memfabric_page_fault_resolved"),
            Some(LifecycleEvent::MemFabricPageFaultResolved)
        );
        assert_eq!(
            LifecycleEvent::from_name("page_fault_resolved"),
            Some(LifecycleEvent::MemFabricPageFaultResolved)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_memfabric_dimension_paged"),
            Some(LifecycleEvent::MemFabricDimensionPaged)
        );
        assert_eq!(
            LifecycleEvent::from_name("dimension_paged"),
            Some(LifecycleEvent::MemFabricDimensionPaged)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_memfabric_pool_saturated"),
            Some(LifecycleEvent::MemFabricPoolSaturated)
        );
        assert_eq!(
            LifecycleEvent::from_name("memfabric_saturated"),
            Some(LifecycleEvent::MemFabricPoolSaturated)
        );

        let fault_ctx = HookContext::for_memfabric_page_fault(
            "chunk-nether-0",
            0x7fff_0000_1000,
            1850,
            "remote-nvram-node-1",
        );
        assert_eq!(fault_ctx.event, "on_memfabric_page_fault_resolved");
        assert_eq!(
            fault_ctx.memfabric_page_id.as_deref(),
            Some("chunk-nether-0")
        );
        assert_eq!(fault_ctx.memfabric_vaddr, Some(0x7fff_0000_1000));
        assert_eq!(fault_ctx.memfabric_latency_nanos, Some(1850));

        let dim_ctx = HookContext::for_memfabric_dimension_paged(
            "the_nether",
            32,
            131072,
            "remote-nvram-node-1",
        );
        assert_eq!(dim_ctx.event, "on_memfabric_dimension_paged");
        assert_eq!(dim_ctx.memfabric_dimension.as_deref(), Some("the_nether"));
        assert_eq!(dim_ctx.memfabric_pages_count, Some(32));
        assert_eq!(dim_ctx.memfabric_bytes_freed, Some(131072));

        let sat_ctx = HookContext::for_memfabric_pool_saturated(88.5, 94.2);
        assert_eq!(sat_ctx.event, "on_memfabric_pool_saturated");
        assert_eq!(sat_ctx.memfabric_dram_percent, Some(88.5));
        assert_eq!(sat_ctx.memfabric_nvram_percent, Some(94.2));

        assert_eq!(
            LifecycleEvent::from_name("on_nvme_namespace_created"),
            Some(LifecycleEvent::NvmeNamespaceCreated)
        );
        assert_eq!(
            LifecycleEvent::from_name("namespace_created"),
            Some(LifecycleEvent::NvmeNamespaceCreated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_nvme_multipath_failover"),
            Some(LifecycleEvent::NvmeMultipathFailoverTriggered)
        );

        let ns_ctx = HookContext::for_nvme_namespace_created(1, 1024, Some("lobby"), Some("overworld"));
        assert_eq!(ns_ctx.event, "on_nvme_namespace_created");
        assert_eq!(ns_ctx.nvme_nsid, Some(1));
        assert_eq!(ns_ctx.server_name.as_deref(), Some("lobby"));

        let fo_ctx = HookContext::for_nvme_multipath_failover("nqn.primary", "tcp", "rdma");
        assert_eq!(fo_ctx.event, "on_nvme_multipath_failover");
        assert_eq!(fo_ctx.nvme_nqn.as_deref(), Some("nqn.primary"));
        assert_eq!(fo_ctx.nvme_transport.as_deref(), Some("tcp"));

        let alert_ctx = HookContext::for_nvme_pool_capacity_alert(92.4, 900_000_000_000, 1_000_000_000_000);
        assert_eq!(alert_ctx.event, "on_nvme_pool_capacity_alert");
        assert_eq!(alert_ctx.nvme_pool_utilization_percent, Some(92.4));
    }

    #[test]
    fn test_vpn_lifecycle_hooks() {
        assert_eq!(
            LifecycleEvent::from_name("on_vpn_tunnel_established"),
            Some(LifecycleEvent::VpnTunnelEstablished)
        );
        assert_eq!(
            LifecycleEvent::from_name("vpn_key_rotated"),
            Some(LifecycleEvent::VpnKeyRotated)
        );
        assert_eq!(
            LifecycleEvent::from_name("peer_connected"),
            Some(LifecycleEvent::VpnPeerConnected)
        );
        assert_eq!(
            LifecycleEvent::from_name("security_degraded"),
            Some(LifecycleEvent::VpnSecurityDegradedAlert)
        );

        let vpn_ctx = HookContext::for_vpn_tunnel_established("craft-wg0", "hardware_p4");
        assert_eq!(vpn_ctx.event, "on_vpn_tunnel_established");
        assert_eq!(vpn_ctx.vpn_tunnel_id.as_deref(), Some("craft-wg0"));
        assert_eq!(vpn_ctx.vpn_crypto_mode.as_deref(), Some("hardware_p4"));

        let rekey_ctx = HookContext::for_vpn_key_rotated("craft-wg0", "eu-central", 42);
        assert_eq!(rekey_ctx.event, "on_vpn_key_rotated");
        assert_eq!(rekey_ctx.vpn_tunnel_id.as_deref(), Some("craft-wg0"));
        assert_eq!(rekey_ctx.vpn_peer_id.as_deref(), Some("eu-central"));
        assert_eq!(rekey_ctx.vpn_renegotiation_micros, Some(42));
    }

    #[test]
    fn test_bft_lifecycle_hooks() {
        assert_eq!(
            LifecycleEvent::from_name("on_bft_block_committed"),
            Some(LifecycleEvent::BftBlockCommitted)
        );
        assert_eq!(
            LifecycleEvent::from_name("bft_block"),
            Some(LifecycleEvent::BftBlockCommitted)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_bft_quorum_formed"),
            Some(LifecycleEvent::BftQuorumFormed)
        );
        assert_eq!(
            LifecycleEvent::from_name("quorum_formed"),
            Some(LifecycleEvent::BftQuorumFormed)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_bft_validator_slashed"),
            Some(LifecycleEvent::BftValidatorSlashed)
        );
        assert_eq!(
            LifecycleEvent::from_name("validator_slashed"),
            Some(LifecycleEvent::BftValidatorSlashed)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_bft_view_timeout"),
            Some(LifecycleEvent::BftViewTimeout)
        );
        assert_eq!(
            LifecycleEvent::from_name("pacemaker_timeout"),
            Some(LifecycleEvent::BftViewTimeout)
        );

        let commit_ctx = HookContext::for_bft_block_committed(42, "0xabcd1234", 12.5);
        assert_eq!(commit_ctx.event, "on_bft_block_committed");
        assert_eq!(commit_ctx.bft_view, Some(42));
        assert_eq!(commit_ctx.bft_block_hash.as_deref(), Some("0xabcd1234"));
        assert_eq!(commit_ctx.bft_commit_latency_ms, Some(12.5));

        let qc_ctx = HookContext::for_bft_quorum_formed(42, "0xabcd1234", 4);
        assert_eq!(qc_ctx.event, "on_bft_quorum_formed");
        assert_eq!(qc_ctx.bft_view, Some(42));
        assert_eq!(qc_ctx.bft_block_hash.as_deref(), Some("0xabcd1234"));

        let slash_ctx = HookContext::for_bft_validator_slashed("val-byzantine", "equivocation", 42);
        assert_eq!(slash_ctx.event, "on_bft_validator_slashed");
        assert_eq!(slash_ctx.bft_validator_id.as_deref(), Some("val-byzantine"));
        assert_eq!(slash_ctx.bft_slash_reason.as_deref(), Some("equivocation"));
        assert_eq!(slash_ctx.bft_view, Some(42));

        let timeout_ctx = HookContext::for_bft_view_timeout(42, "val-1", "val-2");
        assert_eq!(timeout_ctx.event, "on_bft_view_timeout");
        assert_eq!(timeout_ctx.bft_view, Some(42));
    }

    #[test]
    fn test_jitter_lifecycle_hooks() {
        assert_eq!(
            LifecycleEvent::from_name("on_kernel_microstall_detected"),
            Some(LifecycleEvent::KernelMicroStallDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("microstall"),
            Some(LifecycleEvent::KernelMicroStallDetected)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_realtime_priority_escalated"),
            Some(LifecycleEvent::RealtimePriorityEscalated)
        );
        assert_eq!(
            LifecycleEvent::from_name("priority_escalated"),
            Some(LifecycleEvent::RealtimePriorityEscalated)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_irq_storm_shielded"),
            Some(LifecycleEvent::IrqStormShielded)
        );
        assert_eq!(
            LifecycleEvent::from_name("irq_shielded"),
            Some(LifecycleEvent::IrqStormShielded)
        );
        assert_eq!(
            LifecycleEvent::from_name("on_jitter_threshold_exceeded"),
            Some(LifecycleEvent::JitterThresholdExceeded)
        );
        assert_eq!(
            LifecycleEvent::from_name("jitter_exceeded"),
            Some(LifecycleEvent::JitterThresholdExceeded)
        );

        let stall_ctx = HookContext::for_kernel_microstall_detected("lobby", 850, "PriorityInversion");
        assert_eq!(stall_ctx.event, "on_kernel_microstall_detected");
        assert_eq!(stall_ctx.server_name.as_deref(), Some("lobby"));
        assert_eq!(stall_ctx.microstall_duration_us, Some(850));

        let prio_ctx = HookContext::for_realtime_priority_escalated("lobby", 85, "2,3");
        assert_eq!(prio_ctx.event, "on_realtime_priority_escalated");
        assert_eq!(prio_ctx.server_name.as_deref(), Some("lobby"));
        assert_eq!(prio_ctx.realtime_priority, Some(85));
        assert_eq!(prio_ctx.isolated_cpus.as_deref(), Some("2,3"));

        let irq_ctx = HookContext::for_irq_storm_shielded(33, "mlx5_comp", "0,1");
        assert_eq!(irq_ctx.event, "on_irq_storm_shielded");
        assert_eq!(irq_ctx.irq_number, Some(33));
        assert_eq!(irq_ctx.isolated_cpus.as_deref(), Some("0,1"));

        let thresh_ctx = HookContext::for_jitter_threshold_exceeded("lobby", 620.5, 500.0);
        assert_eq!(thresh_ctx.event, "on_jitter_threshold_exceeded");
        assert_eq!(thresh_ctx.server_name.as_deref(), Some("lobby"));
        assert_eq!(thresh_ctx.jitter_micros, Some(620.5));
    }
}
