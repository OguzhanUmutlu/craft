# Embedded Lua Scripting Runtime, Automation & Lifecycle Hooks Skill Guide

> **Domain**: Embedded Lua 5.4 VM, Event-Driven Lifecycle Bus, Headless Automation & ModalX TUI  
> **Primary Location**: `crates/scripting/`, `crates/daemon/`, `crates/cli/src/commands/script.rs`

---

## 1. Lua 5.4 Runtime Architecture & Isolation

Craft embeds Lua 5.4 via `mlua 0.12` to provide sandboxed, high-performance scripting capabilities for dedicated server orchestration, automation pipelines, and reactive lifecycle event processing.

```
┌─────────────────────────────────────────────────────────────┐
│                       Craft CLI / TUI                       │
│      (craft script run | eval | list | test | new)          │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    craft-scripting Engine                   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                 Lua 5.4 Sandbox                     │   │
│   │  craft.log | craft.platform | craft.fs | craft.http │   │
│   │  craft.servers | craft.backup | craft.net           │   │
│   │  craft.audit | craft.exec                           │   │
│   └─────────────────────────────────────────────────────┘   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │            Execution Deadline Guard                 │   │
│   │   HookTriggers::default().every_nth_instruction()   │   │
│   └─────────────────────────────────────────────────────┘   │
└──────────────────────────────▲──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 Event-Driven Hook Bus                       │
│    Global: ~/.craft/hooks/ | Per-Server: <dir>/hooks.lua    │
│    Audit Dispatch Log: ~/.craft/logs/hooks.log              │
└──────────────────────────────▲──────────────────────────────┘
                               │
                tokio::spawn(dispatch_async)
                               │
┌──────────────────────────────┴──────────────────────────────┐
│                    craft-daemon Supervisor                  │
│   Triggers: ServerStart, ServerStop, ServerCrash,           │
│             CircuitTrip, BackupStart, BackupComplete,       │
│             StorageLow, PlayerJoin                          │
└─────────────────────────────────────────────────────────────┘
```

### 1.1. Execution Guardrails & Timeout Enforcement
To prevent rogue Lua scripts from entering infinite loops or exhausting supervisor resources, the runtime enforces an instruction-counting deadline hook:

```rust
let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1));
let _ = self.lua.set_hook(
    HookTriggers::default().every_nth_instruction(5000),
    move |_, _| {
        if Instant::now() >= deadline {
            Err(mlua::Error::RuntimeError(format!(
                "Script execution timed out after {}s",
                timeout_secs
            )))
        } else {
            Ok(mlua::VmState::Continue)
        }
    },
);
```

---

## 2. Standard Library API Reference (`craft.*`)

All Lua scripts executed under Craft have access to the sandboxed `craft.*` namespace:

### 2.1. Logging (`craft.log`)
- `craft.log.info(msg: string)`: Log informational message with `[INFO]` prefix.
- `craft.log.warn(msg: string)`: Log warning message with `[WARN]` prefix.
- `craft.log.error(msg: string)`: Log error message with `[ERROR]` prefix.
- `craft.log.debug(msg: string)`: Log debug message with `[DEBUG]` prefix.

### 2.2. Host Platform & Environment (`craft.platform`)
- `craft.platform.os() -> string`: Current operating system (`"linux"`, `"windows"`, `"macos"`).
- `craft.platform.arch() -> string`: Architecture string (`"x86_64"`, `"aarch64"`).
- `craft.platform.is_windows() -> boolean`: True if running on Windows.
- `craft.platform.is_linux() -> boolean`: True if running on Linux.
- `craft.platform.is_macos() -> boolean`: True if running on macOS.
- `craft.platform.env(var_name: string) -> string|nil`: Read environment variable.

### 2.3. Safe Filesystem Operations (`craft.fs`)
- `craft.fs.exists(path: string) -> boolean`: Check if file or directory exists.
- `craft.fs.is_file(path: string) -> boolean`: Check if path is a regular file.
- `craft.fs.is_dir(path: string) -> boolean`: Check if path is a directory.
- `craft.fs.read_string(path: string) -> string`: Read text file contents.
- `craft.fs.write_string(path: string, content: string) -> boolean`: Write text file contents.
- `craft.fs.copy(src: string, dst: string) -> boolean`: Copy file or directory.
- `craft.fs.remove(path: string) -> boolean`: Remove file or directory.
- `craft.fs.size(path: string) -> integer`: Get file size in bytes.

### 2.4. Fleet Server Management (`craft.servers`)
- `craft.servers.list() -> table`: Returns array of server tables `{ name, path, software, game, port, running, pid }`.
- `craft.servers.get(name: string) -> table|nil`: Returns server metadata table.
- `craft.servers.start(name: string) -> boolean, string`: Dispatches server startup via daemon IPC.
- `craft.servers.stop(name: string) -> boolean, string`: Dispatches server shutdown via daemon IPC.
- `craft.servers.command(name: string, command: string) -> boolean, string`: Injects stdin console command.

### 2.5. Hot Backup Operations (`craft.backup`)
- `craft.backup.list(server_name: string) -> table`: Returns array of backup objects `{ filename, size_bytes, modified_epoch }`.
- `craft.backup.create(server_name: string, note?: string) -> table`: Dispatches hot backup and returns summary table `{ filename, size_bytes, path }`.

### 2.6. Binary Protocol Network Ping (`craft.net`)
- `craft.net.ping(host: string, port: integer) -> table`: Queries remote game server via SLP/RakNet/A2S_INFO. Returns `{ online: boolean, motd: string, players_online: integer, players_max: integer, version: string, latency_ms: integer }`.

### 2.7. Outbound HTTP Networking (`craft.http`)
- `craft.http.get(url: string, headers?: table) -> table`: Synchronous HTTP GET. Returns `{ status: integer, body: string, ok: boolean }`.
- `craft.http.post(url: string, body: string, headers?: table) -> table`: Synchronous HTTP POST. Returns `{ status: integer, body: string, ok: boolean }`.

### 2.8. Immutable Cryptographic Audit Log (`craft.audit`)
- `craft.audit.log(action: string, details?: string, server?: string) -> boolean`: Appends an entry into Craft's HMAC-SHA256 verified audit ledger.

### 2.9. Command Execution (`craft.exec`)
- `craft.exec.run(command: string, args: table) -> table`: Executes host process with captured output. Returns `{ exit_code: integer, stdout: string, stderr: string }`.

---

## 3. Server Lifecycle Hook Bus

The supervisor daemon triggers asynchronous lifecycle hooks at critical state transitions.

### 3.1. Supported Lifecycle Triggers
1. `ServerStart`: Dispatched when a server process is spawned and detached.
2. `ServerStop`: Dispatched when an intentional stop command is executed.
3. `ServerCrash`: Dispatched when a child process exits abnormally with a non-zero code.
4. `CircuitTrip`: Dispatched when rapid crash loops trigger the daemon circuit breaker.
5. `BackupStart`: Dispatched immediately prior to atomic snapshot creation.
6. `BackupComplete`: Dispatched upon successful backup archive generation.
7. `StorageLow`: Dispatched when free disk space falls below critical thresholds (<5 GB).
8. `PlayerJoin`: Dispatched upon player network handshake or proxy login.

### 3.2. Script Discovery Hierarchy
Hooks are automatically discovered and executed in the following order:
1. **Global Unified Hook**: `~/.craft/hooks/hooks.lua` (matches function name corresponding to event).
2. **Global Event-Specific Hook**: `~/.craft/hooks/<event_name>.lua` (e.g. `on_server_crash.lua`).
3. **Per-Server Hook**: `<server_dir>/hooks.lua` (scoped exclusively to the target server).

### 3.3. Hook Context Table (`context`)
Every hook script receives an injected global table `context` with the following schema:
```lua
context = {
    event = "ServerCrash",
    timestamp = 1718000000,
    server_name = "survival-1",
    server_path = "/home/usr/.craft/servers/survival-1",
    port = 25565,
    error = "Process terminated with exit code 1",
    details = "OutOfMemoryError in Java Virtual Machine"
}
```

### 3.4. Logging and Audit Trail
Every hook invocation is recorded to `~/.craft/logs/hooks.log` with timestamp, event kind, duration in milliseconds, success flag, and any runtime error traces.

---

## 4. Headless CLI Automation Suite

```bash
# Execute standalone script
craft script run /path/to/script.lua

# Execute script with custom server context and timeout
craft script run maintenance.lua --server lobby-1 --timeout 60 -- --clean --dry-run

# Inline evaluation of Lua expressions
craft script eval "return craft.platform.os()"
craft script eval "craft.servers.list()"

# List all registered lifecycle hooks
craft script list
craft script list --server survival-1

# Simulate / test lifecycle hook execution
craft script test ServerCrash --server survival-1
craft script test StorageLow

# Scaffold a new hook script
craft script new on_server_crash
craft script new notify_discord --server survival-1
```

---

## 5. ModalX Interactive Studio Submenu

Craft's interactive dashboard incorporates the **Lua Automation & Lifecycle Hooks** panel:
- **Hook Catalog**: Visualizes registered global and per-server hooks with status indicators (`[GLOBAL]`, `[LOCAL]`).
- **Event Simulator**: Interactive wizard to simulate lifecycle events (`ServerStart`, `BackupComplete`, `StorageLow`) with real-time feedback and execution duration.
- **Headless Runner**: Execute any `.lua` file interactively with stdout modal replay.
- **Inline Expression Evaluator**: Interactive REPL to evaluate Lua expressions and view typed return values.
- **Scaffolding Wizard**: Generates production-ready Lua templates in `~/.craft/scripts/` or per-server directories.
