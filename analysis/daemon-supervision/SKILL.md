# Daemon & Background Supervision Skill Guide

> **Domain**: Background Process Supervision, IPC, Ring-Buffer Log Replay & OS Services  
> **Primary Location**: `crates/daemon/`

---

## 1. Supervisor Daemon Architecture

Craft's daemon (`craft-daemon`) operates detached from interactive terminals, supervising active game server processes, streaming logs in real time, and surviving terminal closures:

```
[ CLI Client (craft view / dashboard) ]
              │
              │  IPC Transport: Unix Domain Socket / Named Pipe
              ▼
[ craft-daemon Supervisor ]
  ├── Client Connection Handler (tokio::spawn)
  ├── Server Process Runners (Child stdin/stdout/stderr)
  ├── 50,000-Line Circular Log Buffer (Per-Server in Memory)
  └── On-Disk Logger (<server_path>/logs/console.log)
```

---

## 2. IPC Protocol Specification

- **Transport**:
  - Linux/macOS: `~/.craft/run/daemon.sock` via `tokio::net::UnixListener` / `UnixStream`.
  - Windows: `\\.\pipe\craft-daemon` via `tokio::net::windows::named_pipe::ServerOptions`.
- **Message Types**:
  - Request:
    - `StartServer { path, software, memory, jvm_args, java_path }`
    - `StopServer { path }`
    - `RestartServer { path }`
    - `AttachConsole { path }`
    - `SendInput { path, input }`
    - `ListRunning`
  - Response:
    - `Ok`, `Error(String)`
    - `LogBacklog(String)` (Instant replay of recent buffer upon attachment)
    - `LogChunk(String)` (Continuous live broadcast)

---

## 3. Log Streaming & Ring Buffer Mechanics

### 3.1. 50,000-Line Circular Buffer
- Each running server instance has an associated bounded circular buffer (`VecDeque<String>`) in memory capped at 50,000 entries.
- When new log lines arrive from child `stdout` or `stderr`, older lines are popped from the front once capacity is reached.
- This ensures memory overhead per server remains bounded (<10 MB RSS) even over months of uptime.

### 3.2. Clean Stream Filtering
- **Carriage Return Stripping**: Trailing `\r` characters from Windows console outputs or legacy Java wrappers are stripped before storing or forwarding.
- **Empty Line Suppression**: Consecutive whitespace-only lines are discarded to prevent double-spacing glitches in TUI consoles.
- **Persistent Disk Logging**: Every line is appended to `<server_path>/logs/console.log` with timestamp indexing.

---

## 4. Native OS Service Unit Integration

Craft provides automated service configuration via `craft service install`:
1. **Linux (Systemd User Unit)**:
   - Path: `~/.config/systemd/user/craft.service`
   - Configures `ExecStart=/usr/local/bin/craft service start --foreground`, `Restart=on-failure`, and triggers `loginctl enable-linger $USER` for persistence across SSH logouts.
2. **macOS (LaunchAgent)**:
   - Path: `~/Library/LaunchAgents/com.craft.daemon.plist`
   - Configures `RunAtLoad=true` and `KeepAlive=true` for launchd management.
3. **Windows (Scheduled Task)**:
   - Configures an automatic startup task running `craft.exe service start --foreground` upon user logon.

---

## 5. Crash Circuit Breaker & Exponential Backoff

Uncontrolled auto-restart loops can peg host CPU cores, spam log disks, and corrupt world databases. `craft-daemon` integrates a sliding-window crash circuit breaker (`CircuitBreaker`):

```
       [ Crash Event ]
              │
              ▼
   ┌─────────────────────┐
   │ Check Window Count  │─── < 3 crashes / 60s ───► [ Closed ] ──► Backoff (2s..60s) ──► Restart
   └─────────────────────┘
              │
        >= 3 crashes
              │
              ▼
         [ Open ] ────────► Halts auto-restarts; alert logged; requires reset or cool-off
              │
        Cooldown (60s)
              │
              ▼
       [ Half-Open ] ─────► Canary start trial
              │
     ┌────────┴────────┐
     ▼                 ▼
[ Crashes ]      [ Healthy 120s ]
     │                 │
   [ Open ]        [ Closed ]
```

### 5.1. Backoff Progression
- Delay progression: `2s -> 5s -> 15s -> 30s -> 60s` maximum.
- **Healthy Run Auto-Reset**: Once a server runs continuously for 120 seconds without exiting, the failure counter automatically resets to zero.

### 5.2. Breaker States (`BreakerState`)
- `Closed`: Normal monitoring; failures increment sliding-window tally.
- `Open`: Tripped; blocks further automated restarts until cooldown expires or manual reset is triggered.
- `HalfOpen`: Single canary start attempt allowed; failure returns to `Open`, success returns to `Closed`.

---

## 6. Asynchronous Decoupled Restart Queue

In Rust, spawning a recursive `tokio::spawn` task inside an async supervisor method that calls another async supervisor method triggers compiler errors regarding auto-trait bounds (`Send` cycle across nested futures).

### Architectural Solution:
Supervisor uses an unbounded Tokio channel (`restart_tx: mpsc::UnboundedSender<PathBuf>`):
1. When a child process terminates unexpectedly, the crash monitor checks the circuit breaker.
2. If allowed, it applies backoff sleep and pushes the server path into `restart_tx`.
3. A decoupled background task reads from `restart_rx` and calls `supervisor.start_server(&path)`.
4. This completely breaks the future cycle while maintaining safe, orderly restarts.

---

## 7. Extended IPC Commands

The daemon protocol supports inspecting and manipulating supervision subsystems:
- **`GetCircuitBreakers`**: Returns a list of all tracked servers with their current state (`Closed`, `Open`, `HalfOpen`), failure counts, and next retry delays.
- **`ResetCircuitBreaker { path }`**: Clears failure history and resets the circuit breaker to `Closed`.
- **`GetBackupSchedules`**: Returns all configured automated backup policies and their last execution timestamps.

---

## 8. Enterprise Telemetry & Prometheus `/metrics` Engine

The supervisor exposes standard Prometheus exposition format telemetry via HTTP `GET /metrics`:
- **Host Metrics**: Total/used RAM, CPU utilization percentage, total/available disk storage capacity via `sysinfo`.
- **Server Lifecycle Metrics**: Status gauge (`1` = running, `0` = stopped), RSS memory resident bytes, child CPU utilization, consecutive crash counts, and circuit breaker numerical states (`0` = Closed, `1` = Half-Open, `2` = Open).
- **Network Query Telemetry**: Queries online players, maximum player slots, and ping latency in milliseconds through `craft-net` (`ping_server_auto`) supporting Minecraft Java SLP, Bedrock RakNet, Valve A2S_INFO, and TCP port probing.
- **Label Sanitization**: Prometheus metric names and labels enforce standard Prometheus naming conventions (`[a-zA-Z_:][a-zA-Z0-9_:]*`), escaping invalid characters.

---

## 9. Event-Driven Webhook Dispatching & HMAC Signatures

Craft features an asynchronous webhook notification pipeline (`WebhookDispatcher`) configured in `~/.craft/webhooks.toml`:
- **Event Triggers**: `ServerCrash`, `AutoRestart`, `CircuitTrip`, `BackupComplete`, `StorageExhaustion`, `ServerStart`, `ServerStop`.
- **Target Platform Formatting**:
  - `Discord`: Rich embeds with color-coded severity borders (Red for crashes/trips, Green for healthy backups/starts, Yellow for auto-restarts/storage warnings), structured field grids, and timestamps.
  - `Slack`: Block Kit formatted messages with header sections, bold mrkdwn fields, and operational context.
  - `GenericJson`: Machine-readable JSON payloads suitable for custom webhooks, SIEM integration, and orchestration pipelines.
- **Cryptographic Security**: Payloads are signed using pure-Rust HMAC-SHA256 (`X-Craft-Signature: sha256=<hex>`), providing tamper-proof payload validation for receiving endpoints.
- **Concurrency & Resilience**: Dispatches occur asynchronously via decoupled Tokio background tasks with 10-second request timeouts and standard `Craft-Daemon/1.0` user agent headers.

---

## 10. Authenticated WebSocket Gateway & Prefixed Stream Replay

The remote gateway allows web dashboards and remote CLI tools to attach to server live console feeds and send stdin inputs via `ws://<bind>:<port>/ws/console?server=<name>`:
- **Prefixed Stream Replay**: To distinguish between standard HTTP requests (`GET /metrics`, `GET /health`) and WebSocket upgrade handshakes without losing bytes, `PrefixedStream` wraps `Cursor<Vec<u8>>` and delegates to `TcpStream` once the initial byte prefix is drained.
- **Authentication**: Constant-time Bearer token verification (`verify_token`) via `Authorization: Bearer <token>` or `?token=<token>` query parameters prevents timing attack vulnerabilities.
- **Sliding-Window Rate Limiting**: In-memory IP-based rate limiter restricts connection rates to 30 requests per minute per IP address, automatically pruning expired windows.
- **Bi-Directional Streaming**: Streams real-time circular buffer broadcasts to connected WebSocket clients while sanitizing and injecting incoming JSON command payloads directly into child process `stdin`.

---

## 11. Disk Storage Exhaustion Background Monitoring

The daemon runs a periodic background storage monitor task every 5 minutes:
- Audits the mount point hosting `~/.craft/` using `sysinfo::Disks`.
- Evaluates free disk space against configured thresholds (`storage_warning_threshold_bytes` [default 5 GB] and `storage_warning_threshold_percent` [default 10%]).
- When storage drops below either threshold, dispatches `WebhookPayload::storage_exhaustion` with an hourly alert cooldown to avoid notification flooding.


