# Craft 2.0 (Rust Edition)

> **High-performance, industry-grade Minecraft server setup, management CLI tool, and background daemon written in Rust.**

Craft can provision, run, attach to, supervise, ping, back up, containerize, and configure firewall rules for any Minecraft server software across **Java Edition** and **Bedrock Edition**.

---

## Highlights & Features

- ⚡ **Blazing Fast & Lightweight**: Single native compiled binary (<12 MB) with instant startup (<2ms) and low memory footprint (<10 MB RSS for daemon).
- 🧩 **16+ Server Software Providers**:
  - **Java Edition**: Paper, Purpur, Folia, Vanilla Java, Fabric, Spigot, Quilt, NeoForge.
  - **Bedrock Edition**: Vanilla Bedrock Dedicated Server, PocketMine-MP, NukkitX.
  - **Proxies**: Velocity, Waterfall, BungeeCord, GeyserMC, WaterdogPE.
- 🔄 **Live Background Daemon & Console Attachment**:
  - Background supervisor service with typed IPC over Unix Domain Sockets and Windows Named Pipes / TCP.
  - Interactive console attachment (`craft view` / `craft attach`) with circular log replay and raw stdin command injection.
  - Auto-start servers on system boot (`craft auto`).
- 🔎 **Multi-Source Plugin & Mod Manager**:
  - Unified search & install across **Modrinth API v2**, **PaperMC Hangar API v1**, and **PocketMine Poggit**.
- 📡 **Native Protocols & Diagnostics**:
  - Java Server List Ping (SLP) status & latency checker without external tools.
  - Bedrock RakNet UDP unconnected ping.
  - Built-in RCON client (`craft rcon`).
- 💾 **Zero-Downtime World Snapshots**:
  - RCON-synchronized safe world flushes (`save-off` -> `save-all flush` -> gzip snapshot -> `save-on`).
  - Automated retention policies and instant restore.
- 🛡️ **Network & Firewall Automation**:
  - Automated system firewall provisioning (UFW / iptables / Windows Firewall) to restrict server access to trusted IPs or proxies.
  - Windows Bedrock UWP loopback exemption manager.
- 🛠️ **Developer Scaffolding & Containerization**:
  - `craft template plugin [paper|velocity]` -> Generates modern Gradle/Kotlin project with sample listeners and VS Code debug profiles.
  - `craft template datapack` -> Scaffolds modern Minecraft datapack.
  - `craft dockerize` -> Generates production multi-stage `Dockerfile` and `docker-compose.yml`.

---

## Installation & Building

### Requirements
- [Rust & Cargo](https://rustup.rs/) (version 1.80+)

### Building from Source
```bash
git clone https://github.com/OguzhanUmutlu/craft.git
cd craft
cargo build --release
```
The optimized executable will be located at `target/release/craft`.

You can also use the bundled wrapper scripts in `bin/`:
```bash
./bin/craft --help
# or on Windows:
.\bin\craft.cmd --help
```

---

## Command Reference

### Server Provisioning & Management
```bash
# Set up a new server (downloads assets, selects Java, generates scripts, sets EULA)
craft new paper 1.21.4 my-survival --memory 4G --agree-eula

# Run a server in the background (supervised by Craft daemon)
craft run my-survival

# Run a server directly in foreground in current terminal
craft run my-survival --here

# Attach to live server console (send commands, view logs)
craft view my-survival

# Stop a server gracefully
craft stop my-survival

# Force stop a server immediately
craft stop my-survival --force

# List all servers with running status, auto-run, and paths
craft ls

# Unregister a server (optional -rf to recursively delete server folder with confirmation)
craft rm my-survival -rf

# Load an existing external server directory into Craft
craft load /path/to/server paper 1.21.4 my-existing-server

# Diagnose and self-heal a server (fix permissions, regenerate start scripts, Java runtime)
craft fix my-survival
```

### Version Manifests & Cache
```bash
# List supported software and available versions
craft ver
craft ver paper

# Update software versions from upstream APIs
craft update

# Check download cache size
craft cache

# Purge cached assets
craft cache clean --force
```

### Background Service Daemon & Auto-Start
```bash
# Control the background supervisor service
craft service start
craft service stop
craft service restart
craft service status

# Configure auto-run on system boot
craft auto how
craft auto add my-survival
craft auto rm my-survival
```

### Plugins & Addons
```bash
# Search plugins across Modrinth, Hangar, and Poggit
craft plugin search essentials

# Install a plugin directly into server's plugins/ directory
craft plugin install <modrinth-project-id> my-survival
```

### Backups & RCON
```bash
# Create an atomic, compressed world snapshot (.tar.gz)
craft backup create my-survival

# List snapshots for a server
craft backup list my-survival

# Restore a snapshot
craft backup restore my-survival /path/to/backup.tar.gz

# Send remote console command via RCON
craft rcon my-survival "whitelist add Steve"
```

### Network Diagnostics & Security
```bash
# Ping a Java Minecraft server (SLP)
craft ping mc.example.com:25565

# Ping a Bedrock server (RakNet UDP)
craft ping bedrock.example.com:19132 --bedrock

# Restrict server port to a specific IP or proxy
craft firewall allow my-survival 192.168.1.50

# Check/enable Windows Bedrock UWP loopback exemption
craft loopback status
craft loopback enable
```

### Remote SSH Orchestration & Cross-Platform Bootstrapping
```bash
# Add a remote host (supports user@host or user@host:port, key or password auth)
craft remote add my-vps root@192.168.1.100:2222 --key ~/.ssh/id_ed25519

# List all configured remote hosts
craft remote ls

# Test SSH connection and probe remote host OS/environment
craft remote test my-vps

# Tri-platform automated bootstrapping (installs Java 21, Craft daemon service, firewall)
# Works seamlessly on Linux (systemd), macOS (launchd), and Windows (PowerShell/OpenSSH)
craft remote setup my-vps

# Synchronize local server files to remote host over SFTP
craft remote sync my-vps ./my-local-server /opt/craft/servers/my-local-server

# Manage remote servers seamlessly using the global --remote flag
craft run my-survival --remote my-vps
craft ls --remote my-vps
craft stop my-survival --remote my-vps
craft view my-survival --remote my-vps    # Full interactive PTY with Ctrl+B -> D detach

# Remove remote host configuration
craft remote rm my-vps
```

### Developer Templates & Docker
```bash
# Scaffold Paper / Velocity plugin project with Gradle & VS Code config
craft template plugin MyPlugin --platform paper

# Scaffold a Minecraft datapack
craft template datapack MyDatapack

# Generate optimized Dockerfile and docker-compose.yml for a server
craft dockerize my-survival
```

---

## Workspace Crate Architecture

```
craft/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── craft-core/             # Core models, TOML/JSON registry, remotes.toml, Java bytecode detector
│   ├── craft-providers/        # Server software providers (Paper, Purpur, Mojang, Fabric, BDS, PMMP, etc.)
│   ├── craft-daemon/           # Background process supervisor, IPC, log ring-buffer
│   ├── craft-net/              # Native SLP ping, RakNet ping, RCON client, firewall
│   ├── craft-plugins/          # Modrinth, Hangar, and Poggit clients & installers
│   ├── craft-backup/           # Zero-downtime compressed world snapshots & restore
│   ├── craft-remote/           # Async SSH orchestration, SFTP sync, PTY console, tri-platform bootstrap
│   └── craft-cli/              # Flagship CLI binary and command routing
└── data/                       # Embedded fallback version manifests
```

---

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
