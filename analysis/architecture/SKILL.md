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
│   └── meta/              # Zstd-compressed JSON metadata
├── backups/               # Local snapshot storage (<server-name>/<archive>.tar.zst)
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
  - `<server_dir>/server.lock` ensures a server instance cannot be launched simultaneously by multiple processes.

### 3.3. `RbacRegistry` & `AuditLedger`
- **`RbacRegistry` (`~/.craft/rbac.toml`)**: Manages `UserAccount` entries with 1,000-round SHA-256 salted hashes, `Role` hierarchies, granular `Permission` sets, and per-user `assigned_servers` filtering. Auto-initializes default `admin:admin` account if empty.
- **`AuditLedger` (`~/.craft/audit.log`)**: Records every CLI, REST, and WebSocket mutation with continuous SHA-256 hash chains starting at `GENESIS_HASH` and HMAC-SHA256 signatures. Supports verification against tampering with `AuditLedger::verify_chain`.

### 3.4. `ClustersRegistry` & Topological DAG Scheduling
- **Location**: `~/.craft/clusters.toml`
- **Format**: TOML array of `ServerCluster` structs (`name`, `nodes`, `proxy_entry`) with node definitions (`name`, `role`, `remote`, `depends_on`).
- **Roles**: `backend`, `proxy`, `lobby`.
- **DAG Topological Ordering**:
  - `ServerCluster::resolve_startup_order()` constructs an in-degree dependency graph from explicit `depends_on` lists.
  - Proxy nodes implicitly depend on all backend/lobby nodes if unconfigured, guaranteeing routing proxies start last once upstream servers are healthy.
  - Detects cyclic dependencies and returns descriptive errors.
  - `ServerCluster::resolve_shutdown_order()` reverses the sequence, ensuring proxies disconnect players before backend worlds terminate.

### 3.5. Cross-Node Federated Migration (`ServerMigrator`)
- **Protocol**:
  1. Validates source server is stopped (`server.lock` and PID verification).
  2. Verifies remote host SSH connection and `craft` binary installation.
  3. Compresses source server into an atomic `.tar.zst` snapshot (excluding locks, PIDs, and sockets).
  4. Computes SHA-256 digest locally and streams upload via SFTP to `~/.craft/staging/`.
  5. Computes remote checksum (`sha256sum`, `shasum -a 256`, or OpenSSL) and validates integrity before unpacking.
  6. Extracts directly into `~/.craft/servers/<target_name>` on the remote node.
  7. Re-registers the server in remote `~/.craft/servers.toml`, cleans up staging, and optionally trashes local source (`TrashManager::trash_path`).

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
