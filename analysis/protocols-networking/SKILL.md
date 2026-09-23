# Protocols & Networking Security Skill Guide

> **Domain**: Native Binary Protocols, Network Ping, RCON & OS Firewalls  
> **Primary Location**: `crates/net/`

---

## 1. Native Binary Protocols

Craft implements all network query protocols natively in pure Rust without relying on external system utilities or CLI wrappers:

```
[ craft-net ]
    ├── Java SLP Engine     (TCP: VarInt Framing, 0x00 Handshake, JSON Status)
    ├── Bedrock RakNet      (UDP: 0x01 Unconnected Ping, 0x1C Unconnected Pong)
    ├── Valve A2S_INFO      (UDP: Source Engine Protocol, Challenge Handling)
    ├── Async RCON Client   (TCP: RFC-Compliant Packet Framing & Auth)
    ├── OS Firewall Engine  (ufw, iptables, pfctl, netsh)
    └── UWP Loopback Tool   (CheckNetIsolation for Bedrock localhost)
```

---

## 2. Protocol Specifications

### 2.1. Minecraft Java Server List Ping (SLP)
- **Transport**: TCP
- **Handshake Flow**:
  1. Client sends `0x00` Handshake packet containing `[Protocol Version (VarInt), Server Address (String), Server Port (u16), Next State: 1 (VarInt)]`.
  2. Client sends `0x00` Status Request packet (empty payload).
  3. Server responds with `0x00` Status Response containing a JSON payload with server description, player counts (`online`, `max`), and sample player list.
- **Latency Measurement**: Recorded as the duration between sending the status request and receiving the full frame.

### 2.2. Bedrock RakNet Unconnected Ping
- **Transport**: UDP
- **Packet Structure**:
  - Packet ID: `0x01` (Unconnected Ping)
  - Time: 64-bit Big-Endian timestamp
  - Magic: 16-byte fixed RakNet offline identifier (`0x00ffff00fefefefefdfdfdfd12345678`)
  - Client GUID: 64-bit random identifier
- **Response**: `0x1c` (Unconnected Pong) containing server GUID and a string delimited by `;`:
  - `[Edition; MOTD; Protocol; Version; Online; Max; ServerID; WorldName; GameMode; ...]`.

### 2.3. Valve A2S_INFO Query Protocol
- **Transport**: UDP
- **Used by**: Dedicated game servers (Palworld, Valheim).
- **Format**: Sends header `0xFFFFFFFF` followed by character `'T'` (`0x54`) and payload string `"Source Engine Query\0"`.
- **Challenge Handling**: Handles challenge tokens returned by modern servers by reflecting the 4-byte token in a follow-up query packet.

### 2.4. Asynchronous RCON Protocol
- **Transport**: TCP
- **Packet Wire Format**:
  - `Length`: 32-bit signed integer (Little-Endian)
  - `Request ID`: 32-bit signed integer (Little-Endian)
  - `Type`: 32-bit signed integer (`3` = Auth, `2` = Exec Command)
  - `Body`: Null-terminated ASCII/UTF-8 string
  - `Padding`: 2-byte null terminator (`0x00 0x00`)
- **Safety**: Supports multi-packet response assembly for commands with outputs exceeding 4096 bytes.

### 2.5. Pure-Rust TCP `SleepProxy` & Packet Wake-Up Service
- **Transport**: TCP (binds to the sleeping server's port)
- **Status State (`next_state = 1`)**:
  - Responds to `0x00` Status Request with a JSON description containing the sleeping MOTD (`"[Craft] Server is sleeping. Connect to wake up!"`), 0 online players, and version identifier.
  - Responds to `0x01` Ping with Pong echoing the 64-bit timestamp.
- **Login State (`next_state = 2`)**:
  - Intercepts player login handshake.
  - Sends immediate server wake signal across an asynchronous `mpsc::Sender<String>` channel to the daemon supervisor.
  - Disconnects player cleanly with a `0x00` Login Disconnect packet containing a descriptive chat JSON payload (`"[Craft] Server is starting up! Please reconnect in 15 seconds."`), avoiding TCP socket hanging and connection timeout errors on the client.
- **Port Teardown**: Upon wake signal dispatch, `SleepProxyHandle::shutdown` terminates the TCP listener, allowing the actual dedicated server process to bind the port cleanly without collision.

---

## 3. Host Firewall & Security Automation

Craft provides programmatic OS-level firewall provisioning via `craft firewall`:
1. **Linux (UFW & iptables)**:
   - Evaluates whether UFW is active; falls back to raw `iptables` if absent.
   - Restricts Minecraft ports (e.g. 25565) to trusted backend IPs (e.g. Velocity or Bungee proxy IPs) to prevent port-bypass vulnerabilities.
2. **macOS (`pfctl`)**:
   - Manages packet filter anchors under `/etc/pf.anchors/com.craft`.
3. **Windows (`netsh advfirewall`)**:
   - Injects inbound rules restricting ports to specific remote IP masks.

---

## 4. Windows UWP Loopback Exemption

By default, the Windows AppContainer sandbox prevents UWP applications (like Minecraft Bedrock for Windows) from opening local connections to `127.0.0.1`.
- Craft invokes `CheckNetIsolation.exe LoopbackExempt -a -n=Microsoft.MinecraftUWP_8wekyb3d8bbwe` to automate developer and player access without manual registry hacking.

---

## 5. Remote Gateway WebSocket Protocol & Handshakes

The gateway service exposes a unified TCP listener on port 8124 handling both HTTP and WebSocket connections:
- **Prefixed Handshake Routing**: The listener reads the initial HTTP request buffer into memory. If the path is `/ws/console`, the connection is upgraded to WebSocket framing via `tokio-tungstenite` using `PrefixedStream` to replay the consumed handshake bytes without dropping data.
- **Constant-Time Verification**: Bearer tokens are validated using bitwise XOR accumulation over equalized length slices (`verify_token`), neutralizing timing side-channel attacks.
- **Console Stream Protocol**:
  - Live lines from the circular log ring buffer are transmitted as JSON frames: `{"type": "log", "line": "..."}`.
  - Client command injection payloads accept: `{"command": "say Hello"}` or plain text strings.
  - WebSocket Ping/Pong keep-alive frames are processed transparently with automatic replies.

---

## 6. Global Edge Mesh & Multi-Sample Latency Probing

Craft implements an active multi-region edge mesh topology backed by `~/.craft/edge.toml` under `edge.lock` advisory file locks:
- **Multi-Sample TCP Prober (`EdgeLatencyProber`)**:
  - Emits $N$ consecutive TCP handshakes (configurable timeout, default 1500ms) to measure min, max, and average round-trip time (RTT).
  - Calculates packet loss percentage: $\frac{\text{failed\_samples}}{\text{total\_samples}} \times 100$.
  - Computes sample standard deviation (jitter) in milliseconds:
    $$\sigma = \sqrt{\frac{1}{N - 1} \sum_{i=1}^N (x_i - \bar{x})^2}$$
- **Optimal Route Selection**:
  - Evaluates registered edge nodes against `GeoRoutingPolicy`:
    - `LowestLatency`: Selects candidate with minimum average RTT.
    - `GeographicProximity`: Evaluates region affinity (e.g. `us-east` preferred over `ap-southeast`).
    - `WeightedRoundRobin`: Balances load proportionally based on node weight thresholds.
    - `Failover`: Evaluates primary node first, falling back to backup nodes upon consecutive probe timeouts.
- **Backbone Condition Matrix**:
  - Classifies regional network stability into discrete health tiers:
    - `Optimal`: Latency $< 45$ ms, Jitter $< 5$ ms, Loss $= 0\%$.
    - `Elevated`: Latency $< 100$ ms, Jitter $< 20$ ms, Loss $< 2\%$.
    - `Degraded`: Latency $< 200$ ms, Jitter $< 50$ ms, Loss $< 5\%$.
    - `Critical`: Latency $\ge 200$ ms or Loss $\ge 5\%$.

---

## 7. Dynamic Proxy Route Generation & Single-Use Player State Handoffs

- **Dynamic Edge Route Generator (`EdgeRouteGenerator`)**:
  - Generates drop-in routing configurations without external scripting or template engines:
    - **Velocity (`velocity.toml`)**: Formats `[servers]` mapping backend server targets with clean try order fallbacks.
    - **BungeeCord (`config.yml`)**: Emits YAML `servers:` nodes with hostnames, MOTDs, and restricted access flags.
    - **HAProxy L4 (`haproxy.cfg`)**: Emits `frontend` and `backend` sections configured in `mode tcp` with TCP keep-alive, balance roundrobin, and health check intervals.
    - **Envoy L4 (`envoy.yaml`)**: Generates Envoy static cluster configurations with TCP proxy filters and cluster endpoints.
- **Single-Use Player State Handoffs (`EdgeStateBroker`)**:
  - Orchestrates seamless cross-region player transfers between clusters.
  - Issues time-bounded (60-second TTL) single-use cryptographic transfer tokens (`PlayerSessionHandoff`).
  - Encapsulates inventory snapshots (`InventorySnapshot`), potion effects, health, experience levels, and game mode.
  - Atomic one-time consumption guard: Once claimed by the destination server daemon, the handoff token is permanently invalidated, strictly preventing inventory duplication attacks.
  - HMAC-SHA256 authenticated cross-region chat envelopes (`CrossRegionChatEnvelope`) provide tamper-proof inter-server communication.
- **Dynamic Latency Optimization Playbooks (`LatencyPlaybook`)**:
  - Presets: `CompetitivePvP` (low latency, high tick fidelity), `MegaSMP` (dynamic view distance throttling), `CrossRegionEconomy` (buffered chat synchronization).
  - Autonomously tunes `server.properties` parameters (`view-distance`, `simulation-distance`, `network-compression-threshold`) to cushion servers during backbone degradation.

---

## 8. Real-Time Tick Profiling, Netty Packet Inspection & Latency Micro-Histograms

Phase 18 introduces real-time tick duration profiling, sliding-window MSPT analysis, Netty packet rate inspection, and logarithmic latency micro-histograms into `craft-net` and the supervisor daemon.

### 8.1. High-Resolution Logarithmic Micro-Histograms (`LatencyHistogram`)
- **Memory Footprint**: Strict constant bounded memory (< 2 KB) with constant $O(1)$ sample insertion without dynamically allocated sample vectors.
- **Logarithmic Decades**:
  - 8 decades covering $1\,\mu\text{s}$ ($10^0$) up to $100,000,000\,\mu\text{s}$ ($10^8\,\mu\text{s} = 100\,\text{s}$).
  - 9 contiguous sub-intervals per decade ($1\times, 2\times, \dots, 9\times \text{base}$), yielding exactly 72 contiguous, non-overlapping buckets.
  - Bucket Index Mapping:
    $$\text{decade} = \min\left(7, \lfloor \log_{10}(v) \rfloor\right), \quad \text{sub} = \min\left(8, \left\lfloor \frac{v}{10^{\text{decade}}} \right\rfloor - 1\right), \quad \text{idx} = \text{decade} \times 9 + \text{sub}$$
  - Invertible Bucket Bounds:
    $$\text{low} = (\text{sub} + 1) \cdot 10^{\text{decade}}, \quad \text{high} = \begin{cases} (\text{sub} + 2) \cdot 10^{\text{decade}} & \text{if } \text{sub} < 8 \\ 10^{\text{decade}+1} & \text{if } \text{sub} = 8 \end{cases}$$
- **Linear Quantile Interpolation**:
  - Quantiles ($P_{50}, P_{90}, P_{95}, P_{99}, P_{99.9}$) compute target rank $R = \lceil q \cdot N \rceil$.
  - Interpolates linearly within the enclosing bucket:
    $$V_q = \text{low} + \frac{R - C_{\text{prev}}}{C_{\text{bucket}}} \cdot (\text{high} - \text{low})$$
- **ASCII Histogram Visualization**:
  - `render_ascii(width)` displays human-readable microsecond/millisecond bucket labels, sample counts, and normalized Unicode block glyph bars (`█`).

### 8.2. Real-Time Tick Profiler & MSPT Estimator (`TickProfiler`)
- **Sliding-Window Ring Buffer**:
  - Fixed capacity (default 60–120 samples) storing timestamped `TickSample` records.
  - Computes moving average MSPT and effective server TPS:
    $$\text{TPS} = \min\left(20.0, \frac{1000.0}{\text{MSPT}}\right)$$
- **Jitter (Sample Standard Deviation)**:
  $$\sigma = \sqrt{\frac{1}{N - 1} \sum_{i=1}^N (\text{MSPT}_i - \overline{\text{MSPT}})^2}$$
- **Operational Health Classification (`TickHealthGrade`)**:
  - `[PRISTINE]`: MSPT $< 25.0\,\text{ms}$, $\text{TPS} \ge 19.8$.
  - `[STABLE]`: MSPT $< 45.0\,\text{ms}$, $\text{TPS} \ge 18.0$.
  - `[DEGRADED]`: MSPT $< 60.0\,\text{ms}$, $\text{TPS} \ge 14.0$.
  - `[OVERLOADED]`: MSPT $\ge 60.0\,\text{ms}$ or $\text{TPS} < 14.0$.
- **ASCII Sparkline Generator**:
  - Quantizes sample history into an 8-level sparkline string (` ▂▃▄▅▆▇█`) with zero terminal width distortion.

### 8.3. Netty Ingress/Egress Packet & Byte Inspector (`NettyPacketInspector`)
- **Throughput Metrics**:
  - Tracks Ingress RX PPS, Egress TX PPS, RX bytes/sec, and TX bytes/sec.
  - Maintains a sliding history of rate samples to calculate burst ratios.
- **Burst & Flood Anomaly Detection**:
  - Detects traffic surges when $\frac{\text{current\_pps}}{\text{baseline\_pps}} \ge 3.5\times$.
  - Triggers `PacketFloodAnomaly` when RX PPS exceeds safe capacity threshold (default 5,000 PPS).

### 8.4. Daemon Telemetry Loop & CLI Interface
- **In-Process `TickService`**: Runs continuous loopback network probes against active servers, recording tick durations and packet exchanges into shared `ServerTelemetryState`.
- **IPC Protocol Extension**:
  - `IpcRequest::GetTickProfile { server_name }` -> `IpcResponse::TickProfile { summary, sparkline }`
  - `IpcRequest::GetPacketStats { server_name }` -> `IpcResponse::PacketStats { summary }`
  - `IpcRequest::GetLatencyHistogram { server_name }` -> `IpcResponse::LatencyHistogram { histogram, chart_lines }`
- **CLI Commands**:
  - `craft profile tick <server> [-w 60]`: Detailed MSPT table, percentiles, jitter, and ASCII sparkline.
  - `craft profile packets <server>`: Netty RX/TX throughput, bandwidth, and burst status.
  - `craft profile histogram <server> [-w 60]`: Logarithmic latency distribution and ASCII bar chart.
  - `craft profile overview <server>`: Unified diagnostic overview combining all telemetry.
- **ModalX Centered TUI**: Integrated into `craft manage` -> `Tools` -> `Tick Profiling & Network Telemetry`.

---

## 9. Distributed Real-Time Tracing, OpenTelemetry Export & W3C Context Propagation

Craft incorporates a zero-dependency, pure-Rust distributed tracing engine compliant with the **W3C Trace Context** standard and OpenTelemetry (OTel) OTLP/HTTP specifications.

### 9.1. W3C Traceparent Wire Format & Non-Zero Invariants
- **Format**: `version-trace_id-span_id-trace_flags` (e.g., `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`).
  - `version`: Fixed 2-hex-digit protocol version (`00`). Rejects `ff`.
  - `trace_id`: 16-byte (32-hex-character) lowercase unique distributed identifier. Cannot be all zeros (`00000000000000000000000000000000`).
  - `span_id`: 8-byte (16-hex-character) lowercase local operation identifier. Cannot be all zeros (`0000000000000000`).
  - `trace_flags`: 8-bit hex field. Bit 0 indicates sampling status (`01` = recorded/sampled, `00` = not sampled).
- **Tracestate**: Supports vendor key-value pairs formatted as comma-separated lists (`craft=node1,rojo=2`).
- **Inter-Process & SSH Propagation**:
  - Injected via environment variable `CRAFT_TRACEPARENT` across remote SSH sessions (`RemoteCraftClient::exec_with_trace_context`).
  - Child processes and subprocess executions inherit the parent distributed trace ID while instantiating fresh child span IDs.

### 9.2. Pure-Rust Tracer Engine & RAII Span Guard
- **`Tracer` & `TraceSampler`**:
  - Provides thread-safe, lock-free span lifecycle management.
  - Configurable sampling strategies: `AlwaysOn`, `AlwaysOff`, and `Ratio(f64)` (evaluated against pseudo-random high-entropy bytes).
- **RAII `ActiveSpan`**:
  - Automatically records span start timestamp upon creation and computes total nanosecond elapsed duration upon drop or explicit `finish()`.
  - Supports structured attributes (`SpanAttributeValue::String|Int|Float|Bool`), inline lifecycle events (`SpanEvent`), and causal span links (`SpanLink`).
  - Records span status codes (`Unset`, `Ok`, `Error`) with optional error messages.

### 9.3. Bounded Circular Span Ring Buffer (`SpanRingBuffer`)
- **Memory Footprint & Zero TPS Penalty**:
  - Implemented as a bounded `VecDeque<RecordedSpan>` with fixed capacity (default 10,000 spans) protected by mutex.
  - $O(1)$ push and drain operations ensure zero performance penalty on 20.0 TPS game loops.
  - When capacity is saturated, oldest spans are discarded (FIFO) and an atomic drop counter (`AtomicU64`) is incremented.

### 9.4. OpenTelemetry OTLP/HTTP JSON Exporter (`OtlpJsonExporter`)
- **Payload Schema**:
  - Serializes recorded span batches into standard OTLP/HTTP JSON (`/v1/traces`):
  ```json
  {
    "resourceSpans": [{
      "resource": {
        "attributes": [
          {"key": "service.name", "value": {"stringValue": "craft-daemon"}},
          {"key": "telemetry.sdk.language", "value": {"stringValue": "rust"}}
        ]
      },
      "scopeSpans": [{
        "scope": {"name": "craft.tracing", "version": "1.0.0"},
        "spans": [...]
      }]
    }]
  }
  ```
- **Collector Interoperability**: Direct push export compatibility with Jaeger, Grafana Tempo, SigNoz, and OpenTelemetry Collector.
- **Offline Fallback**: When no HTTP endpoint is defined or collector is offline, batches are written to `.craft/tracing/spans/trace_export_<timestamp>.json`.

### 9.5. Causal Tree Reconstruction (`TraceTree`)
- Reconstructs arbitrary batches of out-of-order `RecordedSpan` entries into an acyclic directed causal tree (`TraceTreeNode`).
- Distinguishes root spans from internal child spans and formats ASCII dependency graphs:
```
`-- [SERVER] game_loop (craft-daemon) [dur: 15.20ms] [OK]
    |-- [INTERNAL] tick_world (craft-daemon) [dur: 10.10ms] [OK]
    |   `-- [CLIENT] save_chunk (craft-daemon) [dur: 3.40ms] [OK]
    `-- [PRODUCER] network_flush (craft-daemon) [dur: 4.20ms] [OK]
```

### 9.6. Daemon IPC, Scripting Hooks & CLI Interface
- **Daemon IPC**:
  - `GetTracingStatus` -> `TracingStatusResult`
  - `QueryTraces` -> `TracesQueryResult`
  - `GetTraceDetails` -> `TraceDetailsResult`
  - `ExportTracesNow` -> `TracesExportedResult`
  - `SetTracingConfig` -> `TracingConfigResult`
- **Scripting Lifecycle Hook Events**:
  - `TraceSpanRecorded`: Dispatched whenever a span completes with duration and attributes.
  - `OtlpExportFailed`: Dispatched on OTLP collector network timeouts or HTTP 5xx responses.
  - `TraceSamplingSurge`: Dispatched when buffer drops exceed configured thresholds.
- **CLI Commands**:
  - `craft trace status [--json]`: Inspect buffer capacity, sample ratio, and OTLP collector endpoint.
  - `craft trace list [--service <s>] [-d <ms>] [-l <limit>]`: Query recent recorded spans.
  - `craft trace get <trace-id>`: Render complete ASCII causal span tree with attributes and events.
  - `craft trace export [--json]`: Force immediate batch push to OTLP collector.
  - `craft trace config [--enabled <bool>] [--sample-ratio <f64>] [--otlp-endpoint <url>]`: Hot-reconfigure tracing engine.
  - Subcommand aliases: `craft tracing ...` and `craft otel ...`.
- **ModalX Centered TUI**: Integrated in `craft manage` -> `Tools` -> `Distributed Tracing & OpenTelemetry (OTel)`.

---

## 10. Autonomous Kernel-Bypassed DPDK Packet Processing & NUMA-Aware Memory Pinning

### 10.1. Hardware Architecture & NUMA Topology Discovery
- **NUMA Subsystem Discovery**:
  - Direct Linux `sysfs` interrogation via `/sys/devices/system/node/nodeX/` and `/sys/devices/system/cpu/`.
  - Fallback topology discovery via `libc::get_nprocs()` and standard sysinfo for single-socket UMA environments.
  - Per-node CPU affinity lists, total/free physical memory accounting, and 2MB/1GB hugepage pool status.
  - Distances matrix calculation (`/sys/devices/system/node/nodeX/distance`) reflecting interconnect NUMA penalty.
- **CPU Range Syntax**: Universal support for range notations (`"2-5"`, `"0,2,4,6"`, `"1-3,7,9-11"`).

### 10.2. Lock-Free SPSC/MPMC DPDK Packet Ring Buffer
- **False Sharing Elimination**:
  - Ring buffer indices (`head`, `tail`) annotated with `#[repr(align(64))]` cache-line padding to prevent L1/L2 thrashing between reader and writer cores on x86_64.
- **Lock-Free Concurrency**:
  - Single-Producer Single-Consumer (SPSC) lock-free atomic pointer exchange using `Acquire`/`Release` memory ordering.
  - Multi-Producer Multi-Consumer (MPMC) atomic CAS reserve-and-commit protocol for multi-core packet ingestion.
  - Zero heap allocation on steady-state RX/TX bursts: reusable `PacketDescriptor` ring with preallocated byte buffers.

### 10.3. Sub-Microsecond Welford Jitter Engine
- **Online Running Variance**:
  - Real-time packet inter-arrival jitter calculation using Welford's single-pass numerical stability algorithm.
  - Quantile estimators: P50, P90, and P99 jitter percentiles tracking microsecond spikes.
  - Dynamic 10-bucket sparkline distribution generator (` ▂▃▄▅▆▇█`) for terminal visualization.

### 10.4. DPDK Poll-Mode Hardware Driver Integration
- **Zero-Copy Kernel Bypass**:
  - Poll-mode driver integration checking for `/sys/bus/pci/drivers/vfio-pci` and `/sys/class/uio`.
  - Synthetic packet stream generator for high-throughput zero-copy loopback benchmarking (1M+ PPS).
  - Graceful userspace fallback: when vfio-pci hardware is unavailable, packet pipelines operate seamlessly over cache-aligned atomic memory rings.

### 10.5. Core Pinning & Zero-Jitter Scheduling
- **Process and Thread Affinity**:
  - Linux `sched_setaffinity` syscall invocation via `libc` for dedicated server process PIDs and worker threads.
  - Inter-process file locking via `numa.lock` ensuring serialized persistence to `numa.toml`.
- **NUMA Allocation Policies**:
  - `Local`: Allocate strictly from node hosting the pinned CPU cores.
  - `Interleave`: Round-robin page allocation across all active NUMA nodes.
  - `Preferred(node)`: Prioritize target NUMA node, falling back to adjacent nodes if depleted.
  - `Bind`: Strictly restrict allocation to specified node without fallback.
- **Kernel Boot Isolation Parameters (`generate_boot_params`)**:
  - Automated parameter generation for `/etc/default/grub`:
    - `isolcpus=<cores>`: Removes cores from standard CFS kernel scheduler queue.
    - `nohz_full=<cores>`: Stops kernel timer ticks on isolated cores when a single task runs.
    - `rcu_nocbs=<cores>`: Offloads RCU callback processing to housekeeping cores.
    - `default_hugepagesz=1G hugepagesz=1G hugepages=<N>`: Allocates contiguous 1GB pages.

### 10.6. Daemon IPC, Scripting Hooks & CLI Interface
- **Daemon IPC Protocols**:
  - `GetNumaStatus` -> `NumaStatusResult`
  - `PinServerCores` -> `PinServerCoresResult`
  - `SetNumaPolicy` -> `NumaPolicyResult`
  - `BenchmarkNumaMemory` -> `NumaBenchmarkResult`
  - `GetDpdkStatus` -> `DpdkStatusResult`
- **Prometheus Telemetry Metrics**:
  - `craft_dpdk_rx_packets_total`, `craft_dpdk_tx_packets_total`
  - `craft_dpdk_avg_jitter_microseconds`, `craft_dpdk_p99_jitter_microseconds`
  - `craft_dpdk_throughput_pps`, `craft_dpdk_throughput_mb_per_sec`
  - `craft_numa_nodes_total`, `craft_numa_total_memory_bytes`, `craft_numa_pinned_servers_total`
- **Scripting Lifecycle Hook Events**:
  - `NumaMigrationTriggered`: Dispatched when automatic server memory migration triggers.
  - `DpdkPacketFloodAlert`: Dispatched when packet ingress surges above configured threshold.
  - `CorePinningAdjusted`: Dispatched when CPU core allocations or NUMA policies are altered.
- **CLI Commands**:
  - `craft numa status [--json]`: Inspect topology, memory per node, hugepages, and driver state.
  - `craft numa pin <server> --cpus <range> [--node <n>] [--policy <pol>] [--json]`: Pin server process to CPU cores and memory node.
  - `craft numa policy <server> --policy <pol> [--node <n>] [--json]`: Update server NUMA policy.
  - `craft numa bench [--node 0] [--size-mb 16] [--json]`: Measure local vs remote cross-socket memory throughput and ring burst rate.
  - `craft numa boot-args --cores <range> [--hugepages-1g <n>] [--json]`: Generate kernel boot parameters.
  - Subcommand aliases: `craft dpdk ...` and `craft pinning ...`.
- **ModalX Centered TUI**: Integrated in `craft manage` -> `Tools` -> `Kernel-Bypassed DPDK & NUMA Memory Pinning`.

---

## 11. Zero-Downtime TCP Connection Splicing & Dynamic BGP Anycast Steering

Craft incorporates a zero-downtime TCP connection splicing engine and BGP routing automation subsystem enabling transparent live game server migration across cluster nodes with zero player disconnection timeouts.

### 11.1. In-Memory Freeze Buffer & Sequence Tracking (`ConnectionSplicer`)
- **Connection Splicing Lifecycle**:
  - When live migration enters `Freezing`, the splicer intercepts the upstream and downstream TCP socket streams for each connected player.
  - Active sessions are represented by `PlayerSocketHandoffFrame`:
    - `player_uuid`: Unique player identifier (UUIDv4).
    - `username`: Player display name.
    - `client_addr`: Client IP:port socket address.
    - `server_addr`: Bound server socket address.
    - `inbound_seq_num`: Client-to-server TCP sequence number.
    - `outbound_seq_num`: Server-to-client TCP sequence number.
    - `window_scale`: Negotiated TCP window scaling factor.
    - `tls_master_key`: Optional decrypted session key for encrypted proxies.
- **Freeze-Window Buffering**:
  - While the source process suspends and memory state transfers, incoming client packets are queued in an in-memory buffer (`VecDeque<u8>`).
  - No TCP RST or FIN packets are transmitted to the client; the TCP window is optionally advertised as zero to pause client transmission without tearing down the socket.
- **Socket Drain & Replay**:
  - Upon target node activation, buffered packets are drained (`drain_buffer`) and replayed into the target socket stream with sequence continuity, preventing client disconnects.

### 11.2. Binary Migration Wire Protocol Framing
- **Magic Identifier**: `CRAFT_MIGRATION_MAGIC: [u8; 4] = [0x43, 0x4D, 0x49, 0x47]` (`CMIG`).
- **Wire Message Types**:
  - `Handshake { migration_id, server_name, source_node, target_node, total_memory_bytes }`
  - `PreCopyChunk { migration_id, round, chunk_index, total_chunks, chunk: MemoryPageChunk }`
  - `FreezeNotice { migration_id, freeze_sla_ms }`
  - `StateManifest { migration_id, manifest: ServerCheckpointManifest }`
  - `ResumeAck { migration_id, success, error }`
  - `AbortNotice { migration_id, reason }`
- **Encoding & Validation**:
  - `encode_migration_message(&msg) -> Result<Vec<u8>, CraftError>` writes 4-byte magic, 4-byte big-endian payload length, and serialized payload.
  - `decode_migration_message(&bytes) -> Result<MigrationWireMessage, CraftError>` validates magic header and verifies payload length before deserialization.

### 11.3. Autonomous BGP Anycast Steering Engine (`AnycastBgpEngine`)
- **Anycast BGP Mechanics**:
  - Game server IP prefixes (e.g., `/32` IPv4 or `/128` IPv6 Anycast VIPs) are announced to upstream BGP routers from multiple edge nodes.
  - Traffic routes via ECMP (Equal-Cost Multi-Path) to the closest healthy node.
  - During live migration, traffic steering is achieved by adjusting BGP attributes or switching announcements:
    - Prepend AS paths (`as_path_prepend: 3`) on the draining source node to gracefully direct new connections to the target.
    - Announce the VIP on the target node.
    - Withdraw the VIP from the source node once socket handoff is finalized.
- **Multi-Daemon Configuration Synthesis**:
  - `generate_bird_config`: Generates BIRD 2.x protocol bgp stanza with `import all`, `export filter`, and local/remote AS definitions.
  - `generate_frr_config`: Generates FRRouting `router bgp` and `address-family ipv4/ipv6 unicast` configuration.
  - `generate_exabgp_config`: Generates ExaBGP neighbor definition with process runner.
- **Dynamic Route Health Evaluation (`evaluate_route_health`)**:
  - Continuously monitors node health score (0-100), MSPT, and packet loss.
  - If health score drops below threshold (default 60), the engine recommends route withdrawal or AS path prepending to prevent traffic blackholing.

---

## 12. Autonomous eBPF Kernel Observability, Zero-Overhead Syscall Profiling & Deep JVM GC Telemetry

Craft delivers kernel-level non-invasive continuous profiling, off-heap socket buffer pressure evaluation, and deep JVM GC runtime introspection:

### 12.1. Pure-Rust Tracepoint & Syscall Profiling (`EbpfProbeEngine`)
- **Zero-Overhead Syscall Interception**:
  - Intercepts essential game server kernel tracepoints: `sys_read`, `sys_write`, `sys_futex`, `sys_epoll_wait`.
  - Measures precise kernel entry and exit durations to calculate moving average latencies without JVM stop-the-world sampling jitter.
  - In-memory bounded ring buffer (10,000 records) discards oldest entries on saturation, guaranteeing constant-bounded overhead (<0.5% CPU at 20.0 TPS).
- **Advisory File-Locked Registry**:
  - Probes are tracked in `EbpfRegistry` and serialized to `~/.craft/ebpf/probes.toml` under `ebpf.lock` advisory lock protection.

### 12.2. Off-Heap Socket Buffer Telemetry (`SocketBufferTelemetry`)
- **Kernel Socket Queue Depth**:
  - Monitors TCP socket receive and transmit buffers (`rx_queue_bytes`, `tx_queue_bytes`) against allocated socket buffers (`so_rcvbuf_bytes`, `so_sndbuf_bytes`).
  - Accurately identifies network congestion before application-level socket timeouts occur.
- **Pressure Level Classification**:
  - `Pristine`: RX/TX depth < 40%.
  - `Moderate`: RX/TX depth between 40% and 70%.
  - `Elevated`: RX/TX depth between 70% and 85%.
  - `Critical`: RX/TX depth > 85%, indicating imminent TCP window stalls and network thread blocking.

### 12.3. Hierarchical Stack Flame Graph Generation (`FlameGraphBuilder`)
- **Collapsed Stack Ingestion**:
  - Accepts standard folded stack strings (`Server thread;MinecraftServer.tick();ChunkProvider.loadChunk 100`).
  - Reconstructs a multi-level stack tree (`FlameGraphNode`) and recursively computes execution percentage shares.
- **Multi-Format Visualizers**:
  - `render_ascii`: Emits an indented ASCII branch hierarchy tree (`|--`, `` `-- ``) with percentage tags and sample weights.
  - `render_svg`: Generates a fully standalone SVG vector flame graph styled with the Catppuccin warm palette, zoomable layers, and interactive mouseover tooltips.

### 12.4. Deep JVM GC & Safepoint Telemetry
- **Garbage Collection Runtime Introspection**:
  - Ingests fine-grained GC phase events (`young_gen`, `concurrent_mark`, `remark`, `full_gc`, `mixed_gc`).
  - Correlates heap space reclaimed (`reclaimed_bytes()`) with pause duration (`pause_ms()`).
- **Safepoint Synchronization Spike Thresholding**:
  - Tracks the duration required for all threads to reach safepoints (`safepoint_sync_time_ns`).
  - Any safepoint synchronization pause exceeding 50ms is flagged as `[WARN]` and automatically triggers `JvmSafepointSpikeDetected` alerts across the event hook bus.

### 12.5. Operator Controls & TUI Integration
- **CLI Commands**:
  - `craft bpf trace <server> [-d 30] [-e all] [-r 99] [--json]`: Attach kernel tracepoint probe.
  - `craft bpf status <server> [--json]`: Inspect active probe state, socket buffer depth, and syscall latency.
  - `craft bpf flamegraph <server> [-f ascii|svg] [-o <path>] [--json]`: Render or export flame graphs.
  - `craft bpf gc <server> [-n 10] [-w] [--json]`: Inspect JVM GC events and safepoint sync pauses.
  - `craft bpf stop <server> [--probe-id <id>] [--json]`: Detach profiling probe.
  - Command aliases: `craft ebpf ...` and `craft prof ...`.
- **ModalX Centered TUI**: Integrated into `craft manage` -> `Tools` -> `Autonomous eBPF Observability & Deep JVM GC Telemetry`.



