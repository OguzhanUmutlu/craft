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
