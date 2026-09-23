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

