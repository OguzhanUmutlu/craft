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

