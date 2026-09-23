# Security, Diagnostics & Data Safety Skill Guide

> **Domain**: Non-Destructive Deletion, Process Locking, Integrity Verification & Diagnostics  
> **Primary Location**: `crates/core/src/trash.rs`, `crates/core/src/process.rs`, `crates/cli/src/commands/fix.rs`

---

## 1. Non-Destructive File Operations (`TrashManager`)

Craft enforces a strict "safety-first" file management invariant: **never perform unrecoverable deletions without explicit confirmation or user opt-in (`--permanent`)**.

```
[ Active File / World Directory ]
              │
              │  craft world rm / craft rm
              ▼
[ TrashManager Staging (~/.craft/trash/) ]
  ├── Computes recursive size (dir_size_bytes)
  ├── Computes deterministic SHA-256 hash (compute_dir_hash / compute_hash)
  ├── Moves into ~/.craft/trash/<id>_<name>/
  └── Updates ~/.craft/trash/manifest.toml under exclusive lock
```

### Invariants:
1. **Directory & File Parity**: `trash_path` supports both single files and recursive directory hierarchies with equal fidelity.
2. **Restoration Integrity Verification**: Files restored from trash verify that their SHA-256 hash matches the manifest before placing them back in their original path.
3. **Safe World Deletion**: `craft world rm` defaults to moving the target world to the trash bin, returning the trash ID and recovery instructions (`craft trash restore <id>`).

---

## 2. Process Locking & Dual-Execution Defense

Multiple processes launching the same game server directory leads to corrupted level indexes, overwritten player data, and split region chunks.
- **Mechanism**: Every server execution acquires an exclusive OS lock on `<server_path>/server.lock` via `fs2::FileExt::lock_exclusive()`.
- **Active PID Verification**: PID files (`server.pid`) are written upon startup. Before any lifecycle action (stop, restart, backup restore), `get_server_running_pid` inspects whether the PID is actually alive.
- **Stale Lock Cleanup**: If a previous process was forcefully killed (`kill -9`) or the system crashed, `craft fix` checks if the holding PID is dead; if orphan, it cleans up the stale lock file safely.

---

## 3. Comprehensive Diagnostic Self-Healing (`craft fix`)

The `craft fix` command executes automated diagnostic checks:
1. **JAR Healing**: Scans for versioned or variant JAR filenames if `server.jar` is missing.
2. **Java Runtime Matching**: Inspects bytecode version and updates start scripts if Java is missing or mismatched.
3. **EULA Enforcement**: Automatically accepts or repairs `eula=true` in `eula.txt` for Minecraft Java servers.
4. **Native Permissions**: Ensures `0o755` executable permissions are set on `start.sh`, `bedrock_server`, PHP binaries, Factorio, Terraria, Valheim, and Palworld binaries.
5. **Port Collision Detection**: Checks `server.properties` ports against other registered servers and detects if the port is already bound by another host process.
6. **Ghost Registry Cleanup**: Flags registered server paths that no longer exist on disk.

---

## 4. Multi-Tenant Role-Based Access Control (`RbacRegistry`)

Craft implements granular multi-tenant access control backed by `~/.craft/rbac.toml`:
- **Role Hierarchies**:
  - `SuperAdmin`: Unrestricted privileges across all servers, user management, audit trails, and configuration updates.
  - `ServerOperator`: Server lifecycle operations (`start`, `stop`, `restart`, `console`, `files`, `backups`) restricted to explicitly assigned server instances.
  - `BackupAuditor`: Read-only access to backups and audit logs, with permissions to trigger manual snapshots and inspect archives.
  - `Viewer`: Read-only observation of server status, read-only live console log feeds, and server directory listings.
- **Granular Permissions**:
  `ServerStart`, `ServerStop`, `ServerRestart`, `ServerDelete`, `ServerFixForce`, `ServerConsoleView`, `ServerConsoleInput`, `BackupCreate`, `BackupRestore`, `BackupDelete`, `FileBrowse`, `FileEdit`, `FileDelete`, `AuditLogView`, `UserManage`.
- **Per-User Server Scoping**:
  `assigned_servers: Option<Vec<String>>` permits fine-grained scoping. If `None` or containing `*`, access is global across all managed instances.
- **Key-Stretching Password Security**:
  Passphrases are salted with 16 random bytes and stretched over 1,000 iterative rounds of SHA-256 hashing (`hash_password`), mitigating brute-force and dictionary attacks.
- **Transactional File Concurrency**:
  Mutations to `~/.craft/rbac.toml` acquire exclusive advisory locks (`~/.craft/locks/rbac.lock`) via `fs2` and perform atomic write-and-rename cycles.

---

## 5. Append-Only Cryptographic Audit Ledger (`AuditLedger`)

Every administrative invocation across the CLI, daemon REST API, and WebSocket console is immutably appended to `~/.craft/audit.log`:
- **Continuous Hash Chain**:
  Each entry embeds `previous_hash`, `entry_hash`, and a pure-Rust `signature` calculated via HMAC-SHA256 over the current entry hash using the daemon master secret:
  ```
  Genesis [0000...0000]
         │
         ▼
  Entry 1 [ Hash_1 = SHA256(prev_hash | ts | actor | action | status | ...), Sig_1 = HMAC(Hash_1) ]
         │
         ▼
  Entry 2 [ Hash_2 = SHA256(Hash_1 | ts | actor | action | status | ...),    Sig_2 = HMAC(Hash_2) ]
  ```
- **Tamper Detection (`verify_chain`)**:
  `AuditLedger::verify_chain` parses the log line-by-line, recalculates SHA-256 digests and validates HMAC signatures. Any line deletion, insertion, or in-place edit immediately breaks the hash chain, pinpointing the exact corrupted index.
- **Lock-Guarded Appends**:
  Log writing acquires an exclusive file lock (`~/.craft/locks/audit.lock`) before reading the last recorded hash and appending new serializations.

---

## 6. Linux Cgroups v2 Resource Isolation, Dynamic Throttling & Fair-Share Scheduling

Craft provides kernel-native Linux cgroups v2 resource isolation, hard CPU/memory quotas, and fair-share scheduling across multi-tenant deployments without Docker runtime overhead.

```
[ Active Process (PID) ]
             │
             ▼
[ /sys/fs/cgroup/craft/<server_name>/ ]
  ├── memory.max (hard OOM kill boundary)
  ├── memory.high (soft throttle threshold & reclaim)
  ├── cpu.max (quota and period: "$QUOTA $PERIOD")
  ├── cpu.weight (CFS fair-share scheduling weight 1..=10000)
  ├── io.weight (BFQ / blk-iocost I/O scheduling weight 1..=10000)
  ├── pids.max (fork bomb mitigation)
  └── cgroup.procs (active task assignment)
```

### Invariants & Architectural Capabilities:
1. **Resilient Fallback Mode (`CgroupV2Driver`)**:
   - Tests availability of `/sys/fs/cgroup/cgroup.controllers`. If permitted, creates `/sys/fs/cgroup/craft/<server>`.
   - On unprivileged containers, macOS, Windows, or testing sandboxes, falls back transparently to userspace mock hierarchy under `~/.craft/cgroups/mock_sys_fs/`, ensuring 100% of all quota validations, budget calculations, and CLI commands function without permissions errors.
2. **Tenant Quota Budgeting (`QuotaRegistry`)**:
   - Stored in `~/.craft/quotas/quotas.toml` with `quotas.lock` advisory locking (`fs2`).
   - Tracks tenant limits: `max_servers`, `max_memory_bytes`, `max_cpu_percent`, `max_storage_bytes`.
   - Strictly enforces budget allocation; rejects over-committing server allocations that exceed tenant limits unless `allow_burst` is enabled.
3. **Priority Tiers & Fair-Share Rebalancing**:
   - Four priority tiers:
     - `GatewayProxy`: CPU weight 500, IO weight 500 (guaranteed low latency for Velocity, BungeeCord, HAProxy).
     - `StandardWorld`: CPU weight 100, IO weight 100 (standard CFS slice for game servers).
     - `BackgroundWorker`: CPU weight 50, IO weight 50 (batch tasks like Dynmap, world pre-gen).
     - `BatchTask`: CPU weight 20, IO weight 20 (backup compression, snapshot export).
   - In-process `QuotaService::enforce_fair_share` re-allocates weights dynamically during host saturation.
4. **Hot-Reloadable Limits**:
   - Cgroups v2 limits can be modified on running servers instantly via `craft quota set <server> --cpu <percent> --memory <mb>` without restarting the game server process.
5. **Lifecycle Hooks & Auditing**:
   - `LifecycleEvent::ResourceQuotaExceeded`, `CgroupThrottled`, `FairShareAdjusted` fire into the embedded Lua hook bus with live metrics context (`memory_current_bytes`, `cpu_throttled_usec`, `throttle_ratio`, `tenant_id`).

