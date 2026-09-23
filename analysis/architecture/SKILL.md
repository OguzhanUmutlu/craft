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
