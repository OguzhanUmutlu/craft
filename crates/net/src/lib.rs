pub mod a2s;
pub mod chunk_packet;
pub mod dpdk;
pub mod ebpf_engine;
pub mod ebpf_filter;
pub mod edge_probe;
pub mod edge_router;
pub mod firewall;
pub mod histogram;
pub mod hsm_transport;
pub mod loopback;
pub mod live_splicer;
pub mod mtls;
pub mod packet_inspector;
pub mod page_pool;
pub mod pqc_transport;
pub mod query;
pub mod raknet;
pub mod rcon;
pub mod sleep_proxy;
pub mod slp;
pub mod raft_transport;
pub mod tick_profiler;
pub mod wireguard;
pub mod xdp_pipeline;
pub mod pmu_sampler;
pub mod shm_bus;
pub mod patch_engine;
pub mod microvm;
pub mod crash_triage;
pub mod rdma;
pub mod smartnic;
pub mod memfabric;
pub mod nvme;
pub mod vpn;
pub mod bft;
pub mod jitter;
pub mod neuromorphic;
pub mod optical;
pub mod ptp;
pub mod dna;
pub mod cpo;


pub use a2s::{ping_a2s_server, A2sPingStatus};
pub use chunk_packet::{
    decode_varint, encode_varint, ChunkDataPacket, ChunkSection, ChunkSerializationBenchmark,
    SocketTransferSimulation, DEFAULT_CHUNK_PACKET_ID,
};
pub use dpdk::{
    DpdkDriver, DpdkDriverConfig, DpdkDriverStats, JitterCalculator, PacketDescriptor,
    PacketRingBuffer,
};
pub use ebpf_engine::{
    EbpfProbeEngine, FlameGraphBuilder, SocketBufferPressureLevel, SocketBufferTelemetry,
};
pub use ebpf_filter::{DropStatistics, EbpfFilterCompiler, PacketFilterEngine, PacketVerdict, RawPacketHeader};
pub use edge_probe::{EdgeLatencyProber, EdgeProbeResult};
pub use edge_router::EdgeRouteGenerator;
pub use firewall::allow_ip_port;
pub use histogram::LatencyHistogram;
pub use hsm_transport::{
    decode_hsm_frame, encode_hsm_frame, AttestationChallenge, AttestationResponse,
    HsmFrameHeader, HsmSignedEnvelope, HsmTransportVerifier, ZkMembershipChallenge,
    ZkMembershipResponse, CRAFT_HSM_MAGIC, CRAFT_HSM_VERSION, FRAME_TYPE_ATTESTATION_CHALLENGE,
    FRAME_TYPE_ATTESTATION_RESPONSE, FRAME_TYPE_SIGNED_ENVELOPE,
    FRAME_TYPE_ZK_MEMBERSHIP_CHALLENGE, FRAME_TYPE_ZK_MEMBERSHIP_RESPONSE,
};
pub use live_splicer::{
    decode_migration_message, encode_migration_message, AnycastBgpEngine, ConnectionSplicer,
    MigrationWireMessage, PlayerSocketHandoffFrame, SplicerState, CRAFT_MIGRATION_MAGIC,
};
pub use loopback::{enable_bedrock_loopback, is_bedrock_loopback_enabled};
pub use mtls::{CertMetadata, MtlsEngine, NodeCertBundle, RootCaBundle};
pub use packet_inspector::{
    AnomalySeverity, NettyPacketInspector, PacketFloodAnomaly, PacketRateSummary,
};
pub use page_pool::{
    PageDescriptor, PagePoolPipelineResult, PageSliceRef, SocketPagePool, CACHE_LINE_ALIGNMENT,
    DEFAULT_PAGE_SLICE_SIZE, DEFAULT_POOL_CAPACITY_PAGES,
};
pub use pqc_transport::{
    decode_pqc_proposal, decode_pqc_response, encode_pqc_proposal, encode_pqc_response,
    PqcClientHandshake, PqcHandshakeProposal, PqcHandshakeResponse, PqcNodeCertBundle,
    PqcServerTransport, PqcSessionContext, CRAFT_PQC_MAGIC, CRAFT_PQC_VERSION,
};
pub use query::{ping_server_auto, probe_tcp_port, UniversalPingStatus};
pub use raft_transport::{
    decode_raft_envelope, decode_raft_message, encode_raft_envelope, encode_raft_message,
    AppendEntriesArgs, AppendEntriesReply, HeartbeatArgs, HeartbeatReply, InstallSnapshotArgs,
    InstallSnapshotChunkArgs, InstallSnapshotChunkReply, InstallSnapshotReply,
    RaftMessageEnvelope, RaftRpcMessage, RequestVoteArgs, RequestVoteReply, SplitBrainArbitrator,
    CRAFT_RAFT_MAGIC,
};
pub use raknet::{ping_bedrock_server, BedrockPingStatus};
pub use rcon::RconClient;
pub use sleep_proxy::{SleepProxy, SleepProxyConfig, SleepProxyHandle};
pub use slp::{ping_java_server, ServerPingStatus};
pub use tick_profiler::{TickHealthGrade, TickProfileSummary, TickProfiler, TickSample};
pub use wireguard::{base64_decode, base64_encode, WgConfigGenerator, WireguardKeypair, WireguardPeerMetrics};
pub use xdp_pipeline::{
    compute_syn_cookie, validate_raknet_packet, validate_syn_cookie, RakNetValidationResult,
    XdpBenchmarkResult, XdpPacketDescriptor, XdpPipeline, RAKNET_OFFLINE_MAGIC,
};
pub use pmu_sampler::{demangle_symbol, MemoryChurnReport, PmuSampler};
pub use shm_bus::{benchmark_shm_throughput, ShmConsumer, ShmProducer, ShmRingBus};
pub use patch_engine::{
    benchmark_patch_throughput, JvmBytecodeEngine, NativeTrampolineEngine, PatchSafetyVerifier,
};
pub use microvm::{
    benchmark_microvm_boot, TapBridgeDriver, VsockMultiplexer, VsockSession,
};
pub use crash_triage::{
    benchmark_crash_triage, signal_name_from_code, AiTriageAdvisor, ElfCoreDumpParser,
    JvmHsErrParser, MemoryLeakDetector, SIGABRT, SIGBUS, SIGFPE, SIGILL, SIGSEGV,
};
pub use rdma::{
    benchmark_rdma_fabric, compute_roce_icrc, RdmaFailoverBridge, RdmaProtectionDomain,
    RdmaVerbsEngine, RoceV2Bth, RoceV2Packet, RoceV2Reth, BTH_OPCODE_ACKNOWLEDGE,
    BTH_OPCODE_RDMA_READ_REQUEST, BTH_OPCODE_RDMA_READ_RESPONSE, BTH_OPCODE_RDMA_WRITE_ONLY,
    BTH_OPCODE_SEND_ONLY, ROCE_V2_UDP_PORT,
};
pub use smartnic::{
    benchmark_smartnic_line_rate, synthesize_raknet_unconnected_pong, synthesize_slp_pong,
    P4PipelineEngine, SmartNicFallbackBridge, SmartNicOffloadEngine,
};
pub use memfabric::{
    benchmark_remote_paging, DimensionEvictionSummary, DimensionMemoryFabric,
    DimensionTouchSummary, PageFaultEventType, PageFaultRecord, RemotePagingEngine,
    UserfaultPageHandler, BASE_VIRTUAL_ADDR, PAGE_SIZE_2M, PAGE_SIZE_4K,
};
pub use nvme::{
    benchmark_nvme_fabric, FlashBlockPoolEngine, NvmeCommandCapsule, NvmeCompletionCapsule,
    NvmeConnectPayload, NvmeTargetEngine, NVME_COMMAND_SIZE, NVME_COMPLETION_SIZE,
    NVME_DEFAULT_BLOCK_SIZE, NVME_FABRICS_OPCODE, NVME_OPCODE_DATASET_MGMT,
    NVME_OPCODE_FLUSH, NVME_OPCODE_READ, NVME_OPCODE_WRITE, NVME_OPCODE_WRITE_ZEROES,
    NVME_STATUS_SUCCESS,
};
pub use vpn::{
    benchmark_vpn_mesh, compute_mac1, hkdf_sha256, PqxdhEngine, PqxdhInitiatorState,
    PqxdhSession, SmartNicCryptoOffloadEngine, WgPqxdhDataPacket, WgPqxdhInitMessage,
    WgPqxdhResponseMessage, WireGuardMeshEngine,
};
pub use bft::{
    benchmark_bft_consensus, BftRateLimiter, BftWireMessage, HotStuffBftEngine,
    ProposalWirePayload, QcWirePayload, ViewChangeWirePayload, VoteWirePayload,
    ZkProofSyncPayload, BFT_WIRE_MAGIC, MSG_TYPE_PROPOSAL, MSG_TYPE_QC, MSG_TYPE_VIEW_CHANGE,
    MSG_TYPE_VOTE, MSG_TYPE_ZK_SYNC,
};
pub use jitter::{
    benchmark_kernel_jitter, KernelSchedTracer, MicroStallScheduler, PerfEventRingBuffer,
    RawPerfSample,
};
pub use neuromorphic::{
    benchmark_neuromorphic_scheduler, SpikeNeuralNetwork, SpikeQueue, SynapseMatrix,
};
pub use optical::{
    benchmark_optical_crossbar, MemsCrossbarSwitch, OpticalWaveguideFrame, WdmMultiplexer,
    OPTICAL_FRAME_MAGIC, OPTICAL_HEADER_SIZE,
};
pub use ptp::{
    benchmark_ptp_clock_sync, generate_ptp_status_summary, PtpClockServo, PtpMessage,
    PtpMessageType, TrueTimeEngine, PTP_FRAME_MAGIC, PTP_HEADER_SIZE,
};
pub use dna::{
    benchmark_dna_archival, decode_oligos_to_chunk, encode_chunk_to_oligos, simulate_dna_decay,
    DnaSynthesisFrame, NanoporeSequencer, RawSquiggle, DNA_FRAME_MAGIC, SEGMENT_PAYLOAD_SIZE,
};
pub use cpo::{
    benchmark_cpo_interconnect, CpoTensorFrame, CpoThermalRegulator, PhotonicTensorEngine,
    CPO_FRAME_MAGIC, FLAG_ANALOG_MVM, FLAG_LOOPBACK, FLAG_THERMAL_STABILIZED,
};




