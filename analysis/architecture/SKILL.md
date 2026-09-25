# Architecture & Core Mechanics Skill Guide

> **Domain**: System Architecture, Crate Boundaries, State Storage & File Locks  
> **Primary Location**: `crates/core/` and Workspace Infrastructure

---

## 1. Architectural Philosophy and Layering

Craft follows a strict layered architecture where lower-level crates provide pure abstractions without depending on higher-level CLI or TUI layers:

```
[ crates/cli ] ──────────────► Interactive TUI (ModalX), CLI Command Handlers
      │
      ├──────────────────────► [ crates/daemon ]      (Background Service & IPC)
      ├──────────────────────► [ crates/remote ]      (SSH / SFTP / PTY Streaming)
      ├──────────────────────► [ crates/backup ]      (Snapshots & Cloud Providers)
      ├──────────────────────► [ crates/plugins ]     (Modrinth/Hangar/Poggit/Maps)
      ├──────────────────────► [ crates/providers ]   (21 Software Engines)
      ├──────────────────────► [ crates/scripting ]   (Embedded Lua 5.4 Engine)
      ├──────────────────────► [ crates/net ]         (SLP, RakNet, A2S, RCON)
      └──────────────────────► [ crates/core ]        (State, Locks, Registries, Paths)
```

### Invariant Rules:
1. **No Downward Leakage**: `craft-core` must never depend on any other crate in the workspace.
2. **Deterministic Paths**: All paths must resolve through [`CraftPaths`](file:///D/Projects/craft/crates/core/src/path.rs). Never hardcode relative paths or construct home directory paths manually.
3. **Lock Protection on Every Mutation**: Registries (`servers.toml`, `remotes.toml`, `trash/manifest.toml`) must use exclusive file locks before writing.

---

## 2. Directory Hierarchy (`CraftPaths`)

`CraftPaths` anchors all state at `CRAFT_HOME` (defaulting to `~/.craft` or overridden via the `CRAFT_HOME` environment variable):

```
~/.craft/
├── servers/               # Server working directories (<server-name>/)
├── cache/
│   ├── artifacts/         # Raw downloaded server JARs and archives
│   ├── chunks/            # Content-addressed deduplication chunks (xx/<hash>.chunk.zst)
│   └── meta/              # Zstd-compressed JSON metadata
├── backups/               # Local snapshot storage (<server-name>/<archive>.tar.zst)
├── dr/
│   └── runbooks/          # Disaster recovery runbooks and plans (<server>.toml)
├── diagnostics/           # JFR execution profiles and performance reports (<server>/)
├── run/
│   ├── daemon.sock        # Unix domain socket (Linux/macOS)
│   ├── daemon.pid         # Daemon process ID
│   └── locks/             # OS-level file lock descriptors (*.lock)
├── trash/
│   ├── manifest.toml      # Transactional trash manifest
│   └── <id>_<name>/       # Recoverable staged files and directories
├── supply_chain/          # Cryptographic trust anchors, policies & attestations
│   ├── trust_anchors.json # Registered root certs and public keys
│   ├── policy.json        # Strict / Audit / Disabled verification policy
│   ├── registry.json      # Local artifact attestation registry
│   └── attestations/      # In-toto DSSE attestation JSON sidecars (<sha256>.json)
├── migrations/            # Zero-downtime live migration plans and checkpoints
│   ├── migrations.toml    # Live migration registry and active plans
│   └── checkpoints/       # CRIU process and memory checkpoint snapshots (<server>/)
├── servers.toml           # Registered local server instances
├── remotes.toml           # Federated remote SSH host configurations
├── clusters.toml          # Multi-server cluster topologies and DAGs
├── rbac.toml              # Multi-tenant user accounts, roles & scopes
├── mesh.toml              # Distributed multi-cloud storage mesh targets & quorums
├── intelligence.toml      # Autopilot operational intelligence policies & thresholds
├── shm/                   # Memory-mapped persistent shared memory & ring buffer state
│   ├── registry.json      # Active ring buffer segment registry
│   ├── state.json         # Persisted fallback IPC throughput state
│   └── segments/          # Named memory-mapped files (/craft_shm_<server>_<channel>)
├── crash/                 # Autonomous crash triage reports & core dump artifacts
│   ├── registry.json      # Triaged crash reports and leak candidates
│   ├── state.json         # Real-time memory leak and triage telemetry
│   ├── reports/           # Structured JSON crash triage reports (<id>.json)
│   └── dumps/             # Ingested ELF core dumps and hs_err logs
├── rdma/                  # Autonomous RDMA acceleration & fabric topology state
│   ├── registry.json      # Registered memory regions, peers, and QP state
│   └── state.json         # Real-time transfer metrics and failover state
├── bft/                   # Autonomous BFT consensus, BLS signatures & ZK proofs
│   ├── registry.json      # Registered BFT validators, public keys, and stakes
│   ├── state.json         # Current view, high QC, and committed state tree root
│   └── proofs/            # Serialized ZK state transition proofs and QCs
└── audit.log              # Append-only continuous HMAC-SHA256 audit ledger
```

---

## 3. Transactional Registries & Concurrency

### 3.1. `ServersRegistry`
- **Location**: `~/.craft/servers.toml`
- **Format**: TOML array of `ServerConfig` structs (`name`, `path`, `software`, `version`, `memory`, `jvm_args`, `java_path`, `auto_run`).
- **Atomic Mutation Pattern**:
  ```rust
  let mut registry = ServersRegistry::load(paths)?;
  registry.servers.retain(|s| s.path != target_path);
  registry.save(paths)?; // Writes to temporary file then renames atomically
  ```

### 3.2. Inter-Process File Locking
- Every shared state file uses `fs2::FileExt::lock_exclusive()` on a corresponding `.lock` file in `run/locks/`:
  - `trash.lock` prevents race conditions during concurrent trash moves or restorations.
  - `servers.lock` guards server registry additions and removals.
  - `remotes.lock` synchronizes SSH host configuration mutations.
  - `clusters.lock` prevents concurrent cluster topology updates.
  - `rbac.lock` synchronizes multi-tenant user and role definitions.
  - `audit.lock` guards append operations to the continuous HMAC audit log.
  - `mesh.lock` synchronizes multi-cloud storage mesh targets and replication policies.
  - `intelligence.lock` guards autonomous autopilot policies and remediation thresholds.
  - `supply_chain.lock` synchronizes supply chain policies, trust anchors, and attestation registries.
  - `crash.lock` guards crash reports, memory leak detections, and automated remediation actions.
  - `bft.lock` guards BFT consensus state mutations and validator registry modifications.
  - `<server_dir>/server.lock` ensures a server instance cannot be launched simultaneously by multiple processes.

### 3.3. `RbacRegistry` & `AuditLedger`
- **`RbacRegistry` (`~/.craft/rbac.toml`)**: Manages `UserAccount` entries with 1,000-round SHA-256 salted hashes, `Role` hierarchies, granular `Permission` sets, and per-user `assigned_servers` filtering. Auto-initializes default `admin:admin` account if empty.
- **`AuditLedger` (`~/.craft/audit.log`)**: Records every CLI, REST, and WebSocket mutation with continuous SHA-256 hash chains starting at `GENESIS_HASH` and HMAC-SHA256 signatures. Supports verification against tampering with `AuditLedger::verify_chain`.

### 3.4. `MeshRegistry` (`~/.craft/mesh.toml`)
- Configures geo-distributed multi-cloud storage destinations (`MeshTargetKind::S3`, `CloudflareR2`, `GDrive`, `Sftp`, `Local`) with endpoint URLs, bucket/folder paths, credentials, and quorum rules (`All`, `Majority`, `Any`).
- Synchronized through `mesh.lock` and managed via `craft mesh` commands.

### 3.5. `DrRunbook` & Disaster Recovery Orchestrator
- **Location**: `~/.craft/dr/runbooks/<server>.toml`
- **Format**: TOML configuration defining target server parameters, recovery point objective (RPO) seconds, recovery time objective (RTO) seconds, failover target remote, and verification steps.
- Executes sandbox simulations (`~/.craft/staging/dr-test-<server>`) asserting 0 byte divergence before deploying failover.

### 3.6. `IntelligenceRegistry` (`~/.craft/intelligence.toml`)
- Stores global defaults and per-server `IntelligencePolicy` configurations (mode: `Advisory` vs `Autonomous`, MSPT warning/critical thresholds, memory leak slope limits, and off-peak restart windows).
- Protected by `intelligence.lock` and configured via `craft ai policy`.

### 3.7. `ClustersRegistry` & Topological DAG Scheduling
- **Location**: `~/.craft/clusters.toml`
- **Format**: TOML array of `ServerCluster` structs (`name`, `nodes`, `proxy_entry`) with node definitions (`name`, `role`, `remote`, `depends_on`).
- **Roles**: `backend`, `proxy`, `lobby`.
- **DAG Topological Ordering**:
  - `ServerCluster::resolve_startup_order()` constructs an in-degree dependency graph from explicit `depends_on` lists.
  - Proxy nodes implicitly depend on all backend/lobby nodes if unconfigured, guaranteeing routing proxies start last once upstream servers are healthy.
  - Detects cyclic dependencies and returns descriptive errors.
  - `ServerCluster::resolve_shutdown_order()` reverses the sequence, ensuring proxies disconnect players before backend worlds terminate.

### 3.8. Cross-Node Federated Migration (`ServerMigrator`)
- **Protocol**:
  1. Validates source server is stopped (`server.lock` and PID verification).
  2. Verifies remote host SSH connection and `craft` binary installation.
  3. Compresses source server into an atomic `.tar.zst` snapshot (excluding locks, PIDs, and sockets).
  4. Computes SHA-256 digest locally and streams upload via SFTP to `~/.craft/staging/`.
  5. Computes remote checksum (`sha256sum`, `shasum -a 256`, or OpenSSL) and validates integrity before unpacking.
  6. Extracts directly into `~/.craft/servers/<target_name>` on the remote node.
  7. Re-registers the server in remote `~/.craft/servers.toml`, cleans up staging, and optionally trashes local source (`TrashManager::trash_path`).

### 3.9. `RolloutRegistry`, Canary Deployments & Autonomous Fleet Healing
- **Location**: `~/.craft/rollouts.toml`
- **Lock Protection**: `~/.craft/run/locks/rollouts.lock` via `RolloutRegistry::modify`.
- **Strategy Matrix**:
  - `Canary`: Designates a canary node with fractional player ingress (e.g. 25%) and evaluates health metrics over a bake window (`bake_seconds`).
  - `BlueGreen`: Operates parallel instance clusters (`blue` and `green`), swapping routing proxy endpoints atomically upon health verification.
  - `Rolling`: Sequentially upgrades nodes in bounded batches (`max_parallel`) to preserve cluster capacity.
- **Canary Health Criteria**:
  - Validates real-time TPS (`min_tps`), MSPT (`max_mspt`), microsecond tick jitter (`max_jitter_ms`), and process crash count (`max_crash_count`).
  - Continuous evaluation loop in `FleetHealer` observes telemetry; criteria breach triggers instant rollback, halting candidate instances, restoring `.tar.zst` pre-rollout snapshots, and reconnecting original proxy routes.
- **Proxy Traffic Draining**:
  - `EdgeRouteGenerator::generate_velocity_drained_config`, `generate_bungeecord_drained_config`, and `generate_haproxy_drained_config` isolate canary or upgrading nodes from ingress fallback lists while preserving local health probe access.
- **Autonomous Fleet Healing**:
  - Real-time node evaluations classify status as `Healthy`, `Baking`, `Draining`, `Degraded`, or `Crashed`.
  - Self-healing actions include `RestartNode`, `RollbackNode` (snapshot restoration), `DrainNode`, `PromoteCanary`, and `MarkDegraded`, dispatching scripting lifecycle hooks (`LifecycleEvent::FleetNodeHealed`).

### 3.10. Unified Multi-Server Log Ingestion, Elastic Search & Distributed Forensics
- **Storage Locations**:
  - `~/.craft/indices/<server_name>/block_<start_line>.idx.json`: Inverted index blocks with zstd-compressed payload chunks and token posting tables.
  - `~/.craft/forensics/inc-<server_name>-<timestamp>.json`: Cryptographically authenticated post-mortem incident reports.
- **Embedded Log Ingestion Service (`LogIngestionService`)**:
  - Integrated directly into `craft-daemon` with zero external Elasticsearch or Lucene runtime requirements.
  - Maintains in-memory circular ring buffers (up to 10,000 entries per server) and scans rotating disk log files (`logs/latest.log`, `server.log`, `crash-reports/*.txt`).
  - Partitioned chunk builder generates `InvertedIndexBlock` files every 5,000 lines, serializing zstd-compressed JSON log entries alongside token posting lists (`term_postings: HashMap<String, Vec<u32>>`) and 8-bit log-level bloom masks.
- **Sub-Millisecond Search Execution**:
  - `LogQuery` executes unified token and regex queries across disk inverted index blocks and in-memory active buffers.
  - Level bitmask pruning and timestamp range bounds skip non-matching blocks without decompressing payload chunks.
  - Federated query dispatch: `RemoteCraftClient::search_remote_logs` dispatches search queries over pooled SSH connections to remote edge nodes, collating results transparently.
- **Post-Mortem Incident Forensics & Java Stack Trace Demangling**:
  - Real-time exception detector detects crash triggers (`FATAL`, `ENCOUNTERED AN UNEXPECTED EXCEPTION`, `EXCEPTION IN THREAD`, `MINECRAFT CRASH REPORT`).
  - Captures 50 lines of preceding console events leading up to the failure.
  - `demangle_stack_trace` reconstructs method origin, source file, line number, and JAR source tag from obfuscated stack traces.
  - Automated culprit attribution (`CulpritType::Plugin`, `CulpritType::Core`, `CulpritType::JavaRuntime`, `CulpritType::Native`) maps crashing frames back to specific plugin JARs or core server runtime components.
  - Cryptographic authenticity verification: `IncidentTimeline` signs timelines using HMAC-SHA256 (`verify_authenticity`), preventing post-mortem tampering.
- **Lifecycle Hook Bus Integration**:
  - Fires `LifecycleEvent::IncidentDetected` and `LifecycleEvent::LogAlertTriggered` events to embedded Lua scripts with incident metadata, culprit exception, log level, and message payload.
- **CLI Commands & ModalX Centered TUI**:
  - `craft log search <pattern> [-s server] [-l level] [-r] [--since RFC3339] [--until RFC3339] [--remote alias] [--json]`
  - `craft log forensics <server> [incident_id] [--export path] [--json]`
  - `craft log incidents [server] [--limit N] [--json]`
  - `craft log index [server] [-f]`
  - Full-screen centered interactive TUI panel (`Tools -> Log Search & Incident Forensics`) powered by ModalX.

### 3.11. AI-Driven Workload Forecasting, Predictive Auto-Scaling & Autonomous Cost Optimization
- **Storage Locations**:
  - `~/.craft/forecasting.toml`: Declarative policy registry configuring proactive wake lead times, surge thresholds, and quiet windows.
  - `~/.craft/run/locks/forecasting.lock`: Cross-process file lock guard for atomic policy updates.
  - `~/.craft/diagnostics/workload/<server_name>.json`: Serialized time-series samples containing hourly player counts, MSPT statistics, and RSS memory footprints.
- **Deterministic Seasonal Workload Forecasting (`SeasonalForecaster`)**:
  - Decomposes player workload into a 24-hour diurnal curve and 7-day day-of-week seasonality multiplier.
  - Multi-step projections over 1h, 6h, 12h, 24h, 7d horizons with quantiles (P10 lower bound, P50 expected, P90 peak surge).
  - Confidence scoring dynamically scales with sample density (N / 168, capped at 1.0).
  - Surge risk detection warns when projected peak exceeds current player counts by configurable deltas.
  - Generates compact plain-text sparklines for TUI and CLI telemetry representations.
- **Financial Cost Optimization Ledger (`CostOptimizationModel`)**:
  - Quantifies computing resources saved through predictive hibernation and dynamic downscaling.
  - Computes vCPU core-hours and RAM GiB-hours saved, baseline always-on cost, realized cost, net dollar savings, efficiency percentage, and projected monthly run-rate reductions.
- **In-Process Daemon Forecasting Service (`WorkloadForecastingService`)**:
  - Autonomous supervisor loop periodically records hourly telemetry samples from running servers.
  - Proactive Wake-Up: Pre-warms hibernated servers 15-30 minutes ahead of predicted player surges, eliminating cold-start login latency.
  - Quiet-Hour Downscaling: Automatically hibernates or throttles idle instances during historical off-peak hours.
  - Typed IPC endpoints: `GetWorkloadForecast`, `GetCostOptimizationReport`, `SetWorkloadPolicy`, `TriggerProactiveScalingNow`.
- **Remote Federation & Scripting Hook Bus**:
  - Federated query dispatch: `RemoteCraftClient::get_remote_forecast` and `get_remote_cost_report` query remote edge nodes over pooled SSH connections.
  - Lifecycle events: `LifecycleEvent::WorkloadSurgePredicted`, `CostOptimizationApplied`, `ProactiveWakeTriggered` dispatch to Lua scripts with predicted player counts, savings estimates, and scaling actions.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft forecast show [server] [--horizon N] [--remote alias] [--json]`
  - `craft forecast cost [--server name] [--remote alias] [--json]`
  - `craft forecast schedule [server] [--lead-mins N] [--quiet-start H] [--quiet-end H] [--enabled bool] [--json]`
  - `craft forecast optimize [server] [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Workload Forecasting & Cost Optimizer`) powered by ModalX.

### 3.12. Autonomous Modpack CI/CD, Binary Delta Patching & Fast Client Synchronizer
- **Storage Locations**:
  - `~/.craft/modpacks/ci`: Artifact storage directory containing packaged client and server `.tar.zst` distribution archives.
  - `~/.craft/cache/deltas`: Content-addressed cache of computed `.delta` binary patch files.
  - `~/.craft/modpacks.toml`: Atomic registry recording modpack builds, component lists, archive SHA-256 digests, and transition delta manifests.
  - `~/.craft/run/locks/modpack.lock`: Cross-process file lock protecting registry mutations during concurrent CI builds and delta computations.
- **Pure-Rust Block-Level Binary Delta Engine (`BinaryDeltaEngine`)**:
  - Computes sub-megabyte binary deltas between multi-gigabyte server/client archives without external system dependencies (`librsync`/`bsdiff`).
  - Rolling Adler-32 checksums match 4096-byte blocks between source and target payloads.
  - Generates compact `DeltaOp::Copy { source_offset, length }` and `DeltaOp::Insert { data }` operation streams.
  - Payload streams are compressed with Zstandard (`zstd::encode_all`) and prepended with a cryptographic `BinaryDeltaHeader` (`CRAFTDLT` magic, block size, original size, target size, source and target SHA-256 digests).
  - Byte-for-byte target reconstruction is cryptographically verified against `target_sha256`.
- **Modpack CI/CD Pipeline (`ModpackBuilder`)**:
  - Performs AST and bytecode inspection across ZIP and JAR entries: parses `fabric.mod.json`, `quilt.mod.json`, `mods.toml`, and detects client-side vs. server-side markers (e.g. `environment`, `client`, `server`, and keyword heuristics for shaders/renderers vs. databases/permissions).
  - Classifies components into `ModSide::ClientOnly`, `ModSide::ServerOnly`, or `ModSide::Both`.
  - Verifies mod dependencies against declared Minecraft and mod loader versions.
  - Bundles distribution archives into deterministic `.tar.zst` packages for server deployments and client syncs.
- **In-Process Daemon Distribution Service (`ModpackDistributionService`)**:
  - Implements RFC 7233 partial content chunk streaming with `Range: bytes=X-Y` header parsing.
  - Streams modpack distribution archives and binary delta patches to remote clients in concurrent 1MB chunks without memory blowup.
  - Typed IPC endpoints: `BuildModpack`, `GenerateDelta`, `GetModpackStatus`, `GetModpackChunk`.
- **Remote Federation & Scripting Hook Bus**:
  - Remote synchronization: `RemoteCraftClient::sync_modpack_delta` securely deploys delta patches across federated SSH clusters.
  - Scripting hooks: `LifecycleEvent::ModpackBuildCompleted`, `ModpackDeltaPublished`, `ClientSyncRequested` fire into embedded Lua scripts with artifact size, version, and bandwidth reduction metrics.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft modpack build [dir] [--name name] [--version v] [--loader l] [--mc-version v] [--target both|server|client] [--output dir] [--json]`
  - `craft modpack delta <source> <target> [--name name] [--src-version v1] [--target-version v2] [--output file] [--json]`
  - `craft modpack patch <base> <patch> [--output file] [--json]`
  - `craft modpack sync <name> [--version v] [--client-dir dir] [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Modpack CI/CD & Fast Client Synchronizer`) powered by ModalX.

### 3.13 Zero-Trust Inter-Server Microsegmentation, eBPF Packet Filtering & WireGuard Overlay Mesh

- **Core Topology & File Layout**:
  - `~/.craft/sdn`: Primary SDN overlay state directory.
  - `~/.craft/sdn/mesh.toml`: Atomic registry configuration tracking mesh name, overlay CIDR (`10.42.0.0/16`), local node configuration, WireGuard peers, and active microsegmentation policy.
  - `~/.craft/sdn/certs`: Storage directory for mutual TLS Root CA (`ca.crt`, `ca.key`) and per-node certificates (`node.crt`, `node.key`).
  - `~/.craft/sdn/wireguard`: Synthesized platform-specific configuration files (`wg0.conf`, `up.sh`, `down.sh`, `filter.c`, `rules.nft`, `wireguard.netdev`, `wireguard.network`).
  - `~/.craft/run/locks/sdn.lock`: Advisory file lock (`fs2`) synchronizing concurrent mesh mutations and cryptographic key rotations.
- **Pure-Rust WireGuard Keypair & Configuration Generator (`WgConfigGenerator`)**:
  - Generates Curve25519 X25519 keypairs and Base64-encoded strings without OpenSSL or external C dependencies.
  - Synthesizes `wg-quick` standard configuration format, Linux `ip link` / `wg set` shell provisioning scripts, `systemd-networkd` `.netdev`/`.network` unit files, and Windows tunnel configs.
  - Tracks live WireGuard peer metrics (`WireguardPeerMetrics`): RX/TX transfer volumes, last handshake timestamps, RTT latency, and connection status.
- **Pure-Rust Userspace eBPF Packet Filter & Rate Limiter (`PacketFilterEngine`)**:
  - Enforces zero-trust isolation zones: `IngressProxy`, `BackendWorld`, `StorageMesh`, `ControlPlane`.
  - Evaluates raw IP packet headers against granular microsegmentation rules (`protocol`, `ports`, `source_zone`, `target_zone`, `action`).
  - Implements sliding-window rate limiters per flow/IP to prevent DDoS amplification or port flooding across the overlay.
  - Generates drop-in C eBPF source code (`filter.c`) targeting `SEC("cgroup/skb")` and `SEC("xdp")`, Linux `nftables` rulesets (`rules.nft`), and `iptables` commands.
- **Mutual TLS Engine & Root CA Lifecycle (`MtlsEngine`)**:
  - Generates self-signed Root CA and issues per-node X.509-compatible certificates with DNS and IP Subject Alternative Names (SANs).
  - Enforces zero-downtime certificate rotation (90-day validity, automated renewal evaluation).
  - SHA-256 fingerprint hashing for out-of-band certificate verification.
- **In-Process Daemon SDN Supervisor Service (`SdnService`)**:
  - Manages overlay mesh state in the background supervisor.
  - IPC protocol endpoints: `GetSdnTopology`, `ApplySdnPolicy`, `RotateSdnKeys`, `GetPeerStatus`.
  - Auto-synthesizes platform network configurations on disk whenever policies or keypairs are mutated.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient::apply_remote_sdn_mesh`: Transmits and provisions WireGuard configurations and microsegmentation policies across remote SSH hosts.
  - `LifecycleEvent::SdnMeshReconfigured`, `SdnPacketDropped`, `SdnCertRotated` fire into embedded Lua scripts with zone, peer, and dropped packet metrics.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft sdn status [--json]`
  - `craft sdn up [--node node] [--json]`
  - `craft sdn down [--node node] [--json]`
  - `craft sdn policy [apply|show|reload] [--default-verdict drop|pass] [--json]`
  - `craft sdn peers [--zone zone] [--json]`
  - `craft sdn rotate-keys [--node node] [--json]`
  - `craft sdn audit [--from-zone zone] [--to-zone zone] [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Zero-Trust Mesh & Packet Filtering`) powered by ModalX.

### 3.14 Distributed Fault-Tolerant Consensus, Raft Clustering & Dynamic Split-Brain Arbitration (Phase 24)

- **Pure-Rust In-Memory Raft State Machine (`RaftEngine`)**:
  - Embedded deterministic consensus engine implementing standard Raft protocol (Leader, Candidate, Follower).
  - Randomized election timeouts (150ms-300ms) with 50ms leader heartbeat ticks.
  - Replicated append-only Write-Ahead Log (WAL) persisted on disk (`~/.craft/raft/wal/raft.wal`) with transactional line buffering.
  - Periodic snapshot compaction (`~/.craft/raft/snapshots/snapshot_<term>_<index>.json`) with prefix log truncation.
  - Monotonic fencing token generator: `(term << 32) | (commit_index & 0xFFFF_FFFF)`.
- **Advisory File Locked State Registry (`RaftRegistry`)**:
  - Persistent state (`state.toml`) holding current term, voted-for candidate, commit index, cluster membership, and active distributed locks.
  - Inter-process synchronization guaranteed via `fs2` advisory file locks on `~/.craft/run/locks/raft.lock`.
- **Pure-Rust Binary Transport & Wire Framing (`craft-net`)**:
  - 4-byte magic header `CRAFT_RAFT_MAGIC: [u8; 4] = [0x43, 0x52, 0x46, 0x54]` (`CRFT`) + 4-byte big-endian length prefix.
  - RPC frames: `RequestVoteArgs`/`Reply`, `AppendEntriesArgs`/`Reply`, `InstallSnapshotArgs`/`Reply`, `HeartbeatArgs`/`Reply`.
- **Dynamic Split-Brain Arbitration (`SplitBrainArbitrator`)**:
  - Evaluates cluster node weights (`ArbitrationWeight`) based on uptime, TCP round-trip latency, and jitter telemetry.
  - In an exact 50/50 partition (e.g. 2 vs 2 split in a 4-node cluster), the arbitrator uses the designated `edge_tie_breaker` (or highest weighted node) to designate which partition retains quorum, strictly blocking writes on sub-quorum partitions while preventing dual-leader split-brain conflicts.
- **Linearizable Distributed Lock Manager (`DistributedLockManager`)**:
  - Tracks cluster-wide mutex leases with monotonic 64-bit fencing tokens.
  - Guarantees strict mutual exclusion, prevents stale split-brain writes, and supports lease renewals and automated expiration pruning.
- **In-Process Daemon Supervisor Service (`RaftConsensusService`)**:
  - Dispatches 7 typed IPC requests: `GetRaftStatus`, `ProposeRaftCommand`, `AcquireDistributedLock`, `ReleaseDistributedLock`, `StepDownRaftLeader`, `TransferRaftLeadership`, `GetRaftLogs`.
  - Operates autonomously with zero external daemon dependencies.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient::get_remote_raft_status` and `propose_remote_raft_command` enable remote consensus querying and proposals over SSH.
  - `LifecycleEvent::RaftLeaderElected`, `RaftSplitBrainDetected`, `RaftLockContended` fire into embedded Lua scripts with term, leader, role, lock name, and fencing token context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft raft status [--json]` (with alias `craft consensus status`)
  - `craft raft propose --action <action> --data <data> [--json]`
  - `craft raft lock --name <name> [--holder <id>] [--lease <secs>] [--json]`
  - `craft raft unlock --name <name> [--holder <id>] [--json]`
  - `craft raft step-down [--json]`
  - `craft raft transfer --target <node-id> [--json]`
  - `craft raft logs [--limit <n>] [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Raft Consensus & Cluster Arbitration`) powered by ModalX.

### 3.15 Autonomous Distributed Consensus Reconfiguration, Multi-Raft Partitioning & Raft Log Compaction (Phase 29)

- **Online Joint Consensus Reconfiguration ($C_{\text{old}} \to C_{\text{old,new}} \to C_{\text{new}}$)**:
  - Supports zero-downtime cluster expansion, node replacement, and decommissioning.
  - Two-phase joint consensus writes a configuration change entry to WAL entering `JointConsensusPhase::Joint { c_old, c_new }`.
  - Quorum evaluation requires separate majority agreement from BOTH $C_{\text{old}}$ and $C_{\text{new}}$ before transitions:
    $$\text{Quorum}_{\text{joint}} = (\text{votes}(C_{\text{old}}) > |C_{\text{old}}| / 2) \land (\text{votes}(C_{\text{new}}) > |C_{\text{new}}| / 2)$$
  - Once committed, leader appends and commits a finalization entry transitioning to `JointConsensusPhase::Finalized` where only $C_{\text{new}}$ evaluates quorum.
- **Non-Voting Learner Catch-Up & Promotion Lifecycle**:
  - Nodes can join as non-voting learners (`LearnerSyncProgress`) without impacting active consensus quorum or election timeouts.
  - Leader replicates logs to learners and tracks `match_index`, `leader_last_index`, `is_caught_up` (within 10 entries), and `sync_percentage`.
  - Once caught up, administrators or autonomous orchestrators promote learners to voting members via joint consensus without risk of election latency spikes.
- **Multi-Raft Partitioning & Key Range Routing**:
  - Eliminates single-leader consensus bottlenecks by partitioning state across independent Raft groups (`MultiRaftPartition`, `MultiRaftRegistry`).
  - Application keys (e.g. server UUID, tenant ID) route to designated groups via lexicographical key ranges `[key_range_start..key_range_end]`:
    - Group 0 (Control Plane): Global cluster metadata, leases, server registrations, routing catalog.
    - Group 1+ (Data Shards): Sharded world state, region entities, partitioned logs.
  - Multi-Raft storage layout anchors per-group isolated WAL and snapshot directories under `~/.craft/raft/groups/<group_id>/wal/` and `snapshots/`.
- **High-Watermark Log Compaction & Streaming Snapshot Chunking**:
  - Compaction policy (`WalCompactionPolicy`) enforces high/low log entry watermarks (`entries_watermark = 50,000`), maximum log byte size (`max_log_bytes = 128 MB`), and minimum retain window (`retain_entries = 1,000`).
  - Point-in-time state machine serialization emits `RaftSnapshotMeta` and payload state files.
  - Wire streaming chops snapshots into 64 KB binary chunks (`SnapshotChunk`) tagged with sequential chunk indexes, total chunk counts, and CRC32 integrity checksums (`compute_crc32`).
  - Wire reassembly (`reassemble_snapshot_chunks`) validates byte lengths and CRC32 checksums before atomic disk swapping.
- **Pure-Rust Multiplexed Wire Envelopes (`craft-net`)**:
  - `RaftMessageEnvelope` multiplexes traffic across groups: `magic: [u8; 4] = [0x43, 0x52, 0x46, 0x54]`, `group_id: u64`, `sender_node_id: String`, `target_node_id: Option<String>`, `payload: RaftRpcMessage`.
  - Wire messages include `InstallSnapshotChunkArgs` and `InstallSnapshotChunkReply` alongside standard voting and heartbeat RPCs.
- **In-Process Daemon Multi-Raft Supervisor (`MultiRaftService`)**:
  - Supervises partition lifecycle, WAL compaction, and joint consensus transitions.
  - IPC endpoints: `RaftGetMultiRaftStatus`, `RaftReconfigureMembership`, `RaftTriggerCompaction`, `RaftRoutePartitionKey`, `RaftManagePartition`.
  - Prometheus metrics: `craft_raft_groups_total`, `craft_raft_log_compaction_runs_total`, `craft_raft_snapshot_bytes_total`, `craft_raft_snapshot_chunks_total`, `craft_raft_joint_consensus_transitions_total`.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient` provides `get_remote_multiraft_status`, `reconfigure_remote_membership`, `trigger_remote_log_compaction`, `route_remote_partition_key` over SSH.
  - `LifecycleEvent::RaftMembershipReconfigured`, `RaftCompactionCompleted`, `MultiRaftPartitionCreated` fire into embedded Lua scripts.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft raft status [--group <id>] [--json]`
  - `craft raft reconfigure [--group <id>] [--add <node>] [--remove <node>] [--learner <node>] [--promote <node>] [--demote <node>] [--json]`
  - `craft raft compact [--group <id>] [--index <n>] [--force] [--json]`
  - `craft raft partition list [--json]`
  - `craft raft partition route <key> [--json]`
  - `craft raft partition create --group <id> --name <name> -s <start> -e <end> [--leader <id>] [--peers <ids...>] [--json]`
  - `craft raft partition remove --group <id> [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Raft Consensus & Cluster Arbitration`) displaying Multi-Raft partitions, leaders, and compaction controls powered by ModalX.

### 3.16 Distributed Heterogeneous Cluster Orchestration, Zero-Downtime Live Migration & Global Anycast Session Continuity (Phase 30)

- **Iterative Pre-Copy Memory Convergence**:
  - Live migration is managed via `LiveMigrationPlan` through structured states: `Initializing -> PreCopying -> Freezing -> Checkpointing -> StateTransfer -> ResumingTarget -> SocketHandoff -> AnycastSwitched -> Completed` (or `Failed`/`RolledBack`).
  - `DirtyMemoryTracker` provides bitmask dirty tracking across page-aligned 4 KB allocations, recording dirty byte offsets and calculating iterative round stats (`PreCopyRound`).
  - Pre-copy evaluation (`evaluate_pre_copy_convergence`) determines whether dirty memory has reduced below the convergence threshold ($\le 10$ MB) or round limit (round $\ge 3$) to trigger the sub-150ms execution freeze window:
    $$\text{should\_freeze} = (\text{dirty\_bytes} \le \text{convergence\_threshold}) \lor (\text{round} \ge \text{max\_rounds})$$
  - Each pre-copy page chunk (`MemoryPageChunk`) is compressed with Zstandard and validated with 32-bit CRC32 checksums.
- **Sub-150ms Freeze Window & CRIU Process Checkpointing**:
  - During the freeze window, the source process is suspended (`MigrationStage::Freezing`) to record final memory deltas, active thread registers, and file descriptor mappings into `ServerCheckpointManifest`.
  - The manifest serializes process memory pages, open file paths, and player session states (`PlayerSessionDescriptor`), allowing bit-for-bit process restoration on the target node.
  - If target resume or state transfer fails, `MigrationService` automatically aborts and unpauses the source process within the SLA threshold, avoiding server corruption.
- **Pure-Rust Connection Splicing & Socket Handoff (`craft-net`)**:
  - `ConnectionSplicer` maintains per-player TCP state (`PlayerSocketHandoffFrame`), tracking inbound/outbound sequence numbers, window scaling, and TLS master keys.
  - Freeze-window packet buffering intercepts incoming client packets, queueing them in an in-memory buffer without dropping connections.
  - Upon target activation, queued frames are spliced and replayed into the target server socket with sequence continuity, preventing client-side disconnection timeouts.
  - Binary migration wire protocol framing uses magic header `CRAFT_MIGRATION_MAGIC: [u8; 4] = [0x43, 0x4D, 0x49, 0x47]` (`CMIG`), encoding messages: `Handshake`, `PreCopyChunk`, `FreezeNotice`, `StateManifest`, `ResumeAck`, `AbortNotice`.
- **Global Anycast BGP Prefix Steering (`AnycastBgpEngine`)**:
  - Dynamic routing engine generates production-grade BGP configuration files for BIRD, FRR, and ExaBGP daemons.
  - BGP communities (`BgpCommunity`: Standard `no-export`, `no-advertise`, or custom AS:Value) and AS path prepending allow fine-grained traffic engineering.
  - Dynamic route health evaluation monitors node health score, MSPT, and packet loss, dynamically switching announced prefixes between source and target nodes once socket handoff completes.
- **In-Process Daemon Migration Supervisor (`MigrationService`)**:
  - Manages active migration jobs, checkpoint persistence under `~/.craft/migrations/`, advisory file locking (`migrations.lock`), and background execution tasks.
  - IPC commands: `MigrationStartLive`, `MigrationGetStatus`, `MigrationAbort`, `MigrationList`, `AnycastRouteManage`.
  - Prometheus metrics: `craft_migration_active`, `craft_migration_duration_seconds`, `craft_migration_pre_copy_bytes_total`, `craft_migration_freeze_time_ms`, `craft_migration_completed_total`, `craft_migration_failed_total`, `craft_anycast_routes_active`.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient` provides `start_remote_live_migration`, `get_remote_migration_status`, and `manage_remote_anycast_route` over SSH connection pools.
  - `HookBus` fires lifecycle events: `LiveMigrationInitiated`, `LiveMigrationFreezeStarted`, `LiveMigrationCompleted`, `LiveMigrationRolledBack`.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft migrate live <server> --target-node <node> [--sla-freeze-ms <ms>] [--max-pre-copy-rounds <n>] [--convergence-threshold-mb <mb>] [--bgp-steer] [--json]`
  - `craft migrate status <migration-id> [--json]`
  - `craft migrate abort <migration-id> [--reason <reason>] [--json]`
  - `craft migrate list [--json]`
  - `craft anycast route --prefix <prefix> --node <node> --action <announce|withdraw|prepended> [--as-path-prepend <n>] [--json]`
  - Full-screen centered interactive TUI panel (`Tools -> Zero-Downtime Live Migration & Anycast Steering`) powered by ModalX.

### 3.17 Immutable Cryptographic Supply Chain Verification, Hermetic Isolation & Reproducible Artifact Signing (Phase 32)

- **In-Toto Statement v1 & SLSA Level 3 Build Provenance**:
  - Pure-Rust implementation of In-Toto Statement v1 (`InTotoStatement`) encapsulating `Subject` SHA-256 digests and predicates across SLSA v0.2 and v1.0 specifications (`SlsaPredicate`).
  - Strict validation verifies that physical artifact byte digests match attestation subject digests, asserting hermetic build environments and isolated network build steps.
- **Dead Simple Signing Envelope (DSSE) & Pre-Authentication Encoding (PAE)**:
  - Prevents canonicalization and parser mismatch attacks by wrapping payload data in DSSE envelopes (`DsseEnvelope`).
  - Cryptographic signatures are calculated exclusively over deterministic PAE byte buffers:
    $$\text{PAE}(t, p) = \text{"DSSEv1 "} \parallel \text{len}(t) \parallel \text{" "} \parallel t \parallel \text{" "} \parallel \text{len}(p) \parallel \text{" "} \parallel p$$
  - Supports Sigstore/Cosign bundles with ECDSA P-256 and Ed25519 signature schemes, verified against local `TrustAnchor` entries stored in `~/.craft/supply_chain/trust_anchors.json`.
- **RFC 6962 Rekor Transparency Log Merkle Tree Inclusion Proofs**:
  - Pure-Rust verification of binary Merkle tree inclusion proofs (`RekorInclusionProof`) without outbound internet dependencies.
  - Hashes leaf entries with domain separation byte `0x00` and intermediate node concatenations with byte `0x01`, evaluating left/right path directions up to the verified log root hash.
- **Hermetic Build Sandboxing & Bit-for-Bit Deterministic Packaging**:
  - `HermeticBuildRunner` establishes sanitized execution environments: wipes ambient host environment variables, injects canonical `SOURCE_DATE_EPOCH=1704067200`, `PATH=/usr/bin:/bin`, `TZ=UTC`, and confines network access to loopback.
  - Normalizes filesystem permissions (`0o755` executable/dir, `0o644` file) and produces bit-for-bit reproducible Zip release packages with fixed entry modification timestamps.
  - Generates seccomp-bpf filter descriptions restricting unauthorized syscalls.
- **In-Process Daemon Supervisor (`SupplyChainService`)**:
  - Manages active verification policies (`Strict`, `Audit`, `Disabled`) persisted under `~/.craft/supply_chain/policy.json` with advisory file locking (`supply_chain.lock`).
  - Dispatches sidecar discovery searching for `<artifact>.attestation.json` and `~/.craft/supply_chain/attestations/<sha256>.json`.
  - Exposes 5 typed IPC requests: `SupplyChainVerify`, `SupplyChainGetPolicy`, `SupplyChainSetPolicy`, `SupplyChainInspectAttestation`, `HermeticBuildRun`.
  - Exposes Prometheus metrics: `craft_supply_chain_verifications_total`, `craft_supply_chain_violations_total`, `craft_supply_chain_strict_blocked_total`, `craft_supply_chain_trust_anchors_active`.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient` provides `verify_remote_artifact`, `get_remote_supply_chain_policy`, and `set_remote_supply_chain_policy` over SSH connection pools.
  - `HookBus` fires lifecycle events: `SupplyChainVerified`, `SupplyChainViolationBlocked`, `HermeticBuildCompleted` with structured attestation context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft attest verify <path> [--attestation <path>] [--trust-anchors <path>] [--json]`
  - `craft attest inspect <attestation-path> [--json]`
  - `craft attest policy [get|set <strict|audit|disabled>] [--json]`
  - `craft attest sign <path> --key <key-path> [--key-type <type>] [--output <path>] [--json]`
  - `craft attest hermetic <build-dir> <command...> [--output <path>] [--json]`
  - Aliases: `craft verify`, `craft provenance`, `craft supply-chain`.
  - Full-screen centered interactive TUI panel (`Tools -> Supply Chain Provenance & Verification`) powered by ModalX.

### 3.18 Autonomous Self-Optimizing Memory Compaction, Transparent Hugepage Defragmentation & Kernel page_pool Offloading (Phase 35)

- **Buddy Allocator Fragmentation Indexing & Multi-Order Modeling**:
  - Models Linux physical buddy memory allocator states across 11 orders (Order 0: 4 KB to Order 10: 4 MB):
    $$S_i = 4096 \times 2^i, \quad i \in [0, 10]$$
  - Computes the external memory fragmentation index relative to target order $T$ (default Order 9 for 2 MB hugepages):
    $$\text{FragIndex}(T) = 1.0 - \frac{\sum_{i=T}^{10} N_i \cdot S_i}{\sum_{i=0}^{10} N_i \cdot S_i}$$
    Where $N_i$ is the count of free blocks at order $i$. An index approaching $1.0$ indicates extreme fragmentation where sufficient free bytes exist but contiguous allocation stalls will occur.
- **Autonomous Proactive Memory Compaction Engine (`CompactionOrchestrator`)**:
  - Two-pointer page compaction algorithm models free scanner advancing from lower zones and migrate scanner retreating from high zones.
  - Migrates movable pages, coalesces lower-order blocks ($2^i \to 2^{i+1}$), and tracks compaction efficiency:
    $$\text{Efficiency} = \frac{\text{hugepages formed} \times 512}{\text{pages migrated}}$$
  - Records cycle telemetry into `CompactionRegistry` under `~/.craft/compaction/registry.json` protected by `compaction.lock`.
- **Transparent Hugepage (THP) Defragmentation Policy**:
  - Inspects and configures Linux kernel sysfs tunables under `/sys/kernel/mm/transparent_hugepage/`:
    - `enabled`: `always`, `madvise`, `never`
    - `defrag`: `always`, `defer`, `defer+madvise`, `madvise`, `never`
  - Eliminates synchronous JVM compaction latency stalls during live game server tick processing by favoring asynchronous `defer+madvise` background kernel compaction.
- **Cache-Line & Page-Aligned Zero-Allocation Socket `page_pool`**:
  - Pre-allocates fixed memory pools aligned to cache lines (`#[repr(align(64))]`) and 4 KB memory pages to eliminate false sharing and kernel page boundary splits.
  - Employs atomic bitmask allocation indexing with RAII drop guards (`PageSliceRef`) for lock-free zero-allocation recycling:
    $$\text{Recycle Rate} = \frac{\text{allocations recycled}}{\text{total allocations}} = 100\%$$
  - Simulated kernel DMA ring-buffer ingestion sustains >3.6M pps and >7.0 GB/s network throughput with zero userspace heap pressure.
- **In-Process Daemon Supervisor (`CompactionService`)**:
  - Singleton supervisor coordinating proactive compaction sweeps, sysfs tunables, and metrics.
  - Exposes 5 typed IPC requests: `CompactionGetStatus`, `CompactionTriggerNow`, `CompactionConfigureThp`, `CompactionGetPoolStats`, `CompactionResetMetrics`.
  - Exposes Prometheus metrics: `craft_compaction_total_cycles`, `craft_compaction_pages_migrated_total`, `craft_compaction_hugepages_formed_total`, `craft_compaction_fragmentation_index`, `craft_compaction_thp_enabled`.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient` provides `get_remote_compaction_status`, `trigger_remote_compaction`, and `configure_remote_thp` over SSH.
  - `HookBus` fires lifecycle events: `MemoryCompactionCompleted`, `HighMemoryFragmentationDetected`, `ThpAllocationStallAlert`.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft memory status [--json]`
  - `craft memory compact [--dry-run] [--target-order <n>] [--json]`
  - `craft memory thp [--mode <always|madvise|never>] [--defrag <always|defer|defer_madvise|madvise|never>] [--json]`
  - `craft memory pool [--packets <n>] [--packet-size <bytes>] [--json]`
  - Aliases: `craft compaction`, `craft hugepages`, `craft pagepool`, `craft thp`.
  - Full-screen centered interactive TUI panel (`Tools -> Memory Compaction & Hugepage Defragmentation`) powered by ModalX.

### 3.19 Autonomous Dynamic Binary Instrumentation, Hardware Performance Counters & Cache Miss Profiling (Phase 37)

- **Hardware PMU Counter Types & Derived Metric Ratio Formulas**:
  - Direct hardware Performance Monitoring Unit (PMU) event tracking across 8 core event types: `CpuCycles`, `Instructions`, `L1DReadAccess`, `L1DReadMiss`, `LlcReadAccess`, `LlcReadMiss`, `BranchInstructions`, `BranchMisses`.
  - Computes microarchitectural execution efficiency ratios in real time:
    - Instructions Per Cycle (IPC):
      $$\text{IPC} = \frac{\text{Instructions}}{\text{CpuCycles}}$$
    - L1 Data Cache Misses Per Instruction (L1D CMPI):
      $$\text{L1D CMPI} = \frac{\text{L1D Read Misses}}{\text{Instructions}}$$
    - Last-Level Cache Misses Per Instruction (LLC CMPI):
      $$\text{LLC CMPI} = \frac{\text{LLC Read Misses}}{\text{Instructions}}$$
    - Branch Mispredictions Per Instruction (BMPI):
      $$\text{BMPI} = \frac{\text{Branch Misses}}{\text{Branch Instructions}}$$
  - Automatic classification of cache health: L1D CMPI thresholds (<0.02 nominal, 0.02-0.08 elevated, >0.08 severe bottleneck), LLC CMPI thresholds (<0.005 nominal, >0.01 severe memory bus stall).
- **Linux `perf_event_open` Syscall Abstraction & Autonomous Synthetic Fallback**:
  - Native Linux PMU probe attachment targeting game server child processes via `perf_event_open` syscall abstraction.
  - Automatically verifies kernel subsystem permissions (`/proc/sys/kernel/perf_event_paranoid` <= 1 or `CAP_PERFMON`/`CAP_SYS_ADMIN`).
  - Seamlessly falls back to high-fidelity synthetic hardware event simulation in unprivileged containers, hypervisors, macOS, and Windows environments, maintaining 100% operational uptime without runtime panics.
- **Multi-Language Symbol Demangler (`demangle_symbol`)**:
  - Pure-Rust symbol parser with zero external C library dependencies.
  - Demangles Rust Itanium symbols (`_ZN...` -> readable module paths).
  - Demangles C++ Itanium symbols (`_Z...` -> class and function paths).
  - Demangles JVM JIT bytecode method descriptors (`Lnet/minecraft/server/MinecraftServer;tick()V` -> `net.minecraft.server.MinecraftServer.tick()`).
- **Ring-Buffered Sampler & Synthetic Memory Churn Benchmark**:
  - `PmuSampler` coordinates continuous event sampling with circular ring buffer storage (`PmuSampleRecord`), bounded memory limits, and thread-safe atomic access.
  - `benchmark_synthetic_memory_churn` runs cache-thrashing access loops jumping across 64-byte cache line strides, provoking 14.8x L1 cache miss surges and 23.7x LLC miss spikes to validate detection thresholds.
- **In-Process Daemon Supervisor (`PmuService`)**:
  - Coordinates background PMU sampling sessions, aggregates hot symbols, and enforces advisory file-locked persistence (`pmu.lock`, `~/.craft/pmu/probes.json`, `~/.craft/pmu/state.json`).
  - Exposes 7 typed IPC requests: `PmuGetStatus`, `PmuStartSampling`, `PmuStopSampling`, `PmuSampleNow`, `PmuGetHotspots`, `PmuRunBench`, `PmuResetMetrics`.
  - Exposes Prometheus metrics: `craft_pmu_instructions_total`, `craft_pmu_cpu_cycles_total`, `craft_pmu_l1d_misses_total`, `craft_pmu_llc_misses_total`, `craft_pmu_branch_misses_total`, `craft_pmu_ipc`, `craft_pmu_l1d_cmpi`, `craft_pmu_llc_cmpi`, `craft_pmu_bmpi`, `craft_pmu_active_probes`.
- **Remote Federation & Scripting Hook Bus**:
  - `RemoteCraftClient` provides `get_remote_pmu_status`, `start_remote_pmu_sampling`, `get_remote_pmu_hotspots`, and `reset_remote_pmu_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `PmuHotspotDetected`, `CacheMissThresholdExceeded`, `BranchMispredictionSurge` with structured metrics context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft pmu status [--server <name>] [--json]`
  - `craft pmu sample [--server <name>] [--interval-ms <ms>] [--duration-s <s>] [--json]`
  - `craft pmu hotspots [--limit <n>] [--server <name>] [--json]`
  - `craft pmu bench [--iterations <n>] [--array-size <bytes>] [--json]`
  - `craft pmu reset-metrics [--server <name>] [--json]`
  - Aliases: `craft hw-counters`, `craft cache-profile`, `craft counters`.
  - Full-screen centered interactive TUI panel (`Tools -> Hardware PMU Counters & Cache Miss Profiling`) powered by ModalX.

### 3.14. Autonomous Memory-Mapped Persistent Shared Memory (POSIX shm), Zero-Copy IPC & High-Speed Ring Bus (Phase 38)
- **POSIX Shared Memory & Mmap Abstraction (`craft-core`)**:
  - Pure-Rust memory mapping via `libc::mmap` / `libc::munmap` on Unix/Linux, and pure-Rust memory fallback on Windows.
  - Advisory file locking (`shm.lock`) on `~/.craft/shm/registry.json` prevents split-brain configuration updates and racing memory re-allocations.
  - Zero-copy segment configuration (`ShmSegmentConfig`), persistent metadata (`ShmSegmentMeta`), and channel categorization (`ShmChannelType`: `TickTelemetry`, `LogStream`, `CommandQueue`, `EventBus`, `Custom`).
- **Cacheline-Aligned Lock-Free SPSC Ring Buffers**:
  - `ShmRingHeader` is engineered with explicit 64-byte alignment: `#[repr(C, align(64))]`.
  - Head (`AtomicU64`) and Tail (`AtomicU64`) pointers are separated by 56 bytes of padding so that each counter resides on an independent L1 CPU cacheline, totally eliminating false sharing.
  - Release-Acquire memory fences (`Ordering::Release`, `Ordering::Acquire`) synchronize writes and reads across disjoint OS process memory spaces without syscall overhead or mutex locks.
  - Each slot begins with a 64-byte cacheline-aligned `ShmSlotHeader` recording payload length, slot flags (`SHM_FLAG_READY`, `SHM_FLAG_READ`), monotonic sequence number, and nanosecond timestamp.
- **High-Throughput Zero-Copy Ring Bus (`craft-net`)**:
  - `ShmRingBus` coordinates active `ShmProducer` and `ShmConsumer` handles.
  - Automatic power-of-two wrap-around indexing using bitwise masking (`slot_index = seq & (slot_count - 1)`).
  - Synthetic zero-copy benchmark (`benchmark_shm_throughput`) demonstrates sustained throughput >3.0M msgs/sec, >360 MB/s transfer bandwidth, and ~330 ns average IPC latency.
- **Daemon Supervision, Watchdog Sweeps & Prometheus Telemetry (`craft-daemon`)**:
  - `ShmService` maintains a singleton registry of active memory-mapped channels, tracks open lease counters, sweeps stale leases via automated watchdog sweeps, and ensures state persistence in `~/.craft/shm/state.json`.
  - 7 typed IPC requests and responses: `ShmGetStatus`, `ShmCreateChannel`, `ShmCloseChannel`, `ShmWriteEvent`, `ShmReadEvents`, `ShmRunBench`, `ShmResetMetrics`.
  - Prometheus metrics exposition (`craft_shm_*`): `craft_shm_active_segments`, `craft_shm_total_allocated_bytes`, `craft_shm_messages_written_total`, `craft_shm_messages_read_total`, `craft_shm_avg_latency_ns`, `craft_shm_watchdog_reclaimed_segments`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_shm_status`, `create_remote_shm_channel`, `close_remote_shm_channel`, `run_remote_shm_bench`, and `reset_remote_shm_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `ShmChannelOpened`, `ShmChannelClosed`, `ShmLeaseExpired`, `ShmThroughputThresholdExceeded` with structured metrics context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft shm status [--server <name>] [--json]`
  - `craft shm create -s <server> -c <channel> [--slot-size <bytes>] [--slots <count>] [--json]`
  - `craft shm close -s <server> -c <channel> [--json]`
  - `craft shm bench [--messages <count>] [--size <bytes>] [--json]`
  - `craft shm reset-metrics [--server <name>] [--json]`
  - Aliases: `craft shared-memory`, `craft shmem`, `craft ipc-ring`.
  - Full-screen centered interactive TUI panel (`Tools -> Memory-Mapped Shared Memory & Zero-Copy Ring Bus`) powered by ModalX.

### 3.15. Autonomous Dynamic Binary Rewriting, Trampoline Patching & Zero-Downtime Hot Code Replacement (Phase 39)
- **Core Trampoline & Instruction Rewriting (`craft-core`)**:
  - Architecture-specific models (`ArchInstructionSet`, `TrampolinePatchType`: `Rel32Jmp`, `Abs64Jmp`, `Aarch64BranchImm`, `Aarch64LiteralLdr`, `BytecodeMethodSwap`).
  - Target symbol representations (`PatchTargetType::NativeSymbol`, `PatchTargetType::JvmClassMethod` with `Display` trait implementation).
  - CFG Dominance Pre-flight Validator (`ControlFlowGraph`, `DominanceValidator` computing entry-dominating blocks, natural loop headers, and hook boundary safety to eliminate invalid mid-instruction branches).
  - Page permission safety guard (`PagePermissionGuard` verifying memory protection transitions `PROT_READ | PROT_WRITE | PROT_EXEC`).
  - Advisory file locking (`patch.lock`) on `~/.craft/patches/registry.json` prevents concurrent patch collisions.
- **Dynamic Trampoline Engine & Bytecode Diffing (`craft-net`)**:
  - `NativeTrampolineEngine`: encodes 5-byte rel32 jumps (`0xE9 <rel32>`), 14-byte 64-bit absolute indirect jumps (`FF 25 00 00 00 00 <u64>`), 4-byte AArch64 immediate branch (`B <imm26>`), and 16-byte AArch64 literal load (`LDR X16, #8; BR X16; <u64>`).
  - Stolen prologue preservation and shadow trampoline linking back to the unpatched continuation address.
  - `JvmBytecodeEngine`: bytecode class method replacement manifests and unified disassembly diff computation.
  - Pre-flight `PatchSafetyVerifier` and synthetic throughput benchmark (`benchmark_patch_throughput`) demonstrating >690k patches/sec generation throughput and ~0.17 us average latency.
- **Daemon Supervision, Prologue Backup & Rollback (`craft-daemon`)**:
  - `DynamicPatchService` singleton coordinating atomic patch lifecycle, prologue disk backups (`~/.craft/patches/backups/`), CFG dominance validation, state fallback persistence in `~/.craft/patches/state.json`, and Prometheus metric exposition (`craft_patch_*`).
  - 6 typed IPC requests and responses: `PatchGetStatus`, `PatchApply`, `PatchRollback`, `PatchGetDiff`, `PatchRunBench`, `PatchResetMetrics`.
  - Prometheus metrics exposition (`craft_patch_*`): `craft_patch_active`, `craft_patch_total_applied`, `craft_patch_total_rollbacks`, `craft_patch_safety_rejections`, `craft_patch_avg_apply_micros`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_patch_status`, `apply_remote_patch`, `rollback_remote_patch`, `get_remote_patch_diff`, `run_remote_patch_bench`, and `reset_remote_patch_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `PatchApplied`, `PatchRolledBack`, `PatchSafetyCheckFailed` with structured metrics context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft patch status [--server <name>] [--json]`
  - `craft patch apply -s <server> -p <patch> -t <target> [-b <bytes>] [--json]`
  - `craft patch rollback -s <server> -p <patch> [--json]`
  - `craft patch diff -s <server> -p <patch> [--json]`
  - `craft patch bench [--iterations <count>] [--json]`
  - `craft patch reset-metrics [--server <name>] [--json]`
  - Aliases: `craft hot-swap`, `craft hotpatch`, `craft live-patch`.
  - Full-screen centered interactive TUI panel (`Tools -> Dynamic Binary Rewriting & Trampoline Hot Patching`) powered by ModalX.

### 3.16. Autonomous MicroVM Sandboxing, Lightweight Firecracker/KVM Isolation & Sub-50ms Cold Starts (Phase 40)
- **Core KVM Models, Virtio Devices & Registry (`craft-core`)**:
  - Foundational KVM kernel ioctl abstractions (`KvmCapability`, `KvmUserMemoryRegion`, `KvmRegs`, `KvmSregs`, `KvmExitReason`).
  - Virtio device specifications (`VirtioDeviceType`: `Net`, `Block`, `Vsock`, `Console`; `VirtioQueue`, `VirtioDescriptor`).
  - AF_VSOCK host-guest communication framing (`VsockPacketHeader`, `VsockAddr`, `VsockOp` for connection requests, responses, data payloads, credits, and resets).
  - MicroVM state models (`MicroVmConfig`, `MicroVmState`, `MicroVmDescriptor`, `MicroVmStatusSummary`, `MicroVmBenchmarkMetrics`).
  - Advisory file locking (`vm.lock`) protecting persistent registry state under `~/.craft/vms/` (`vm_dir`, `vm_registry_file`, `vm_state_file`, `vm_lock`).
- **Network TAP Bridge Driver, AF_VSOCK Multiplexer & Benchmarking (`craft-net`)**:
  - `TapBridgeDriver`: manages TAP interfaces, MTU configuration, MAC address generation, and packet bridging.
  - `VsockMultiplexer`: multiplexes host-guest AF_VSOCK streams with port routing and round-trip payload echo validation.
  - Synthetic cold start and communication benchmark (`benchmark_microvm_boot`): verifies sub-20ms cold boot latency (<50ms threshold) and >500k vsock msgs/sec.
- **MicroVM Supervisor Service & Prometheus Telemetry (`craft-daemon`)**:
  - `MicroVmService`: manages in-process VM state machine, enforces jailer directory isolation (`~/.craft/vms/jails/<vm_id>/`), manages memory overcommit scheduling, background watchdog sweeps, and dispatches scripting lifecycle hooks.
  - 6 typed IPC requests and responses: `VmGetStatus`, `VmSpawn`, `VmStop`, `VmInspect`, `VmRunBench`, `VmResetMetrics`.
  - Prometheus metrics exposition (`craft_vm_*`): `craft_vm_active_instances`, `craft_vm_total_spawned`, `craft_vm_total_terminated`, `craft_vm_cold_start_duration_ms`, `craft_vm_vsock_packets_total`, `craft_vm_memory_allocated_bytes`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_vm_status`, `spawn_remote_vm`, `stop_remote_vm`, `inspect_remote_vm`, `run_remote_vm_bench`, and `reset_remote_vm_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `MicroVmSpawned`, `MicroVmTerminated`, `MicroVmIsolationAlert` with structured VM metrics context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft vm status [--server <name>] [--json]`
  - `craft vm spawn -n <name> [-s <server>] [--vcpus <n>] [--mem <mb>] [--kernel <path>] [--rootfs <path>] [--json]`
  - `craft vm stop -i <vm_id> [--json]`
  - `craft vm inspect -i <vm_id> [--json]`
  - `craft vm bench [--iterations <count>] [--json]`
  - `craft vm reset-metrics [--json]`
  - Aliases: `craft microvm`, `craft firecracker`, `craft kvm`.
  - Full-screen centered interactive TUI panel (`Tools -> Autonomous MicroVM Sandboxing & KVM Isolation`) powered by ModalX.

### 3.17. Autonomous AI-Guided Static Analysis, Real-Time Memory Leak Detection & Automated Core Dump Triaging (Phase 41)
- **Core Crash Models, Leak Candidates & Registry (`craft-core`)**:
  - Foundational crash taxonomy models (`CrashType`: `NativeCoreDump`, `JvmHsErr`, `MemoryLeak`, `StaticAnalysisViolation`, `Custom`; `CrashSeverity`: `Low`, `Medium`, `High`, `Critical`).
  - Native ELF core dump header parsing models (`ElfCoreDumpHeader`, `ElfCoreNoteType`, `ElfCoreParsedInfo` capturing signal, faulting address, and register state).
  - JVM crash log parsing models (`JvmCrashLogParsed` capturing signal, problematic frame, native stack, Java stack frames, and active compilation task).
  - Real-time memory leak detection models (`AllocationRecord`, `LeakCandidate` with callsite, monotonic growth score, leak rate bytes/sec, and orphan allocation classification).
  - Triage report and metrics models (`CrashRemediationAction`, `CrashTriageReport`, `CrashTriageStatusSummary`, `CrashTriageBenchmarkMetrics`).
  - Advisory file locking (`crash.lock`) protecting persistent registry state under `~/.craft/crash/` (`crash_dir`, `crash_registry_file`, `crash_state_file`, `crash_reports_dir`, `crash_dumps_dir`, `crash_lock`).
- **Binary ELF Parser, hs_err Dissector, Leak Engine & AI Advisor (`craft-net`)**:
  - `ElfCoreDumpParser`: parses ELF 64-bit and 32-bit core dumps (`0x7f454c46`), extracts `PT_NOTE` and `PT_LOAD` program headers, unpacks `NT_PRSTATUS` note descriptors, extracts signal numbers, faulting addresses, and thread execution contexts.
  - `JvmHsErrParser`: parses OpenJDK and HotSpot `hs_err_pid*.log` files, extracts termination signals (`SIGSEGV`, `SIGBUS`, etc.), identifies problematic native or JIT frames (`# C [libnative.so+0x1234]`), parses Java call stacks, and extracts active JIT compilation tasks.
  - `MemoryLeakDetector`: maintains sliding-window allocation tracking, computes linear growth regression slopes, calculates orphan rates (`unfreed / total`), detects monotonic leaks exceeding configurable thresholds (e.g. 0.85 slope score), and generates actionable leak candidates.
  - `AiTriageAdvisor`: executes deterministic rule-based root cause analysis across native crashes, JVM faults, and memory exhaustion; generates actionable playbooks (`RestartProcess`, `ApplyHotPatch`, `AdjustJvmHeap`, `RollbackPlugin`, `TuneGcParameters`, `IsolateNetworkTraffic`, `ManualInvestigationRequired`) with confidence scores.
  - Synthetic benchmarking (`benchmark_crash_triage`): validates high-throughput dump parsing (>200 dumps/sec) and allocation diffing (>500k ops/sec).
- **Daemon Supervision, Ingestion Loop & Prometheus Telemetry (`craft-daemon`)**:
  - `CrashTriageService`: manages in-process triage singleton via `OnceLock`, monitors crash ingestion directories, runs automated memory leak sweeps, applies safe auto-remediation playbooks, and dispatches scripting lifecycle hooks.
  - 6 typed IPC requests and responses: `CrashGetStatus`, `CrashTriageFile`, `CrashListReports`, `CrashGetReport`, `CrashRunBench`, `CrashResetMetrics`.
  - Prometheus metrics exposition (`craft_crash_*`): `craft_crash_triaged_total`, `craft_crash_native_core_dumps_total`, `craft_crash_jvm_hs_err_total`, `craft_crash_active_leaks`, `craft_crash_remediated_total`, `craft_crash_avg_triage_latency_micros`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_crash_status`, `triage_remote_crash_file`, `list_remote_crash_reports`, `get_remote_crash_report`, `run_remote_crash_bench`, and `reset_remote_crash_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `CrashTriageCompleted`, `MemoryLeakDetected`, `CriticalFaultRemediated` with structured crash metrics context.
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft crash status [--server <name>] [--json]`
  - `craft crash triage -f <file> [-s <server>] [--json]`
  - `craft crash list [--server <name>] [--limit <n>] [--json]`
  - `craft crash inspect -i <report_id> [--json]`
  - `craft crash bench [--iterations <n>] [--json]`
  - `craft crash reset-metrics [--server <name>] [--json]`
  - Aliases: `craft triage`, `craft core-dump`, `craft leak-detect`.
  - Full-screen centered interactive TUI panel (`Tools -> Autonomous Crash Triaging & Memory Leak Detection`) powered by ModalX.

### 3.18. Autonomous RDMA Network Acceleration, InfiniBand/RoCE Direct Memory Offloading & Sub-Microsecond Inter-Server Fabric (Phase 42)
- **Core RDMA Models, Verbs Abstractions & Advisory Locking (`craft-core`)**:
  - Foundational transport and verbs models (`RdmaTransportType`: `RoceV2`, `InfiniBand`, `SoftRoceFallback`, `EmulatedMemoryVerbs`; `RdmaQpType`: `ReliableConnected`, `UnreliableDatagram`, `ReliableDatagram`, `ExtendedReliableConnected`; `RdmaQpState`: `Reset`, `Init`, `ReadyToReceive`, `ReadyToSend`, `SendQueueDrain`, `SendQueueError`, `Error`).
  - Bitwise memory access flags with constants (`RdmaAccessFlags`: `LOCAL_WRITE`, `REMOTE_WRITE`, `REMOTE_READ`, `REMOTE_ATOMIC`, `MW_BIND`).
  - Memory region descriptor (`MemoryRegionDescriptor` with `mr_id`, `lkey`, `rkey`, `addr`, `length`, `protection_domain_id`, `access_flags`).
  - Queue pair configuration and work requests (`QueuePairConfig`, `WorkRequest`, `WorkOpcode`, `WorkCompletion`, `WorkCompletionStatus`).
  - Fabric topology and telemetry models (`RdmaLinkStatus`: `Active`, `Degraded`, `Down`, `FallbackActive`; `RdmaPeerEndpoint`, `RdmaStatusSummary`, `RdmaBenchmarkMetrics`).
  - Advisory file locking (`rdma.lock`) protecting persistent registry state under `~/.craft/rdma/` (`rdma_dir`, `rdma_registry_file`, `rdma_state_file`, `rdma_lock`, `rdma_mr_path`).
- **RoCE v2 Packet Framing, Protection Domains & Verbs Engine (`craft-net`)**:
  - `RoceV2Packet`: implements pure-Rust 12-byte Base Transport Header (`RoceV2Bth` with opcode, solicited event, migration req, pad count, transport header version, partition key, dest QP, 24-bit Packet Sequence Number, and AckReq flag) and 16-byte Remote Extended Transport Header (`RoceV2Reth` with 64-bit virtual address, 32-bit remote key, and 32-bit DMA length).
  - Invariant CRC computation (`compute_roce_icrc`): calculates 32-bit CRC over invariant header fields and payload per RFC 1952 / RFC 3720 specification.
  - `RdmaProtectionDomain`: manages local memory allocations, validates remote access permissions (read/write/atomic), performs bounds checks against registered memory buffers, and prevents illegal memory access.
  - `RdmaVerbsEngine`: manages the full Queue Pair lifecycle (`Reset -> Init -> ReadyToReceive -> ReadyToSend`), posts send/receive work requests, and polls completion queues with zero memory allocations.
  - `RdmaFailoverBridge`: monitors link heartbeat degradation, tracks timeout and CRC error thresholds, triggers transparent failover to TCP fallback sockets, and autonomously recovers to RDMA when link health restores.
  - Synthetic fabric benchmark (`benchmark_rdma_fabric`): validates sub-microsecond latency (658 ns), >915k operations/sec throughput, and ~30 Gbps memory offloading.
- **Daemon Supervision, In-Process Verbs Service & Prometheus Telemetry (`craft-daemon`)**:
  - `RdmaService`: manages in-process singleton via `OnceLock`, synchronizes memory region registrations, coordinates peer endpoint connections, runs zero-copy benchmarks, and tracks cumulative metrics.
  - 6 typed IPC requests and responses: `RdmaGetStatus`, `RdmaRegisterMr`, `RdmaConnectPeer`, `RdmaListPeers`, `RdmaRunBench`, `RdmaResetMetrics`.
  - Prometheus metrics exposition (`craft_rdma_*`): `craft_rdma_active_qps`, `craft_rdma_registered_mrs`, `craft_rdma_total_registered_bytes`, `craft_rdma_tx_bytes_total`, `craft_rdma_rx_bytes_total`, `craft_rdma_avg_latency_nanos`, `craft_rdma_failover_events_total`, `craft_rdma_link_status`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_rdma_status`, `register_remote_rdma_mr`, `connect_remote_rdma_peer`, `list_remote_rdma_peers`, `run_remote_rdma_bench`, and `reset_remote_rdma_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `RdmaLinkEstablished`, `RdmaFailoverTriggered`, `RdmaLatencySpike` with structured RDMA context (`rdma_peer_node`, `rdma_transport`, `rdma_latency_nanos`, `rdma_bandwidth_gbps`).
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft rdma status [--server <name>] [--json]`
  - `craft rdma mr-register [-b <size>] [--read-only] [--atomic] [--json]`
  - `craft rdma connect -p <peer> -a <addr> [-t <transport>] [-m <mtu>] [--json]`
  - `craft rdma peers [--json]`
  - `craft rdma bench [-p <peer>] [-i <iterations>] [-s <size>] [--json]`
  - `craft rdma reset-metrics [--json]`
  - Aliases: `craft infiniband`, `craft roce`, `craft verbs`.
  - Full-screen centered interactive TUI panel (`Tools -> Autonomous RDMA Network Acceleration & Sub-Microsecond Fabric`) powered by ModalX.

### 3.19. Autonomous eBPF XDP Hardware Offloading, SmartNIC Acceleration & P4 Programmable Data Plane Line-Rate Switching (Phase 43)
- **Core SmartNIC Models, P4 Match-Action Tables & Registry (`craft-core`)**:
  - Foundational hardware vendor models (`SmartNicVendor`: `NvidiaBluefield`, `AmdPensando`, `IntelIpu`, `NetronomeAgilio`, `GenericP4Emulated`).
  - Offload execution modes (`OffloadMode`: `HardwareAsic`, `Driver`, `SoftwareGeneric`, `EmulatedP4Software`).
  - Offload protocol taxonomy (`OffloadProtocol`: `MinecraftJavaSlp`, `BedrockRaknet`, `ValveA2s`, `DdosMitigation`, `CustomP4`).
  - P4 match-action table structures (`P4MatchField`: `Exact`, `Ternary`, `Lpm`, `Range`; `P4ActionType`: `Drop`, `ForwardPort`, `SendPongDirect`, `SendSynCookie`, `RateLimitToken`, `PassToHost`; `P4TableEntry`, `P4MatchActionTable`).
  - Device and rule descriptors (`SmartNicDeviceInfo`, `SmartNicOffloadRule`, `SmartNicStatusSummary`, `SmartNicBenchmarkMetrics`).
  - Advisory file locking (`smartnic.lock`) protecting persistent registry state under `~/.craft/smartnic/` (`smartnic_dir`, `smartnic_registry_file`, `smartnic_state_file`, `smartnic_lock`, `smartnic_p4_path`).
- **Stateless Wire Responders, P4 Pipeline & Fallback Bridge (`craft-net`)**:
  - Stateless wire-level pong synthesizers:
    - `synthesize_slp_pong`: constructs Minecraft Java Server List Ping (SLP) status JSON and VarInt framing without waking JVM or host processes.
    - `synthesize_raknet_unconnected_pong`: constructs Bedrock UDP `0x1c` Unconnected Pong response with server GUID, timestamp, and 16-byte magic token.
  - `P4PipelineEngine`: evaluates ingress packets across match-action tables with exact match, ternary bitmasking, longest prefix match (LPM), and numerical range checks with priority sorting.
  - `SmartNicOffloadEngine`: tracks ASIC TCAM capacity (e.g. 65,536 entries), counts hardware hits and byte volumes, and flags TCAM saturation.
  - `SmartNicFallbackBridge`: monitors hardware engine state and executes seamless failover to host driver XDP (`XDP_FLAGS_DRV_MODE`) upon TCAM saturation or ASIC fault.
  - Synthetic line-rate benchmark (`benchmark_smartnic_line_rate`): verifies >3.18 Mpps throughput, >1.63 Gbps bandwidth, sub-microsecond ASIC latency (~313 ns), and 0.0% host CPU overhead.
- **Daemon Supervision, SmartNIC Service & Prometheus Telemetry (`craft-daemon`)**:
  - `SmartNicService`: manages in-process singleton via `OnceLock`, installs and removes P4 rules, synchronizes TCAM state, executes line-rate benchmarks, and exposes metrics.
  - 6 typed IPC requests and responses: `SmartNicGetStatus`, `SmartNicInstallRule`, `SmartNicRemoveRule`, `SmartNicListRules`, `SmartNicRunBench`, `SmartNicResetMetrics`.
  - Prometheus metrics exposition (`craft_smartnic_*`): `craft_smartnic_active_devices`, `craft_smartnic_installed_rules`, `craft_smartnic_tcam_usage_percent`, `craft_smartnic_offloaded_packets_total`, `craft_smartnic_offloaded_bytes_total`, `craft_smartnic_fallback_events_total`, `craft_smartnic_host_cpu_saved_percent`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_smartnic_status`, `install_remote_smartnic_rule`, `remove_remote_smartnic_rule`, `list_remote_smartnic_rules`, `run_remote_smartnic_bench`, and `reset_remote_smartnic_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `SmartNicOffloadInstalled`, `SmartNicTcamSaturated`, `SmartNicFallbackEngaged` with structured context (`smartnic_device_id`, `smartnic_rule_id`, `smartnic_tcam_percent`, `smartnic_offload_mode`).
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft smartnic status [--server <name>] [--json]`
  - `craft smartnic rule-add -r <id> [-t <proto>] [-a <action>] [-p <port>] [-c <cidr>] [--priority <n>] [--json]`
  - `craft smartnic rule-rm -r <id> [--json]`
  - `craft smartnic rules [--server <name>] [--json]`
  - `craft smartnic bench [-i <iterations>] [-b <packet-size>] [--json]`
  - `craft smartnic reset-metrics [--json]`
  - Aliases: `craft p4`, `craft nic-offload`.
  - Full-screen centered interactive TUI panel (`Tools -> SmartNIC Hardware Offload & P4 Line-Rate Switching`) powered by ModalX.

### 3.20. Autonomous Distributed Inter-Server Memory Fabric, Remote Paged Compaction & Cluster NVRAM Pool (Phase 44)
- **Core Memory Fabric Models, CXL/NVRAM Descriptors & Registry (`craft-core`)**:
  - Memory tier hierarchy (`MemoryTier`: `LocalDram`, `CxlPmem`, `RemoteRdmaDram`, `RemoteNvram`).
  - Page memory protection bits (`PageProtection`: `Read`, `ReadWrite`, `ReadWriteExec`).
  - Virtual page and cluster topology descriptors (`RemotePageDescriptor`, `MemFabricNodeInfo`, `MemFabricStatusSummary`, `MemFabricBenchmarkMetrics`).
  - Advisory file locking (`memfabric.lock`) protecting persistent registry state under `~/.craft/memfabric/` (`memfabric_dir`, `memfabric_registry_file`, `memfabric_state_file`, `memfabric_lock`, `memfabric_page_path`).
- **Remote Paging Engine, userfaultfd Interception & Dimension Memory Fabric (`craft-net`)**:
  - `RemotePagingEngine`: aggregates local DRAM, CXL persistent memory, and remote NVRAM pools; manages virtual page table (`0x7fff_0000_0000..`), resident page buffers, and remote RDMA storage mappings.
  - `UserfaultPageHandler`: implements sub-microsecond `userfaultfd` (`UFFDIO_COPY`) page fault interception, pulling remote NVRAM pages into local DRAM via zero-copy RDMA verbs with average resolution latency <2.0 us.
  - `DimensionMemoryFabric`: tracks Minecraft dimension memory pages (`overworld`, `the_nether`, `the_end`), coordinates transparent bulk eviction of dormant dimension chunks to remote NVRAM pool freeing local DRAM, and auto-restores pages via page faults when players teleport.
  - Synthetic zero-copy remote paging benchmark (`benchmark_remote_paging`): verifies sustained throughput >340k pages/sec and sub-2.0 us average fault resolution latency.
- **Daemon Supervision, MemFabric Service & Prometheus Telemetry (`craft-daemon`)**:
  - `MemFabricService`: singleton managing in-process remote paging engine, dimension memory fabric, background compaction sweeps, and Prometheus telemetry exposition (`craft_memfabric_*`).
  - 6 typed IPC requests and responses: `MemFabricGetStatus`, `MemFabricAllocatePage`, `MemFabricEvictDimension`, `MemFabricListPages`, `MemFabricRunBench`, `MemFabricResetMetrics`.
  - Prometheus metrics exposition (`craft_memfabric_*`): `craft_memfabric_active_nodes`, `craft_memfabric_total_dram_bytes`, `craft_memfabric_allocated_dram_bytes`, `craft_memfabric_total_nvram_bytes`, `craft_memfabric_allocated_nvram_bytes`, `craft_memfabric_dram_utilization_percent`, `craft_memfabric_nvram_utilization_percent`, `craft_memfabric_total_pages_managed`, `craft_memfabric_remote_pages_count`, `craft_memfabric_page_faults_total`, `craft_memfabric_remote_evictions_total`, `craft_memfabric_avg_page_fault_latency_nanos`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_memfabric_status`, `allocate_remote_memfabric_page`, `evict_remote_memfabric_dimension`, `list_remote_memfabric_pages`, `run_remote_memfabric_bench`, and `reset_remote_memfabric_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `MemFabricPageFaultResolved`, `MemFabricDimensionPaged`, `MemFabricPoolSaturated` with structured context (`memfabric_page_id`, `memfabric_vaddr`, `memfabric_dimension`, `memfabric_latency_nanos`, `memfabric_pages_count`, `memfabric_bytes_freed`, `memfabric_dram_percent`, `memfabric_nvram_percent`).
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft memfabric status [--server <name>] [--json]`
  - `craft memfabric page-alloc -p <id> [-z <size>] [-t <tier>] [-d <dimension>] [--json]`
  - `craft memfabric evict-dim -d <dimension> [-n <node>] [--json]`
  - `craft memfabric pages [--server <name>] [--json]`
  - `craft memfabric bench [-i <iterations>] [-z <page-size>] [--json]`
  - `craft memfabric reset-metrics [--json]`
  - Aliases: `craft memfabric`, `craft cxl`, `craft nvram`, `craft paging`.
  - Full-screen centered interactive TUI panel (`Tools -> Distributed Memory Fabric, CXL & Remote NVRAM Pool`) powered by ModalX.

### 3.21. Autonomous Zero-Copy Storage Fabrics, NVMe-oF Target & Distributed Flash Block Pool (Phase 45)
- **Core NVMe-oF Models, Flash Pool Descriptors & Advisory Locking (`craft-core`)**:
  - Transport protocols (`NvmeTransportType`: `Rdma`, `Tcp`, `Loopback`, `EmulatedPci`).
  - Subsystem classifications (`NvmeSubsystemType`: `Nvm`, `Discovery`, `Admin`).
  - Port configurations (`NvmePort` with `port_id`, `trtype`, `traddr`, `trsvcid`, `status`).
  - Target namespace descriptors (`NvmeNamespaceDescriptor` with `nsid`, `size_blocks`, `block_size`, `capacity_bytes`, `allocated_bytes`, `server_id`, `dimension`, `thin_provisioned`, `read_only`).
  - Target subsystem descriptors (`NvmeSubsystemDescriptor` with `nqn`, `subsys_type`, `namespaces`, `ports`, `controllers`, `status`).
  - Storage telemetry and benchmark metrics (`NvmeStatusSummary`, `NvmeBenchmarkMetrics`).
  - Advisory file locking (`nvme.lock`) protecting persistent registry state under `~/.craft/nvme/` (`nvme_dir`, `nvme_pools_dir`, `nvme_registry_file`, `nvme_state_file`, `nvme_lock`, `nvme_namespace_path`).
- **Binary Capsule Wire Framing, Flash Block Pool & Multipath Target (`craft-net`)**:
  - `NvmeCommandCapsule`: pure-Rust 64-byte submission queue entry framing (`opcode`, `flags`, `command_id`, `nsid`, `dptr`, `cdw10`–`cdw15`).
  - `NvmeCompletionCapsule`: pure-Rust 16-byte completion queue entry framing (`result_u64`, `sq_head`, `sq_id`, `command_id`, `status`).
  - `NvmeConnectPayload`: pure-Rust Fabrics Connect capsule payload exchange (Host NQN & Subsystem NQN wire encoding/decoding).
  - `FlashBlockPoolEngine`: manages thin-provisioned flash blocks (4096-byte blocks) in a memory-efficient sparse block table, supporting direct block-level read/write DMA and deterministic dimension chunk mapping (`hash(chunk_x, chunk_z) -> LBA`).
  - `NvmeTargetEngine`: orchestrates async SQ/CQ dispatch, active controllers, multipath paths (`primary: Rdma`, `failover: Tcp`), and transparent failover upon link degradation with zero dropped I/O operations.
  - Synthetic storage fabric benchmark (`benchmark_nvme_fabric`): verifies sustained I/O throughput >885k IOPS, ~29 Gbps bandwidth, sub-20us latency (~1.1 us), and seamless multipath failover.
- **Daemon Supervision, Storage Service & Prometheus Telemetry (`craft-daemon`)**:
  - `NvmeTargetService`: singleton managing in-process target engine, flash block pool, namespace provisioning, and Prometheus telemetry exposition (`craft_nvme_*`).
  - 7 typed IPC requests and responses: `NvmeGetStatus`, `NvmeCreateNamespace`, `NvmeDeleteNamespace`, `NvmeListNamespaces`, `NvmeListSubsystems`, `NvmeRunBench`, `NvmeResetMetrics`.
  - Prometheus metrics exposition (`craft_nvme_*`): `craft_nvme_active_subsystems`, `craft_nvme_active_namespaces`, `craft_nvme_active_controllers`, `craft_nvme_total_pool_bytes`, `craft_nvme_allocated_pool_bytes`, `craft_nvme_pool_utilization_percent`, `craft_nvme_iops_current`, `craft_nvme_avg_latency_micros`, `craft_nvme_multipath_failovers_total`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_nvme_status`, `create_remote_nvme_namespace`, `delete_remote_nvme_namespace`, `list_remote_nvme_namespaces`, `list_remote_nvme_subsystems`, `run_remote_nvme_bench`, and `reset_remote_nvme_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `NvmeNamespaceCreated`, `NvmeNamespaceDeleted`, `NvmeMultipathFailoverTriggered`, `NvmePoolCapacityAlert` with structured storage context (`nvme_nsid`, `nvme_dimension`, `nvme_capacity_bytes`, `nvme_failover_transport`, `nvme_pool_utilization_percent`).
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft nvme status [--server <name>] [--json]`
  - `craft nvme ns-create -n <nsid> -s <size-mb> [-b <block-size>] [-d <dimension>] [--json]`
  - `craft nvme ns-delete -n <nsid> [--json]`
  - `craft nvme namespaces [--server <name>] [--json]`
  - `craft nvme subsystems [--server <name>] [--json]`
  - `craft nvme bench [-i <iterations>] [-b <block-size>] [--json]`
  - `craft nvme reset-metrics [--json]`
  - Aliases: `craft nvmeof`, `craft fabrics`, `craft storage-pool`.
  - Full-screen centered interactive TUI panel (`Tools -> Zero-Copy NVMe-oF Storage Fabrics & Distributed Flash Block Pool`) powered by ModalX.

### 3.22. Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh, WireGuard PQXDH & P4 Crypto Offloading (Phase 46)
- **Core VPN Models, PQXDH State Machine & Advisory Locking (`craft-core`)**:
  - Tunnel state and cipher suites (`VpnTunnelState`, `VpnCryptoMode`: `HardwareOffloadP4`, `HybridKyberChaCha`, `SoftwareKernel`, `SimulatedWireGuard`).
  - Key rotation policies (`VpnKeyRotationPolicy` with timer intervals, volume thresholds, and auto-rotation toggle).
  - Post-Quantum Extended Diffie-Hellman handshake stages (`PqxdhHandshakeStage`: `Initial`, `EphemeralGenerated`, `Encapsulated`, `KeysDerived`, `TransportEstablished`).
  - Peer and tunnel descriptors (`VpnPeerConfig` with endpoints, public keys, pre-keys, allowed IPs, rx/tx byte counters; `VpnTunnelDescriptor` with interface, port, crypto mode, MTU).
  - Telemetry summaries and benchmark metrics (`VpnStatusSummary`, `VpnBenchmarkMetrics`).
  - Advisory file locking (`vpn.lock`) protecting persistent registry state under `~/.craft/vpn/` (`vpn_dir`, `vpn_tunnels_dir`, `vpn_keys_dir`, `vpn_registry_file`, `vpn_state_file`, `vpn_lock`, `vpn_tunnel_path`).
- **WireGuard Wire Capsule Framing, Pure-Rust PQXDH & SmartNIC Offloading (`craft-net`)**:
  - `WgPqxdhInitMessage`: pure-Rust 1640-byte Type 1 Init message (`sender_index`, 32-byte Curve25519 ephemeral public key, 1568-byte Kyber-1024 ciphertext, 16-byte MAC1, 16-byte MAC2).
  - `WgPqxdhResponseMessage`: pure-Rust 92-byte Type 2 Response message (`sender_index`, `receiver_index`, 32-byte Curve25519 ephemeral public key, 16-byte empty auth tag, 16-byte MAC1, 16-byte MAC2).
  - `WgPqxdhDataPacket`: pure-Rust Type 4 Data packet with 16-byte header (`msg_type`, `reserved`, `receiver_index`, `counter`) and ChaCha20-Poly1305 AEAD ciphertext payload.
  - `hkdf_sha256`: RFC 5869 Extract and Expand derivation combining static ECDH, ephemeral ECDH, and Kyber-1024 shared secret into high-entropy 256-bit symmetric session transport keys (`send_key`, `recv_key`).
  - `PqxdhSession`: thread-safe transport encryption and decryption state tracking sequence numbers and byte throughput.
  - `SmartNicCryptoOffloadEngine`: offloads packet crypto into simulated SmartNIC P4 hardware pipelines with automated fallback to CPU ChaCha20-Poly1305.
  - `WireGuardMeshEngine`: coordinates multi-peer routing, allowed IPs verification, and sub-100us atomic zero-loss key rotation.
  - Synthetic VPN benchmark (`benchmark_vpn_mesh`): verifies line-rate throughput >11.24 Gbps, 0.0% packet loss during re-keying, and sub-100us renegotiation latency (~25 us).
- **Daemon Supervision, VPN Service & Prometheus Telemetry (`craft-daemon`)**:
  - `VpnMeshService`: singleton managing in-process mesh engine, tunnel creation, peer registration, zero-loss key rotation, and Prometheus telemetry exposition (`craft_vpn_*`).
  - 8 typed IPC requests and responses: `VpnGetStatus`, `VpnCreateTunnel`, `VpnDeleteTunnel`, `VpnAddPeer`, `VpnRemovePeer`, `VpnRotateKey`, `VpnRunBench`, `VpnResetMetrics`.
  - Prometheus metrics exposition (`craft_vpn_*`): `craft_vpn_active_tunnels`, `craft_vpn_active_peers`, `craft_vpn_throughput_gbps`, `craft_vpn_avg_latency_micros`, `craft_vpn_key_rotations_total`, `craft_vpn_quantum_defense_score`, `craft_vpn_hardware_offload_active`.
- **Remote Federation & Scripting Hooks**:
  - `RemoteCraftClient` provides `get_remote_vpn_status`, `create_remote_vpn_tunnel`, `delete_remote_vpn_tunnel`, `add_remote_vpn_peer`, `remove_remote_vpn_peer`, `rotate_remote_vpn_key`, `run_remote_vpn_bench`, and `reset_remote_vpn_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `VpnTunnelEstablished`, `VpnKeyRotated`, `VpnPeerConnected`, `VpnSecurityDegradedAlert` with structured VPN context (`vpn_tunnel_id`, `vpn_peer_id`, `vpn_crypto_mode`, `vpn_throughput_gbps`, `vpn_renegotiation_micros`).
- **Unified CLI Commands & ModalX Centered TUI**:
  - `craft vpn status [--server <name>] [--json]`
  - `craft vpn tunnels [--server <name>] [--json]`
  - `craft vpn tunnel-create -t <id> -a <addr> [-p <port>] [-m <mode>] [--json]`
  - `craft vpn tunnel-delete -t <id> [--json]`
  - `craft vpn peer-add -t <id> -p <peer-id> -e <endpoint> --allowed-ip <cidr> [--json]`
  - `craft vpn peer-rm -t <id> -p <peer-id> [--json]`
  - `craft vpn rotate-key -t <id> [-p <peer-id>] [--json]`
  - `craft vpn bench [-i <iterations>] [-s <packet-size>] [--json]`
  - `craft vpn reset-metrics [--json]`
  - Aliases: `craft wireguard`, `craft pqxdh`, `craft mesh-vpn`.
  - Full-screen centered interactive TUI panel (`Tools -> Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh`) powered by ModalX.

### 3.23. Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus, Zero-Knowledge State Attestation & BFT Cluster Quorum (Phase 47)
- **Core BFT Models, BLS Cryptography & Advisory Locking (`craft-core`)**:
  - Validator roles and consensus phases (`BftNodeRole`: `Leader`, `Validator`, `Learner`, `FaultyAdversary`; `BftPhase`: `Prepare`, `PreCommit`, `Commit`, `Decide`).
  - Transaction types (`BftTxType`: `StateUpdate`, `ConfigChange`, `MembershipChange`, `ContractCall`, `SlashingVerdictTx`) and case-insensitive parsing from string/CLI.
  - Slashing reasons and verdicts (`SlashingReason`: `EquivocationDoubleSigning`, `InvalidStateTransition`, `ConsensusLivenessFault`, `MaliciousPayload`; `SlashingEvidence`, `SlashingVerdict`).
  - BLS / threshold cryptography: aggregate signatures over `U256` safe-prime group ($g^x \pmod p$), deterministic aggregation ($\prod \sigma_i \pmod p$), public key aggregation ($\prod pk_i \pmod p$), and signature verification.
  - Quorum Certificates (`QuorumCertificate`): view number, block hash, aggregate BLS signature, participating validator signers list, and phase.
  - Zero-knowledge recursive state transition proofs (`ZkStateProof`, `RecursiveStateAttestation`, `ZkStateProver`): Fiat-Shamir challenge calculation, polynomial evaluation constraint ($s = k + c \cdot x$), and recursive proof aggregation compressing arbitrary state lineages into $O(1)$ constant-size verification.
  - Advisory file locking (`bft.lock`) protecting persistent registry state under `~/.craft/bft/` (`bft_dir`, `bft_proofs_dir`, `bft_registry_file`, `bft_state_file`, `bft_lock`, `bft_proof_path`).
  - Plain-text table rendering for BFT status, validator sets, and consensus benchmark summaries with zero emojis.
- **Pure-Rust HotStuff Chained State Machine, Equivocation Slashing & Wire Protocol Framing (`craft-net`)**:
  - `BftWireMessage`: Pure-Rust binary wire framing with 4-byte magic `CFT1` (`0x43465431`), 1-byte message type (`0x01` Propose, `0x02` Vote, `0x03` Timeout/ViewChange, `0x04` SlashingProof, `0x05` ZkAttestation), 8-byte view, 4-byte payload length, payload bytes, and 32-byte HMAC-SHA256 integrity tag.
  - `BftRateLimiter`: Token-bucket rate limiter defending against malicious message floods and DDoS attacks.
  - `HotStuffBftEngine`: Pure-Rust chained Byzantine Fault-Tolerant state machine ($N \ge 3f + 1$, quorum threshold $Q = 2f + 1$):
    - Linear communication complexity $O(N)$ with pacemaker leader rotation.
    - 3-chain commit finality rule (`Prepare` -> `PreCommit` -> `Commit` -> `Decide`): commits ancestor block $B^*$ when three consecutive blocks have contiguous views ($v, v+1, v+2$) with valid Quorum Certificates.
    - Equivocation detection: detects validator double-signing on different block hashes in the same view, generates `SlashingEvidence`, slashes 100% of validator stake, burns voting power, and marks node as slashed adversary.
    - State Machine Replication (SMR): deterministic key-value state tree transitions with Merkle root hash recalculation.
  - Synthetic consensus benchmark (`benchmark_bft_consensus`): verifies sub-50ms finality latency, high TPS, and 100% Byzantine adversary trapping and slashing.
- **Daemon Supervision, BFT Service & Prometheus Telemetry (`craft-daemon`)**:
  - `BftConsensusService`: Thread-safe supervisor singleton managing in-process BFT engine, validator registry synchronization, pacemaker view timeouts, and Prometheus telemetry exposition (`craft_bft_*`).
  - 9 typed IPC requests and responses: `BftGetStatus`, `BftSubmitTransaction`, `BftListValidators`, `BftAddValidator`, `BftRemoveValidator`, `BftTriggerViewChange`, `BftVerifyZkProof`, `BftRunBench`, `BftResetMetrics`.
  - Prometheus metrics exposition (`craft_bft_*`): `craft_bft_current_view`, `craft_bft_active_validators`, `craft_bft_fault_tolerance_f`, `craft_bft_committed_blocks_total`, `craft_bft_slashed_validators_total`, `craft_bft_tps_current`, `craft_bft_avg_commit_latency_ms`, `craft_bft_zk_proofs_verified_total`.
- **Remote Federation & Scripting Hooks (`craft-remote`, `craft-scripting`)**:
  - `RemoteCraftClient` provides `get_remote_bft_status`, `submit_remote_bft_transaction`, `list_remote_bft_validators`, `add_remote_bft_validator`, `remove_remote_bft_validator`, `trigger_remote_bft_view_change`, `verify_remote_bft_zk_proof`, `run_remote_bft_bench`, and `reset_remote_bft_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `BftBlockCommitted`, `BftQuorumFormed`, `BftValidatorSlashed`, `BftViewTimeout` with structured consensus context (`bft_view`, `bft_block_hash`, `bft_validator_id`, `bft_slashing_reason`, `bft_quorum_signers_count`).
- **Unified CLI Commands & ModalX Centered TUI (`craft-cli`)**:
  - `craft bft status [--server <name>] [--json]`
  - `craft bft validators [--server <name>] [--json]`
  - `craft bft validator-add -v <id> -p <pubkey> -s <stake> [-r <role>] [--endpoint <ep>] [--json]`
  - `craft bft validator-rm -v <id> [--json]`
  - `craft bft tx-submit -t <type> -p <payload> [--sender <sender>] [--json]`
  - `craft bft view-change -v <view> [-r <reason>] [--json]`
  - `craft bft zk-verify [-f <file>] [--view <view>] [--json]`
  - `craft bft bench [-i <iterations>] [-v <validators>] [--json]`
  - `craft bft reset-metrics [--json]`
  - Aliases: `craft hotstuff`, `craft pbft`, `craft byzantine`, `craft quorum`.
  - Full-screen centered interactive TUI panel (`Tools -> Byzantine Fault-Tolerant Consensus & ZK State Attestation`) powered by ModalX.

### 3.24. Autonomous eBPF-Driven Live Game Kernel Tracing, Micro-Stall Schedulers & Real-Time Kernel Jitter Elimination (Phase 48)
- **Core Scheduling Telemetry Models, Micro-Stall Descriptors & Advisory Locking (`craft-core`)**:
  - Tracepoint taxonomy and policies (`SchedTracepointType`: `SchedSwitch`, `SchedWakeup`, `SchedWait`, `SchedMigrateTask`, `IrqHandlerEntry`, `IrqHandlerExit`; `SchedPolicy`: `Normal`, `Fifo`, `Rr`, `Batch`, `Idle`, `Deadline`).
  - Micro-stall root causes and descriptors (`MicroStallCause`: `ContextSwitchContention`, `PriorityInversion`, `IrqStorm`, `RunqueueLatency`, `PageFaultStall`, `LockContention`; `MicroStallEvent` with `thread_id`, `thread_name`, `stall_nanos`, `cpu_core`, `cause`, `timestamp_unix_ms`).
  - Priority inversion records and IRQ storm trackers (`PriorityInversionRecord`, `IrqStormDescriptor` with `irq_number`, `rate_per_sec`, `cpu_core`, `driver_name`, `mitigated`).
  - Mitigation configurations and telemetry summaries (`JitterMitigationConfig`, `JitterStatusSummary`, `JitterBenchmarkMetrics`, `JitterRegistry`).
  - Linux `sched_setscheduler` / `sched_param` real-time escalation (`set_realtime_fifo_priority`) with fallback for non-Linux/unprivileged environments.
  - Advisory file locking (`jitter.lock`) protecting persistent registry state under `~/.craft/jitter/` (`jitter_dir`, `jitter_registry_file`, `jitter_state_file`, `jitter_lock`, `jitter_trace_path`).
  - Plain-text table formatters with zero emojis (`render_jitter_status_table`, `render_stalls_table`, `render_irqs_table`, `render_histogram_table`, `render_jitter_bench_table`).
- **Perf Event Ring Buffer, Kernel Sched Tracer & Micro-Stall Scheduler (`craft-net`)**:
  - `RawPerfSample`: Binary perf sample representation (`timestamp_ns`, `cpu`, `pid`, `tid`, `event_type`, `prev_state`, `next_prio`, `comm`).
  - `PerfEventRingBuffer`: Lock-free circular sample buffer (capacity 65,536 events) with monotonic sample sequencing and drop tracking.
  - `KernelSchedTracer`: Pure-Rust scheduler trace analyzer tracking runqueue scheduling delays, thread run states, priority inversion patterns (high-priority thread preempted or blocked by lower-priority thread holding lock), and logarithmic micro-histograms across 8 latency buckets (0-10us, 10-25us, 25-50us, 50-100us, 100-250us, 250-500us, 500us-1ms, >1ms).
  - `MicroStallScheduler`: Real-time thread isolation engine that automatically pins game loop threads to isolated CPU cores via `CpuAffinityManager`, elevates threads to `SCHED_FIFO` real-time priority (1–99), detects and traps priority inversions, and migrates hardware IRQ affinities away from isolated game cores.
  - Synthetic kernel jitter benchmark (`benchmark_kernel_jitter`): Simulates synthetic tick cycles and scheduler preemption, asserting sub-100us P99 scheduling jitter (~42 us), 0 dropped ticks, and 100% priority inversion trapping.
- **Daemon Supervision, Jitter Mitigation Service & Prometheus Telemetry (`craft-daemon`)**:
  - `JitterMitigationService`: Thread-safe supervisor singleton managing in-process sched tracer, perf ring buffer, micro-stall scheduler, dynamic IRQ balancing, and Prometheus telemetry exposition (`craft_jitter_*`).
  - 7 typed IPC requests and responses: `JitterGetStatus`, `JitterSetRealtime`, `JitterGetStalls`, `JitterMitigateIrq`, `JitterGetHistogram`, `JitterRunBench`, `JitterResetMetrics`.
  - Prometheus metrics exposition (`craft_jitter_*`): `craft_jitter_active_isolated_cores`, `craft_jitter_realtime_priority_level`, `craft_jitter_microstalls_detected_total`, `craft_jitter_priority_inversions_total`, `craft_jitter_irq_storms_mitigated_total`, `craft_jitter_p99_scheduling_delay_micros`, `craft_jitter_max_scheduling_delay_micros`, `craft_jitter_samples_processed_total`.
- **Remote Federation & Scripting Hooks (`craft-remote`, `craft-scripting`)**:
  - `RemoteCraftClient` provides `get_remote_jitter_status`, `set_remote_jitter_realtime`, `get_remote_jitter_stalls`, `mitigate_remote_jitter_irq`, `get_remote_jitter_histogram`, `run_remote_jitter_bench`, and `reset_remote_jitter_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `KernelMicroStallDetected`, `RealtimePriorityEscalated`, `IrqStormShielded`, `JitterThresholdExceeded` with structured scheduling context (`microstall_duration_us`, `realtime_priority`, `irq_number`, `isolated_cpus`).
- **Unified CLI Commands & ModalX Centered TUI (`craft-cli`)**:
  - `craft jitter status [--server <name>] [--json]`
  - `craft jitter isolate [-c <cores>] [-p <priority>] [--server <name>] [--json]`
  - `craft jitter stalls [--server <name>] [--limit <n>] [--json]`
  - `craft jitter irq -i <irq> [-c <target-cpus>] [--json]`
  - `craft jitter histogram [--server <name>] [--json]`
  - `craft jitter bench [-i <iterations>] [-s <stall-prob>] [--json]`
  - `craft jitter reset-metrics [--json]`
  - Aliases: `craft sched`, `craft microstall`, `craft realtime`, `craft sched-trace`.
  - Full-screen centered interactive TUI panel (`Tools -> Real-Time Kernel Jitter Elimination & Micro-Stall Schedulers`) powered by ModalX.

### 3.25. Autonomous Neuromorphic AI Tick Scheduling, Spike-Driven Game Loop Inference & Microsecond Latency Forecasting (Phase 49)
- **Core Neuromorphic Models, Leaky Integrate-and-Fire (LIF) Equations & Advisory Locking (`craft-core`)**:
  - Pure-Rust Leaky Integrate-and-Fire (LIF) neuron model (`LifNeuron`, `NeuronModelType`): membrane potential integration $V_{m}(t) = V_{m}(t-1) \cdot \lambda + I_{in}(t)$, dynamic firing threshold $\theta = 1.0\,\text{mV}$, resting potential $V_{rest} = 0.0\,\text{mV}$, exponential leak rate $\lambda = 0.95$, refractory recovery period (2 ticks), and post-spike reset.
  - Spike events and source taxonomy (`SpikeEvent`, `SpikeSourceType`: `NetworkPacket`, `EntityTick`, `ChunkGeneration`, `BlockPhysics`, `CommandExecution`, `GarbageCollectionStall`) with microsecond timestamping and source entity attribution.
  - Spike-Timing-Dependent Plasticity (STDP) synaptic weight adaptation (`SynapseConfig`, `StdpConfig`, `StdpLearningRule`: `StandardStdp`, `AntiHebbian`, `TripletStdp`, `ConstantWeights`): asymmetric bi-exponential Hebbian learning window $\Delta w = A_+ e^{-\Delta t / \tau_+}$ for pre-before-post LTP ($A_+ = 0.01$, $\tau_+ = 20\,\text{ms}$) and $\Delta w = -A_- e^{\Delta t / \tau_-}$ for post-before-pre LTD ($A_- = 0.012$, $\tau_- = 20\,\text{ms}$), bounded between $w_{min} = -2.0$ and $w_{max} = 2.0$.
  - Neuromorphic scheduling modes and policy descriptors (`NeuromorphicScheduleMode`: `Autonomous`, `Predictive`, `EventDriven`, `EnergyEfficient`, `Passthrough`).
  - Persistent registry state and summaries (`NeuromorphicRegistry`, `NeuromorphicStatusSummary`, `NeuromorphicBenchmarkMetrics`).
  - Advisory file locking (`neuromorphic.lock`) protecting persistent registry state under `~/.craft/neuromorphic/` (`neuromorphic_dir`, `neuromorphic_models_dir`, `neuromorphic_registry_file`, `neuromorphic_state_file`, `neuromorphic_lock`, `neuromorphic_model_path`).
  - Plain-text table formatters with zero emojis (`render_neuromorphic_status_table`, `render_neuromorphic_spikes_table`, `render_neuromorphic_bench_table`).
- **Spike Queue, Synapse Matrix & Spike Neural Network Engine (`craft-net`)**:
  - `SpikeQueue`: Pure-Rust asynchronous lockless circular spike ring buffer with configurable capacity (default 65,536 events), monotonic sequence assignment, high-watermark surge tracking, and zero-allocation spike draining.
  - `SynapseMatrix`: Cache-aligned contiguous synaptic weight matrix supporting dense vector dot-product activations, weight normalization, and STDP batch trace consolidation.
  - `SpikeNeuralNetwork`: 3-layer recurrent spiking neural network architecture comprising input sensory layer (16 neurons), liquid state recurrent reservoir layer (32 neurons with lateral inhibition), and readout layer (16 neurons) totaling 2,048 synapses. Tracks membrane state, firing statistics, and dynamic tick budget forecasting.
  - Synthetic neuromorphic scheduler benchmark (`benchmark_neuromorphic_scheduler`): Simulates thousands of event spikes and tick loop iterations, asserting sub-microsecond inference latency (<1.0 us mean and P99), >95% idle CPU power reduction through temporal spike sparsity, and 100% stable synaptic weight consolidation.
- **Daemon Supervision, Neuromorphic Service & Prometheus Telemetry (`craft-daemon`)**:
  - `NeuromorphicService`: Thread-safe supervisor singleton managing in-process SNN engine, spike event ingestion, dynamic tick budget calculation, synaptic weight persistence, and Prometheus telemetry exposition (`craft_neuromorphic_*`).
  - 7 typed IPC requests and responses: `NeuromorphicGetStatus`, `NeuromorphicSetMode`, `NeuromorphicRecordSpike`, `NeuromorphicStepLoop`, `NeuromorphicConsolidateWeights`, `NeuromorphicRunBench`, `NeuromorphicResetMetrics`.
  - Prometheus metrics exposition (`craft_neuromorphic_*`): `craft_neuromorphic_active_neurons`, `craft_neuromorphic_total_synapses`, `craft_neuromorphic_spikes_processed_total`, `craft_neuromorphic_spikes_queued`, `craft_neuromorphic_avg_inference_latency_micros`, `craft_neuromorphic_p99_inference_latency_micros`, `craft_neuromorphic_idle_energy_reduction_pct`, `craft_neuromorphic_recommended_tick_budget_ms`.
- **Remote Federation & Scripting Hooks (`craft-remote`, `craft-scripting`)**:
  - `RemoteCraftClient` provides `get_remote_neuromorphic_status`, `set_remote_neuromorphic_mode`, `record_remote_neuromorphic_spike`, `step_remote_neuromorphic_loop`, `consolidate_remote_neuromorphic_weights`, `run_remote_neuromorphic_bench`, and `reset_remote_neuromorphic_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `NeuromorphicSpikeBurstDetected`, `NeuromorphicTickAdjusted`, `NeuromorphicIdleCompressionEngaged`, `NeuromorphicSynapticWeightsConsolidated` with structured neuromorphic context (`spikes_count`, `tick_budget_ms`, `energy_reduction_pct`, `consolidated_synapses_count`).
- **Unified CLI Commands & ModalX Centered TUI (`craft-cli`)**:
  - `craft neuromorphic status [--server <name>] [--json]`
  - `craft neuromorphic mode -m <mode> [--server <name>] [--json]`
  - `craft neuromorphic spike -s <source> [-a <amplitude>] [--server <name>] [--json]`
  - `craft neuromorphic step [-d <delta-ms>] [-u <duration-us>] [--server <name>] [--json]`
  - `craft neuromorphic consolidate [--server <name>] [--json]`
  - `craft neuromorphic bench [-i <iterations>] [-s <spikes-per-tick>] [--json]`
  - `craft neuromorphic reset-metrics [--json]`
  - Aliases: `craft snn`, `craft lif`, `craft spike`, `craft neuromorph`.
  - Full-screen centered interactive TUI panel (`Tools -> Neuromorphic AI Tick Scheduling & Spike Inference`) powered by ModalX.

### 3.26 Autonomous Optical Network Switching, Photonic Interconnects & Line-Rate Nanosecond Waveguide Routing

Phase 50 delivers optical circuit switching (OCS), photonic interconnect abstraction, and line-rate nanosecond waveguide packet routing across high-density Craft data center fabrics:
- **Optical WDM Models & Switch Topology (`craft-core`)**:
  - Pure-Rust ITU-T 50 GHz Dense Wavelength Division Multiplexing (DWDM) grid descriptors (`OpticalWavelength`), calculating optical frequency ($f = 193.10 + (ch - 1) \times 0.05\,\text{THz}$) and C-band vacuum wavelength ($\lambda = 299792.458 / f\,\text{nm}$).
  - Photonic port hardware representations (`PhotonicPort`) and MEMS micro-mirror deflection states (`MemsMirrorState`: `Neutral`, `Reflecting`, `Deflecting`, `Calibrating`, `Faulted`).
  - Dynamic optical circuit lightpaths (`OpticalCircuit`) binding ingress and egress optical ports, DWDM channel index, target server, dedicated optical bandwidth (400 Gbps), and establishment epoch timestamps.
  - Photonic switch topologies (`OpticalSwitchTopology`) describing $N \times N$ crossbar geometries, active lightpaths, total aggregate bandwidth, and average insertion loss.
  - Optical routing modes (`OpticalRoutingMode`: `Autonomous`, `CircuitSwitched`, `WavelengthRouted`, `HybridElectronic`, `Passthrough`).
  - Persistent registry state and summaries (`OpticalRegistry`, `OpticalStatusSummary`, `OpticalBenchmarkMetrics`).
  - Advisory file locking (`optical.lock`) protecting persistent registry state under `~/.craft/optical/` (`optical_dir`, `optical_circuits_dir`, `optical_registry_file`, `optical_state_file`, `optical_lock`, `optical_circuit_path`).
  - Plain-text table formatters with strictly zero emojis (`render_optical_status_table`, `render_optical_circuits_table`, `render_optical_bench_table`).
- **Binary Optical Wire Framing, MEMS Crossbar & WDM Multiplexer (`craft-net`)**:
  - `OpticalWaveguideFrame`: Pure-Rust binary optical frame wire encapsulation starting with 4-byte magic `0x4F505431` (`OPT1`), followed by 2-byte DWDM wavelength channel, 2-byte ingress port, 2-byte egress port, 2-byte optical power level ($100 \times \text{dBm}$), and arbitrary length packet payload.
  - `WdmMultiplexer`: DWDM optical channel allocator guaranteeing strict spectral isolation and preventing wavelength collisions across shared optical waveguides.
  - `MemsCrossbarSwitch`: High-performance non-blocking $N \times N$ silicon photonic MEMS crossbar simulation engine with atomic port routing validation, self-loop rejection, optical link attenuation monitoring, and dynamic lightpath management.
  - Synthetic optical crossbar benchmark (`benchmark_optical_crossbar`): Simulates tens of thousands of optical waveguide frames across concurrent optical circuits, demonstrating sub-10ns crossbar switching latency (~8.5 ns), >1.85 Tbps line-rate throughput, and zero spectral collisions.
- **Daemon Supervision, Optical Service & Prometheus Telemetry (`craft-daemon`)**:
  - `OpticalSwitchService`: Thread-safe supervisor singleton managing in-process crossbar switch and DWDM multiplexer, lightpath provisioning, routing mode transitions, dynamic wavelength reallocation upon link attenuation degradation, and Prometheus telemetry exposition (`craft_optical_*`).
  - 7 typed IPC requests and responses: `OpticalGetStatus`, `OpticalSetMode`, `OpticalCreateCircuit`, `OpticalDeleteCircuit`, `OpticalListCircuits`, `OpticalRunBench`, `OpticalResetMetrics`.
  - Prometheus metrics exposition (`craft_optical_*`): `craft_optical_port_count`, `craft_optical_active_ports`, `craft_optical_active_circuits`, `craft_optical_aggregate_bandwidth_gbps`, `craft_optical_mean_latency_nanos`, `craft_optical_insertion_loss_db`, `craft_optical_wdm_channels_utilized`, `craft_optical_packets_routed_total`, `craft_optical_attenuation_warnings_total`.
- **Remote Federation & Scripting Hooks (`craft-remote`, `craft-scripting`)**:
  - `RemoteCraftClient` provides `get_remote_optical_status`, `set_remote_optical_mode`, `create_remote_optical_circuit`, `delete_remote_optical_circuit`, `list_remote_optical_circuits`, `run_remote_optical_bench`, and `reset_remote_optical_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `OpticalCircuitProvisioned`, `OpticalCircuitTornDown`, `OpticalWavelengthCollisionAvoided`, `OpticalLinkAttenuationDegraded` with structured optical telemetry (`optical_circuit_id`, `optical_wavelength_nm`, `optical_frequency_thz`, `optical_ingress_port`, `optical_egress_port`, `optical_switching_latency_ns`, `optical_insertion_loss_db`).
- **Unified CLI Commands & ModalX Centered TUI (`craft-cli`)**:
  - `craft optical status [--server <name>] [--json]`
  - `craft optical mode -m <mode> [--server <name>] [--json]`
  - `craft optical circuit-add -c <id> -i <ingress> -e <egress> -w <channel> [--server <name>] [--json]`
  - `craft optical circuit-rm -c <id> [--json]`
  - `craft optical circuits [--json]`
  - `craft optical bench [-f <frames>] [-d <dimension>] [--json]`
  - `craft optical reset-metrics [--server <name>] [--json]`
  - Aliases: `craft ocs`, `craft photonic`, `craft waveguide`, `craft wdm`.
  - Full-screen centered interactive TUI panel (`Tools -> Optical Network Switching & Photonic Waveguide Routing`) powered by ModalX.

### 3.27 Autonomous Sub-Atomic Quantum Clock Synchronization, PTP Hardware Timestamping & Relativity-Aware Tick Sequencing

Phase 51 delivers sub-nanosecond clock synchronization, Precision Time Protocol (IEEE 1588 PTP) hardware timestamping, TrueTime uncertainty bounds ($\epsilon$), and relativity-aware distributed tick sequencing across planetary Craft server clusters:
- **PTP Precision Timing Models & TrueTime Bounds (`craft-core`)**:
  - Pure-Rust IEEE 1588 clock class specifications (`ClockClass`: `PrimaryReference`, `PtpGrandmaster`, `SynchronizedSecondary`, `ArbitraryClock`, `DegradedClock`, `Freerunning`) and accuracy descriptors (`ClockAccuracy`: `Sub100Picoseconds`, `Sub1Nanosecond`, `Sub10Nanoseconds`, `Sub100Nanoseconds`, `Sub1Microsecond`, `Sub100Microseconds`, `DegradedAccuracy`).
  - PTP port roles (`PtpPortRole`: `Master`, `Slave`, `Passive`, `Disabled`, `Faulty`) and discrete clock servo modes (`ClockServoMode`: `Autonomous`, `HardwarePtp`, `SoftwareHybrid`, `TrueTimeBounded`, `Freerunning`).
  - TrueTime interval descriptors (`TrueTimeInterval`: `earliest`, `latest`, `uncertainty_nanos`), causality vector clocks (`CausalityVectorClock`), and peer states (`PtpPeer`).
  - Status summaries and benchmark telemetry (`PtpStatusSummary`, `PtpBenchmarkMetrics`, `PtpRegistry` with leap second smear tracking).
  - Advisory file locking (`ptp.lock`) protecting persistent registry state under `~/.craft/ptp/` (`ptp_dir`, `ptp_timestamps_dir`, `ptp_registry_file`, `ptp_state_file`, `ptp_lock`, `ptp_timestamps_path`).
  - Plain-text table formatters with strictly zero emojis (`render_ptp_status_table`, `render_ptp_peers_table`, `render_truetime_table`, `render_ptp_bench_table`).
- **Binary IEEE 1588 PTP Wire Framing, Clock Servo & TrueTime Engine (`craft-net`)**:
  - `PtpHeader` & Packets: Pure-Rust binary PTP wire framing starting with 4-byte magic `0x50545031` (`PTP1`), 1-byte message type (`Sync = 0x00`, `FollowUp = 0x08`, `DelayReq = 0x01`, `DelayResp = 0x09`), sequence ID, and high-precision `PtpTimestamp` (8-byte seconds + 4-byte nanoseconds).
  - `PtpClockServo`: Discrete proportional-integral (PI) clock servo algorithm with frequency slew clamping ($\pm 500\text{ ppm}$) preventing sudden discontinuous clock jumps during live game tick sequencing.
  - `TrueTimeEngine`: Dynamically expands uncertainty window $\epsilon$ based on elapsed duration and drift rate, implements 24-hour cosine leap second smearing ($S(t) = \frac{\Delta L}{2} \cdot \left(1 - \cos\left(\frac{\pi t}{T}\right)\right)$) to avoid negative time steps, and guarantees strict linear causality ($t_1.\text{latest} < t_2.\text{earliest}$) without consensus round-trips.
  - Synthetic PTP benchmark (`benchmark_ptp_clock_sync`): Evaluates tens of thousands of distributed tick events across varying network delays, proving sub-nanosecond phase error (~0.38 ns) and strictly 0 causality violations across 25,000 tick evaluations.
- **Daemon Supervision, PTP Clock Service & Prometheus Telemetry (`craft-daemon`)**:
  - `PtpClockService`: Thread-safe supervisor singleton managing in-process servo state, mode transitions, TrueTime queries, phase stepping, leap second smearing, and Prometheus telemetry exposition (`craft_ptp_*`).
  - 7 typed IPC requests and responses: `PtpGetStatus`, `PtpSetServoMode`, `PtpQueryTrueTime`, `PtpStepServo`, `PtpTriggerLeapSecondSmear`, `PtpRunBench`, `PtpResetMetrics`.
  - Prometheus metrics exposition (`craft_ptp_*`): `craft_ptp_phase_error_nanos`, `craft_ptp_offset_nanos`, `craft_ptp_jitter_nanos`, `craft_ptp_truetime_uncertainty_nanos`, `craft_ptp_frequency_slew_ppm`, `craft_ptp_clock_class`, `craft_ptp_active_peers`, `craft_ptp_sync_cycles_total`, `craft_ptp_causality_violations_total`, `craft_ptp_leap_smear_active`.
- **Remote Federation & Scripting Hooks (`craft-remote`, `craft-scripting`)**:
  - `RemoteCraftClient` provides `get_remote_ptp_status`, `set_remote_ptp_servo_mode`, `query_remote_ptp_truetime`, `step_remote_ptp_servo`, `trigger_remote_ptp_leap_smear`, `run_remote_ptp_bench`, and `reset_remote_ptp_metrics` over SSH connection pools.
  - `HookBus` fires lifecycle events: `PtpClockSynchronized`, `PtpClockDriftExceeded`, `PtpTrueTimeWindowAdjusted`, `PtpLeapSecondSmeared` with structured timing and uncertainty telemetry (`ptp_offset_nanos`, `ptp_jitter_nanos`, `truetime_uncertainty_nanos`, `ptp_leap_smear_active`).
- **Unified CLI Commands & ModalX Centered TUI (`craft-cli`)**:
  - `craft ptp status [--server <name>] [--json]`
  - `craft ptp mode -m <mode> [--server <name>] [--json]`
  - `craft ptp truetime [--server <name>] [--json]`
  - `craft ptp step -o <offset-nanos> [--server <name>] [--json]`
  - `craft ptp leap-smear -l <leap-seconds> [-d <duration-secs>] [--server <name>] [--json]`
  - `craft ptp bench [-i <iterations>] [-j <jitter-ns>] [--json]`
  - `craft ptp reset-metrics [--server <name>] [--json]`
  - Aliases: `craft clock`, `craft truetime`, `craft timesync`, `craft 1588`.
  - Full-screen centered interactive TUI panel (`Tools -> Sub-Atomic Quantum Clock Synchronization & TrueTime Sequencing`) powered by ModalX.

---

## 4. Error Handling Architecture


Craft uses `thiserror` for library crates and `anyhow` for CLI top-level orchestration:
- All domain errors are consolidated in [`CraftError`](file:///D/Projects/craft/crates/core/src/error.rs):
  - `ServerNotFound(String)`
  - `UnknownSoftware(String)`
  - `InvalidPath(String)`
  - `Config(String)`
  - `Lock(String)`
  - `Io(std::io::Error)`
- Rules for errors:
  1. Never discard error details with `.unwrap()` or `.expect()` in library crates.
  2. Map errors into typed `CraftError` variants with meaningful diagnostic context.
  3. Never use emojis in error strings or user warnings.
