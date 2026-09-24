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
