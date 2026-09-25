use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "craft")]
#[command(author = "OguzhanUmutlu")]
#[command(version)]
#[command(about = "High-performance dedicated game server management CLI and daemon", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Open the interactive Craft Server Manager Dashboard
    #[command(alias = "dashboard", alias = "ui", alias = "tui")]
    Manage {
        /// Remote node presentation mode (used when streamed from a remote host)
        #[arg(long = "remote-node", alias = "remote-ui", alias = "node")]
        remote_node: Option<String>,
        /// Target remote host alias (to connect and stream remote dashboard from local CLI)
        #[arg(long = "remote")]
        remote: Option<String>,
    },

    /// Set up a new dedicated game server (interactive wizard if software omitted)
    #[command(alias = "create", alias = "init")]
    New {
        /// Server name (or omit to launch the interactive setup wizard)
        #[arg(default_value = "")]
        name: String,
        /// Explicit server name option
        #[arg(long = "name")]
        name_opt: Option<String>,
        /// Server software (paper, purpur, folia, fabric, quilt, neoforge, velocity, etc.)
        #[arg(value_name = "SOFTWARE")]
        software: Option<String>,
        /// Software version (or "latest")
        #[arg(value_name = "VERSION")]
        version: Option<String>,
        /// Explicit server software option
        #[arg(short = 's', long = "software-id", alias = "software")]
        software_opt: Option<String>,
        /// Explicit software version option
        #[arg(short = 'v', long = "version-id", alias = "version")]
        version_opt: Option<String>,
        /// Server network port (default: 25565)
        #[arg(short = 'p', long = "port")]
        port: Option<u16>,
        /// Custom destination path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Allocate memory (e.g. 2G, 4G, 8G)
        #[arg(short = 'm', long = "memory")]
        memory: Option<String>,
        /// Automatic EULA acceptance
        #[arg(long)]
        agree_eula: bool,
        /// Temporary server (deleted on exit)
        #[arg(long)]
        tmp: bool,
        /// Do not automatically start the server after creation
        #[arg(long)]
        no_start: bool,
        /// Non-interactive mode (use defaults without prompting)
        #[arg(short = 'y', long = "yes", alias = "non-interactive")]
        yes: bool,
        /// Use Aikar's optimized JVM garbage collection flags (G1GC)
        #[arg(long)]
        aikar: bool,
        /// Use Z Garbage Collector (ZGC) low-latency flags
        #[arg(long)]
        zgc: bool,
        /// Use Shenandoah low-pause garbage collector
        #[arg(long)]
        shenandoah: bool,
        /// Custom JVM flags (e.g. "-XX:+UseStringDeduplication")
        #[arg(long, value_delimiter = ' ')]
        jvm_flags: Option<Vec<String>>,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
        /// Custom server runtime type (binary, lua, script)
        #[arg(long = "runtime")]
        runtime: Option<String>,
        /// Custom server executable or script path
        #[arg(long = "exec")]
        exec: Option<String>,
    },

    /// Run an existing server
    #[command(alias = "start")]
    Run {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Run in foreground in current terminal session
        #[arg(short = 'H', long = "here")]
        here: bool,
        /// Run in background daemon supervisor mode (default)
        #[arg(short = 'd', long = "daemon")]
        daemon: bool,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// Stop a running server
    Stop {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Force kill process immediately
        #[arg(short, long)]
        force: bool,
        /// Stop all running servers
        #[arg(short = 'a', long = "all")]
        all: bool,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// Restart a running server
    Restart {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Force kill process before restarting
        #[arg(short, long)]
        force: bool,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// View / attach to live console of a running server
    #[command(alias = "attach", alias = "console")]
    View {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// View, stream, or export server logs and diagnostics
    #[command(alias = "logs")]
    Log {
        #[command(subcommand)]
        action: Option<LogCommands>,
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
    },

    /// AI-driven workload forecasting, predictive auto-scaling, and cost optimization
    #[command(alias = "predict", alias = "costs")]
    Forecast {
        #[command(subcommand)]
        action: Option<ForecastCommands>,
        /// Target server name
        #[arg(default_value = "")]
        server: String,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// List all registered servers and their status
    #[command(alias = "list", alias = "ps")]
    Ls {
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// Unregister an existing server
    #[command(alias = "delete")]
    Rm {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Delete server files recursively with confirmation
        #[arg(short, long)]
        rf: bool,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
    },

    /// Load an existing server folder into Craft
    #[command(alias = "import")]
    Load {
        /// Path to server directory
        path: PathBuf,
        /// Server software
        software: String,
        /// Server version
        version: String,
        /// Custom name
        #[arg(default_value = "")]
        name: String,
    },

    /// List available server software or versions
    Ver {
        /// Server software name
        software: Option<String>,
    },

    /// Update server software version lists
    Update {
        /// Specific softwares to update
        softwares: Vec<String>,
    },

    /// Manage downloaded asset cache
    Cache {
        #[command(subcommand)]
        action: Option<CacheCommands>,
    },

    /// Manage, inspect, update, and build the centralized server software version catalog
    Catalog {
        #[command(subcommand)]
        action: Option<CatalogCommands>,
    },

    /// Manage background service daemon
    #[command(alias = "daemon")]
    Service {
        #[command(subcommand)]
        action: Option<ServiceCommands>,
    },

    /// Manage auto-starting servers
    Auto {
        #[command(subcommand)]
        action: AutoCommands,
    },

    /// Fix or repair an existing server (scripts, permissions, Java version)
    Fix {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
    },

    /// Search and install plugins (non-vanilla servers)
    #[command(alias = "plugins")]
    Plugin {
        #[command(subcommand)]
        action: Option<PluginCommands>,
    },

    /// Search and install mods (modded servers e.g. Fabric, Quilt, NeoForge)
    #[command(alias = "mods")]
    Mod {
        #[command(subcommand)]
        action: Option<ModCommands>,
    },

    /// Search and install datapacks (Minecraft Java worlds)
    #[command(alias = "datapacks")]
    Datapack {
        #[command(subcommand)]
        action: Option<DatapackCommands>,
    },

    /// Ping a server (Minecraft Java/Bedrock, Steam A2S, or auto-detect)
    Ping {
        /// Target address (e.g. localhost:25565, 127.0.0.1:8211, or server name)
        #[arg(default_value = "")]
        target: String,
        /// Bedrock server ping
        #[arg(long)]
        bedrock: bool,
        /// Valve / Steam A2S server ping (Palworld, Valheim, etc.)
        #[arg(long)]
        a2s: bool,
    },

    /// Send an RCON command to a running server
    Rcon {
        /// Server name or host:port
        server: String,
        /// Password
        #[arg(short, long)]
        password: Option<String>,
        /// Command to execute
        command: String,
    },

    /// Create, list, or restore world backups
    Backup {
        #[command(subcommand)]
        action: Option<BackupCommands>,
    },

    /// Configure firewall rules for a server
    Firewall {
        #[command(subcommand)]
        action: FirewallCommands,
    },

    /// Manage Windows Bedrock loopback exemption
    Loopback {
        #[command(subcommand)]
        action: Option<LoopbackCommands>,
    },

    /// Scaffold development templates (plugins, datapacks, mods)
    Template {
        #[command(subcommand)]
        action: TemplateCommands,
    },

    /// Generate Dockerfile and docker-compose.yml for a server
    Dockerize {
        /// Server name
        server: String,
    },

    /// Manage remote hosts and servers over SSH
    Remote {
        #[command(subcommand)]
        action: Option<RemoteCommands>,
    },

    /// Migrate a server to a remote host with atomic snapshot or perform zero-downtime live migration
    Migrate {
        #[command(subcommand)]
        action: Option<MigrateCommands>,
        /// Local server name to migrate (for cold migration)
        #[arg(default_value = "")]
        server: String,
        /// Target remote host alias (e.g. --to my-vps)
        #[arg(long, default_value = "")]
        to: String,
        /// Optional target server name on the remote host (defaults to source name)
        #[arg(long)]
        remote_name: Option<String>,
        /// Optional port to rebind the migrated server on the remote host
        #[arg(long)]
        remote_port: Option<u16>,
        /// Move the local source server to non-destructive trash bin after successful migration
        #[arg(long)]
        trash_source: bool,
        /// Automatically start the migrated server on the remote host supervisor
        #[arg(long)]
        start: bool,
    },

    /// Manage multi-server clusters, routing proxies, and topological startup ordering
    Cluster {
        #[command(subcommand)]
        action: Option<ClusterCommands>,
    },

    /// Deploy and manage Craft container stack with Docker Compose
    Deploy {
        #[command(subcommand)]
        action: Option<DeployCommands>,
    },

    /// View, edit, or search server.properties configuration keys
    #[command(alias = "config", alias = "properties")]
    Prop {
        /// Server name
        server: String,
        #[command(subcommand)]
        action: Option<crate::commands::prop::PropAction>,
    },

    /// Manage, inspect, and switch worlds, saves, dimensions, and player data
    #[command(alias = "worlds", alias = "save", alias = "saves")]
    World {
        /// Server name
        server: String,
        #[command(subcommand)]
        action: Option<crate::commands::world::WorldAction>,
    },

    /// Developer utilities for linking development JARs, JDWP debugging, and hot reloading
    #[command(alias = "developer")]
    Dev {
        /// Server name
        server: String,
        #[command(subcommand)]
        action: crate::commands::dev::DevAction,
    },

    /// Manage recoverable deleted servers and files in the trash bin
    Trash {
        #[command(subcommand)]
        action: Option<crate::commands::trash::TrashAction>,
    },

    /// Execute a Lua script using the built-in Craft Lua runtime
    Lua {
        /// Path to the Lua script to execute
        script: PathBuf,
        /// Arguments passed to the Lua script
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Manage server software definitions and .zip packages
    #[command(name = "software", alias = "sw")]
    Software {
        #[command(subcommand)]
        action: SoftwareCommands,
    },

    /// Manage event-driven notification webhooks (Discord, Slack, GenericJson)
    #[command(name = "webhook", alias = "webhooks")]
    Webhook {
        #[command(subcommand)]
        action: WebhookCommands,
    },

    /// Manage remote WebSocket console gateway and Prometheus telemetry
    #[command(name = "gateway", alias = "gw")]
    Gateway {
        #[command(subcommand)]
        action: GatewayCommands,
    },

    /// Dynamically tune JVM parameters, GC strategy, and memory profiles for a server
    #[command(name = "optimize", alias = "tune")]
    Optimize {
        /// Server name
        name: String,
        /// Optimization profile (conservative, balanced, aggressive)
        #[arg(short, long, default_value = "balanced")]
        profile: String,
        /// Automatically apply recommended settings to server configuration
        #[arg(long)]
        apply: bool,
    },

    /// Universal modpack distribution engine (Modrinth .mrpack & CurseForge)
    #[command(name = "modpack", alias = "pack")]
    Modpack {
        #[command(subcommand)]
        action: ModpackCommands,
    },

    /// Manage auto-scaling, packet-triggered wake-up, and idle hibernation
    #[command(name = "autoscale", alias = "scale")]
    Autoscale {
        /// Server name (omit when listing status across all servers)
        #[arg(default_value = "")]
        name: String,
        /// Enable idle hibernation
        #[arg(long)]
        enable: bool,
        /// Disable idle hibernation
        #[arg(long)]
        disable: bool,
        /// Set idle timeout in minutes before hibernating
        #[arg(long)]
        idle_timeout: Option<u64>,
        /// Sleeping server MOTD
        #[arg(long)]
        motd: Option<String>,
        /// Display current autoscale and hibernation status
        #[arg(long)]
        status: bool,
    },

    /// Manually hibernate a server into SleepProxy or wake it up
    #[command(name = "hibernate", alias = "sleep")]
    Hibernate {
        /// Server name
        name: String,
        /// Wake the server up instead of hibernating
        #[arg(long)]
        wake: bool,
    },

    /// Manage multi-tenant users, roles, and server access permissions
    #[command(name = "user", alias = "users", alias = "rbac")]
    User {
        #[command(subcommand)]
        action: UserCommands,
    },

    /// Inspect and cryptographically verify the append-only audit ledger
    #[command(name = "audit")]
    Audit {
        #[command(subcommand)]
        action: AuditCommands,
    },

    /// Automated disaster recovery playbooks, simulation sandboxes, and failover
    #[command(name = "dr")]
    Dr {
        #[command(subcommand)]
        action: DrCommands,
    },

    /// Manage multi-cloud storage mesh targets, quorums, and synchronization
    #[command(name = "mesh")]
    Mesh {
        #[command(subcommand)]
        action: MeshCommands,
    },

    /// Autonomous operational intelligence, anomaly detection, and predictive diagnostics
    #[command(name = "ai", alias = "autopilot", alias = "diag")]
    Ai {
        #[command(subcommand)]
        action: AiCommands,
    },

    /// Global edge mesh, multi-region server sync, and player traffic routing
    #[command(name = "edge", alias = "geo", alias = "mesh-routing")]
    Edge {
        #[command(subcommand)]
        action: EdgeCommands,
    },

    /// Execute and manage Lua automation scripts and server lifecycle hooks
    #[command(name = "script", alias = "scripts")]
    Script {
        #[command(subcommand)]
        action: ScriptCommands,
    },

    /// Real-time tick profiling, Netty packet inspection, and latency micro-histograms
    #[command(name = "profile", alias = "perf", alias = "telemetry")]
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },

    /// Zero-trust inter-server microsegmentation, eBPF packet filtering, and WireGuard overlay mesh
    #[command(name = "sdn", alias = "overlay", alias = "sdn-mesh")]
    Sdn {
        #[command(subcommand)]
        action: SdnCommands,
    },

    /// Distributed fault-tolerant consensus, Raft clustering, and dynamic split-brain arbitration
    #[command(name = "raft", alias = "consensus")]
    Raft {
        #[command(subcommand)]
        action: RaftCommands,
    },

    /// Linux cgroups v2 resource quotas, CPU/memory throttling, and fair-share scheduling
    #[command(name = "quota", alias = "cgroup", alias = "limits")]
    Quota {
        #[command(subcommand)]
        action: QuotaCommands,
    },

    /// Distributed real-time tracing, OpenTelemetry (OTel) export, and W3C trace context propagation
    #[command(name = "trace", alias = "tracing", alias = "otel")]
    Trace {
        #[command(subcommand)]
        action: TraceCommands,
    },

    /// Hardware-accelerated Minecraft Anvil (.mca) storage engine, zero-copy packet pipelines, and io_uring submissions
    #[command(name = "anvil", alias = "chunk", alias = "mca")]
    Anvil {
        #[command(subcommand)]
        action: AnvilCommands,
    },

    /// Autonomous kernel-bypassed DPDK packet processing, NUMA-aware memory pinning, and zero-jitter scheduling
    #[command(name = "numa", alias = "dpdk", alias = "pinning")]
    Numa {
        #[command(subcommand)]
        action: NumaCommands,
    },

    /// Global Anycast BGP route announcements, session continuity, and multi-cloud steering
    #[command(name = "anycast", alias = "bgp", alias = "route")]
    Anycast {
        #[command(subcommand)]
        action: AnycastCommands,
    },

    /// Autonomous eBPF kernel observability, zero-overhead syscall profiling & deep JVM GC telemetry
    #[command(name = "bpf", alias = "ebpf", alias = "prof")]
    Bpf {
        #[command(subcommand)]
        action: BpfCommands,
    },

    /// Immutable cryptographic supply chain verification, hermetic build isolation & reproducible artifact signing
    #[command(name = "attest", alias = "verify", alias = "provenance", alias = "supply-chain")]
    Attest {
        #[command(subcommand)]
        action: AttestCommands,
    },
    /// Autonomous quantum-resistant cryptographic transition, ML-KEM key exchange & state machine post-quantum hardening
    #[command(name = "pqc", alias = "post-quantum", alias = "quantum", alias = "mlkem")]
    Pqc {
        #[command(subcommand)]
        action: Option<PqcCommands>,
    },
    /// Hardware Security Module (HSM) Integration, PKCS#11 Enclave Attestation & Zero-Knowledge Cluster Membership
    #[command(name = "hsm", alias = "pkcs11", alias = "tpm", alias = "enclave", alias = "zkp")]
    Hsm {
        #[command(subcommand)]
        action: Option<HsmCommands>,
    },
    /// Autonomous Memory Compaction, Transparent Hugepage Defragmentation & Kernel page_pool Offloading
    #[command(name = "memory", alias = "compaction", alias = "hugepages", alias = "pagepool", alias = "thp")]
    Memory {
        #[command(subcommand)]
        action: Option<MemoryCommands>,
    },
    /// Autonomous Self-Healing eBPF XDP Firewall, Anti-DDoS Mitigation & State-Machine Flow Tracking
    #[command(name = "xdp", alias = "xdp-firewall", alias = "antiddos", alias = "ddos", alias = "bpf-xdp")]
    Xdp {
        #[command(subcommand)]
        action: Option<XdpCommands>,
    },
    /// Autonomous Dynamic Binary Instrumentation, Hardware Performance Counters & Cache Miss Profiling
    #[command(name = "pmu", alias = "hw-counters", alias = "cache-profile", alias = "counters")]
    Pmu {
        #[command(subcommand)]
        action: Option<PmuCommands>,
    },
    /// Autonomous Memory-Mapped Persistent Shared Memory (POSIX shm), Zero-Copy IPC & High-Speed Ring Bus
    #[command(name = "shm", alias = "shared-memory", alias = "shmem", alias = "ipc-ring")]
    Shm {
        #[command(subcommand)]
        action: Option<ShmCommands>,
    },
    /// Autonomous Dynamic Binary Rewriting, Trampoline Patching & Zero-Downtime Hot Code Replacement
    #[command(name = "patch", alias = "hot-swap", alias = "hotpatch", alias = "live-patch")]
    Patch {
        #[command(subcommand)]
        action: Option<PatchCommands>,
    },
    /// Autonomous MicroVM Sandboxing, Lightweight Firecracker/KVM Isolation & Sub-50ms Cold Starts
    #[command(name = "vm", alias = "microvm", alias = "firecracker", alias = "kvm")]
    Vm {
        #[command(subcommand)]
        action: Option<VmCommands>,
    },
    /// Autonomous AI-Guided Static Analysis, Real-Time Memory Leak Detection & Automated Core Dump Triaging
    #[command(name = "crash", alias = "triage", alias = "core-dump", alias = "leak-detect")]
    Crash {
        #[command(subcommand)]
        action: Option<CrashCommands>,
    },
    /// Autonomous RDMA Network Acceleration, InfiniBand/RoCE Direct Memory Offloading & Sub-Microsecond Inter-Server Fabric
    #[command(name = "rdma", alias = "infiniband", alias = "roce", alias = "verbs")]
    Rdma {
        #[command(subcommand)]
        action: Option<RdmaCommands>,
    },
    /// Autonomous eBPF XDP Hardware Offloading, SmartNIC Acceleration & P4 Programmable Data Plane Line-Rate Switching
    #[command(name = "smartnic", alias = "p4", alias = "nic-offload")]
    SmartNic {
        #[command(subcommand)]
        action: Option<SmartNicCommands>,
    },
    /// Autonomous Distributed Inter-Server Memory Fabric, Remote Paged Compaction & Cluster NVRAM Pool
    #[command(name = "memfabric", alias = "cxl", alias = "nvram", alias = "paging")]
    MemFabric {
        #[command(subcommand)]
        action: Option<MemFabricCommands>,
    },
    /// Autonomous Zero-Copy Storage Fabrics, NVMe-oF Target & Distributed Flash Block Pool
    #[command(name = "nvme", alias = "nvmeof", alias = "fabrics", alias = "storage-pool")]
    Nvme {
        #[command(subcommand)]
        action: Option<NvmeCommands>,
    },
    /// Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh, WireGuard PQXDH & P4 Crypto Offloading
    #[command(name = "vpn", alias = "wireguard", alias = "pqxdh", alias = "mesh-vpn")]
    Vpn {
        #[command(subcommand)]
        action: Option<VpnCommands>,
    },
    /// Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus, Zero-Knowledge State Attestation & BFT Cluster Quorum
    #[command(name = "bft", alias = "hotstuff", alias = "pbft", alias = "byzantine", alias = "quorum")]
    Bft {
        #[command(subcommand)]
        action: Option<BftCommands>,
    },
    /// Autonomous eBPF-Driven Live Game Kernel Tracing, Micro-Stall Schedulers & Real-Time Kernel Jitter Elimination
    #[command(name = "jitter", alias = "sched", alias = "microstall", alias = "realtime", alias = "sched-trace")]
    Jitter {
        #[command(subcommand)]
        action: Option<JitterCommands>,
    },
    /// Autonomous Neuromorphic AI Tick Scheduling, Spike-Driven Game Loop Inference & Microsecond Latency Forecasting
    #[command(name = "neuromorphic", alias = "snn", alias = "lif", alias = "spike", alias = "neuromorph")]
    Neuromorphic {
        #[command(subcommand)]
        action: Option<NeuromorphicCommands>,
    },
    /// Autonomous Optical Network Switching, Photonic Interconnects & Line-Rate Nanosecond Waveguide Routing
    #[command(name = "optical", alias = "ocs", alias = "photonic", alias = "waveguide", alias = "wdm")]
    Optical {
        #[command(subcommand)]
        action: Option<OpticalCommands>,
    },
}


#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum ProfileCommands {
    /// Inspect real-time tick duration percentiles (P50/P90/P99), jitter, and health grade
    Tick {
        /// Target server name
        server: String,
        /// Rolling sampling window (number of ticks, default: 60)
        #[arg(short = 'w', long = "window", default_value = "60")]
        window: usize,
    },
    /// Inspect Netty ingress/egress packet rates, bandwidth, and burst anomalies
    Packets {
        /// Target server name
        server: String,
    },
    /// Render dynamic logarithmic latency micro-histogram and ASCII bucket distribution
    Histogram {
        /// Target server name
        server: String,
        /// Maximum column width for ASCII bar chart (default: 60)
        #[arg(short = 'w', long = "width", default_value = "60")]
        width: usize,
    },
    /// Comprehensive profiling overview including ticks, packets, and latency histogram
    Overview {
        /// Target server name
        server: String,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum ScriptCommands {
    /// Execute a standalone Lua automation script file
    Run {
        /// Path to the Lua script file (.lua)
        path: PathBuf,
        /// Contextual target server name (populates craft.server and loads server hooks)
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Maximum execution timeout in seconds (default: 10)
        #[arg(short = 't', long = "timeout", default_value = "10")]
        timeout: u64,
        /// Arguments passed to the script accessible via global 'arg' table
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Evaluate an inline Lua expression and print the formatted return value
    Eval {
        /// Lua code string to evaluate
        code: String,
        /// Contextual target server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Maximum execution timeout in seconds (default: 10)
        #[arg(short = 't', long = "timeout", default_value = "10")]
        timeout: u64,
    },
    /// List registered global lifecycle hooks (~/.craft/hooks/) and per-server hooks
    List {
        /// Filter by target server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
    },
    /// Simulate a server lifecycle event to verify hook scripts without real downtime
    Test {
        /// Lifecycle event name (e.g. on_server_start, on_server_crash, on_backup_complete)
        event: String,
        /// Target server name for context
        #[arg(short = 's', long = "server")]
        server: Option<String>,
    },
    /// Scaffold a starter template for a global or per-server lifecycle hook script
    New {
        /// Hook or event name (e.g. on_server_crash, on_server_start, hooks)
        hook_name: String,
        /// Target server name (creates in server directory instead of global ~/.craft/hooks/)
        #[arg(short = 's', long = "server")]
        server: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum AiCommands {
    /// Display operational intelligence and predictive health status
    Status {
        /// Optional server name filter
        server: Option<String>,
    },
    /// Run immediate deep performance diagnostic analysis
    Analyze {
        /// Target server name
        server: String,
    },
    /// Capture lightweight JVM flight recorder execution profile
    Profile {
        /// Target server name
        server: String,
        /// Profile duration in seconds
        #[arg(short, long, default_value = "30")]
        duration: u64,
    },
    /// Execute or dry-run automated performance remediation
    Remediate {
        /// Target server name
        server: String,
        /// Remediation action: cull, gc, restart
        #[arg(short, long)]
        action: String,
        /// Simulate remediation without executing actions
        #[arg(long)]
        dry_run: bool,
    },
    /// Manage Autopilot operational intelligence policies
    Policy {
        /// Target server name
        server: String,
        /// Policy mode: advisory, autonomous, disabled
        #[arg(short, long)]
        mode: Option<String>,
        /// Enable autopilot in advisory mode
        #[arg(long)]
        enable: bool,
        /// Disable autopilot monitoring
        #[arg(long)]
        disable: bool,
        /// Warning MSPT threshold in milliseconds
        #[arg(long)]
        warn_mspt: Option<f64>,
        /// Critical MSPT threshold in milliseconds
        #[arg(long)]
        crit_mspt: Option<f64>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum EdgeCommands {
    /// Display global edge mesh topology, registered nodes, and backbone telemetry
    Status,

    /// Register a new edge ingress proxy node
    Add {
        /// Edge node unique name
        name: String,
        /// Geographic region (e.g. us-east, us-west, eu-central, ap-southeast)
        #[arg(short, long)]
        region: String,
        /// Public ingress endpoint (e.g. 198.51.100.1:25565 or edge-us.craft.internal:25565)
        #[arg(short, long)]
        endpoint: String,
        /// Traffic steering weight (1-1000)
        #[arg(short, long, default_value = "100")]
        weight: u32,
        /// Optional metadata tags
        #[arg(short, long, value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Unregister an edge proxy node
    Rm {
        /// Edge node unique name
        name: String,
    },

    /// Probe RTT latency, jitter, and packet loss across all edge nodes
    Probe {
        /// Number of probe samples per node
        #[arg(short, long, default_value = "3")]
        count: usize,
        /// Probe connection timeout in milliseconds
        #[arg(short, long, default_value = "2000")]
        timeout_ms: u64,
    },

    /// Generate and synchronize multi-region Velocity, BungeeCord, and HAProxy edge routing
    SyncRouting {
        /// Target server cluster name (or all servers if omitted)
        #[arg(short, long)]
        cluster: Option<String>,
        /// Preview generated configuration files without writing to disk
        #[arg(long)]
        dry_run: bool,
        /// Custom destination directory for generated configs
        #[arg(short, long)]
        out_dir: Option<String>,
    },

    /// Initiate or issue a cross-region player session handoff token
    Handoff {
        /// Player username
        player: String,
        /// Player unique UUID
        #[arg(long)]
        player_uuid: Option<String>,
        /// Source origin server
        #[arg(long)]
        from: String,
        /// Target destination server
        #[arg(long)]
        to: String,
        /// Source region
        #[arg(long)]
        source_region: Option<String>,
        /// Target region
        #[arg(long)]
        target_region: Option<String>,
        /// Handoff session TTL in seconds
        #[arg(long, default_value = "30")]
        ttl: u64,
    },

    /// Broadcast an authenticated cross-region chat envelope
    Chat {
        /// Chat channel name (e.g. global, staff, trade)
        channel: String,
        /// Message content
        message: String,
        /// Sender username
        #[arg(short, long)]
        sender: Option<String>,
        /// Originating region
        #[arg(short, long)]
        region: Option<String>,
    },

    /// Apply an automated latency optimization playbook to a server
    Optimize {
        /// Target server name
        server: String,
        /// Latency preset: performance, balanced, fidelity, degraded_safe
        #[arg(short, long, default_value = "balanced")]
        preset: String,
        /// Preview playbook parameters without modifying configurations
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum DrCommands {
    /// Generate or view an automated disaster recovery runbook
    Plan {
        /// Server name
        server: String,
    },
    /// Run non-destructive cold-start recovery simulation in an isolated staging sandbox
    Test {
        /// Server name
        server: String,
    },
    /// Execute automated cold-start disaster recovery failover to a target node
    Failover {
        /// Server name
        server: String,
        /// Destination remote node alias
        #[arg(short, long)]
        target: String,
        /// Automatically start the server after failover
        #[arg(long)]
        live: bool,
    },
    /// Display storage mesh deduplication metrics and disaster recovery status
    Status,
    /// Run Merkle-tree background sampling audit across storage mesh
    Verify {
        /// Server name
        server: String,
        /// Percentage of chunks to sample (e.g. 10 for 10%)
        #[arg(short, long, default_value = "10.0")]
        sample: f64,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum MeshCommands {
    /// List all configured storage mesh replication targets
    #[command(alias = "list")]
    Ls,
    /// Add a new storage mesh replication target (s3, r2, gdrive, sftp, local)
    Add {
        /// Unique target ID (e.g. r2-backup, aws-primary)
        id: String,
        /// Descriptive target name
        name: String,
        /// Target provider kind (s3, r2, gdrive, sftp, local)
        #[arg(short, long, default_value = "s3")]
        kind: String,
        /// Target bucket name or directory path
        #[arg(short, long)]
        bucket_or_path: String,
        /// Custom API endpoint URL (for Cloudflare R2, MinIO, Wasabi)
        #[arg(short, long)]
        endpoint: Option<String>,
        /// Access key ID
        #[arg(long)]
        access_key: Option<String>,
        /// Secret access key
        #[arg(long)]
        secret_key: Option<String>,
        /// Target priority (lower = higher priority)
        #[arg(short, long, default_value = "10")]
        priority: u32,
    },
    /// Remove an existing storage mesh target
    #[command(alias = "remove")]
    Rm {
        /// Target ID to remove
        id: String,
    },
    /// Synchronize deduplicated chunks for a server across all mesh targets
    Sync {
        /// Server name
        server: String,
    },
    /// Probe network reachability and latency across all storage mesh targets
    Health,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum UserCommands {
    /// Add a new user account with role and optional server scoping
    Add {
        /// Username
        username: String,
        /// Role (SuperAdmin, ServerOperator, BackupAuditor, Viewer)
        #[arg(short, long, default_value = "Viewer")]
        role: String,
        /// Password (will prompt securely if omitted)
        #[arg(short, long)]
        password: Option<String>,
        /// Comma-separated list of assigned servers (empty for all servers)
        #[arg(short, long, value_delimiter = ',')]
        servers: Vec<String>,
    },
    /// List all registered users, roles, and server assignments
    #[command(alias = "list")]
    Ls,
    /// Remove an existing user account
    #[command(alias = "remove")]
    Rm {
        /// Username to remove
        username: String,
    },
    /// Change password for a user account
    Passwd {
        /// Username
        username: String,
        /// New password (will prompt securely if omitted)
        #[arg(short, long)]
        password: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum AuditCommands {
    /// List recent audit log entries
    #[command(alias = "list")]
    Ls {
        /// Maximum number of recent entries to show (default: 50)
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    /// Cryptographically verify the HMAC hash chain of the audit ledger
    Verify,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum ModpackCommands {
    /// Inspect a Modrinth (.mrpack) or CurseForge modpack archive
    Inspect {
        /// Path to modpack archive (.mrpack or .zip)
        archive: PathBuf,
    },
    /// Install a modpack archive into an existing or new server directory
    Install {
        /// Target server name
        server: String,
        /// Path to modpack archive (.mrpack or .zip)
        archive: PathBuf,
        /// Bypass local zstd cache store
        #[arg(long)]
        no_cache: bool,
    },
    /// Package a server directory into segregated client/server distributions with CI compatibility checks
    Build {
        /// Base directory containing mods/ and config/ (defaults to current dir)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Modpack release name (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,
        /// Release version string (default: 1.0.0)
        #[arg(short, long, default_value = "1.0.0")]
        version: String,
        /// Target mod loader (fabric, forge, neoforge, quilt)
        #[arg(short, long, default_value = "fabric")]
        loader: String,
        /// Target Minecraft version (e.g. 1.20.4)
        #[arg(short, long, default_value = "1.20.4")]
        mc_version: String,
        /// Target deployment distribution (server, client, or both)
        #[arg(short, long, default_value = "both")]
        target: String,
        /// Output directory for built archives
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Compute a block-level binary delta patch between two modpack version archives
    Delta {
        /// Path to base/source modpack archive
        source: PathBuf,
        /// Path to updated/target modpack archive
        target: PathBuf,
        /// Output file path for generated delta patch (.delta)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Modpack name tag for metadata
        #[arg(long, default_value = "modpack")]
        name: String,
        /// Source version string tag
        #[arg(long, default_value = "v1")]
        src_version: String,
        /// Target version string tag
        #[arg(long, default_value = "v2")]
        target_version: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Reconstruct a target modpack archive from a base archive and binary delta patch
    Patch {
        /// Path to base/source modpack archive
        base: PathBuf,
        /// Path to binary delta patch file (.delta)
        patch: PathBuf,
        /// Path for reconstructed target archive output
        #[arg(short, long)]
        output: PathBuf,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Fast client file synchronizer: fetch or apply modpack updates via chunk streaming
    Sync {
        /// Modpack release name
        name: String,
        /// Target version to sync
        #[arg(short, long)]
        version: Option<String>,
        /// Client installation directory (defaults to current dir)
        #[arg(short, long, default_value = ".")]
        client_dir: PathBuf,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum SdnCommands {
    /// Inspect overlay mesh topology, local node status, zones, and peer connectivity
    Status {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Bring up the WireGuard overlay network interface and apply routing table
    Up {
        /// Target node name (defaults to local node)
        #[arg(short, long)]
        node: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Tear down the WireGuard overlay network interface
    Down {
        /// Target node name (defaults to local node)
        #[arg(short, long)]
        node: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Apply, reload, or inspect kernel eBPF / nftables microsegmentation policy
    Policy {
        /// Set policy action: apply, show, or reload
        #[arg(default_value = "show")]
        action: String,
        /// Default policy verdict (drop or accept)
        #[arg(long)]
        default_verdict: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List all registered WireGuard peers, isolation zones, and handshake latencies
    Peers {
        /// Filter by isolation zone (ingress-proxy, backend-world, storage-mesh, control-plane)
        #[arg(short, long)]
        zone: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Trigger cryptographic mTLS and WireGuard keypair rotation
    RotateKeys {
        /// Target node name (defaults to local node)
        #[arg(short, long)]
        node: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Audit inter-server traffic against zero-trust policy rules and dropped packets
    Audit {
        /// Filter audit events by source zone
        #[arg(long)]
        from_zone: Option<String>,
        /// Filter audit events by destination zone
        #[arg(long)]
        to_zone: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum RaftCommands {
    /// Inspect Raft consensus state, active leader, term, quorum, and cluster nodes
    Status {
        /// Raft partition group ID (defaults to all/aggregated)
        #[arg(short, long)]
        group: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Propose a replicated state mutation to the cluster leader
    Propose {
        /// Raft partition group ID (defaults to "default")
        #[arg(short, long, default_value = "default")]
        group: String,
        /// Mutation action (e.g. config_update, custom)
        #[arg(short, long)]
        action: String,
        /// Payload data associated with the proposal
        #[arg(short, long)]
        data: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Reconfigure cluster membership using joint consensus (add, remove, promote, demote)
    Reconfigure {
        /// Raft partition group ID (defaults to "default")
        #[arg(short, long, default_value = "default")]
        group: String,
        /// Voting nodes to add (format: id, or id@address:port)
        #[arg(long)]
        add: Vec<String>,
        /// Voting nodes to remove by node ID
        #[arg(long)]
        remove: Vec<String>,
        /// Non-voting learner nodes to add for log catch-up (format: id, or id@address:port)
        #[arg(long)]
        learner: Vec<String>,
        /// Promote an existing learner node to voting member
        #[arg(long)]
        promote: Option<String>,
        /// Demote a voting node to non-voting learner
        #[arg(long)]
        demote: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Trigger Raft Write-Ahead Log compaction and snapshot creation
    Compact {
        /// Raft partition group ID (defaults to "default")
        #[arg(short, long, default_value = "default")]
        group: String,
        /// Up-to log index to compact; if omitted, compacts up to last applied index
        #[arg(short, long)]
        index: Option<u64>,
        /// Force compaction regardless of log retention policy
        #[arg(short, long)]
        force: bool,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Multi-Raft partition routing, key hashing, and group lifecycle management
    Partition {
        #[command(subcommand)]
        action: RaftPartitionCommands,
    },
    /// Acquire a linearizable distributed lock with monotonic fencing token
    Lock {
        /// Name of the distributed lock resource
        #[arg(short, long)]
        name: String,
        /// Holder node or client identifier
        #[arg(short = 'H', long, default_value = "local-node")]
        holder: String,
        /// Lease duration in seconds
        #[arg(short, long, default_value_t = 60)]
        lease: u64,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Release an existing distributed lock
    Unlock {
        /// Name of the distributed lock resource
        #[arg(short, long)]
        name: String,
        /// Holder node or client identifier
        #[arg(short = 'H', long, default_value = "local-node")]
        holder: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Voluntarily step down as leader to follower state
    StepDown {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Transfer leadership to a designated cluster member
    Transfer {
        /// Target node identifier
        #[arg(short, long)]
        target: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Inspect replicated Write-Ahead Log (WAL) entries from disk
    Logs {
        /// Maximum number of log entries to display
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum RaftPartitionCommands {
    /// List all Multi-Raft partitions and their key ranges
    List {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Route a partition key (e.g. server UUID, tenant ID) to its designated Raft group
    Route {
        /// Key string to route
        key: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Create a new Multi-Raft partition
    Create {
        /// Partition group numerical identifier
        #[arg(short, long)]
        group: u64,
        /// Partition descriptive name
        #[arg(short, long)]
        name: String,
        /// Lower key range boundary (inclusive string or hex)
        #[arg(short = 's', long)]
        range_start: String,
        /// Upper key range boundary (inclusive string or hex)
        #[arg(short = 'e', long)]
        range_end: String,
        /// Designated initial leader node
        #[arg(short, long)]
        leader: Option<String>,
        /// Initial peer node IDs
        #[arg(short, long)]
        peers: Vec<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Remove an existing Multi-Raft partition
    Remove {
        /// Partition group numerical identifier
        #[arg(short, long)]
        group: u64,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum QuotaCommands {
    /// List all server resource quotas, active cgroups, and throttling telemetry
    List {
        /// Filter by tenant ID
        #[arg(short, long)]
        tenant: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Inspect resource limits and cgroups v2 statistics for a specific server
    Get {
        /// Target server name or path
        server: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Set or update resource quota limits for a server (hot-applied to cgroups v2)
    Set {
        /// Target server name or path
        server: String,
        /// Maximum CPU percentage (e.g. 150 = 1.5 cores, mapped to cpu.max)
        #[arg(long)]
        cpu: Option<u32>,
        /// Hard memory limit in MB (mapped to memory.max)
        #[arg(long)]
        memory: Option<u64>,
        /// Soft memory throttle threshold in MB (mapped to memory.high)
        #[arg(long)]
        memory_high: Option<u64>,
        /// CFS / cgroups v2 fair-share CPU weight (1 to 10000, mapped to cpu.weight)
        #[arg(long)]
        cpu_weight: Option<u32>,
        /// Fair-share I/O weight (1 to 10000, mapped to io.weight)
        #[arg(long)]
        io_weight: Option<u32>,
        /// Maximum tasks/threads limit (mapped to pids.max)
        #[arg(long)]
        pids_max: Option<u32>,
        /// Priority tier (gateway, standard, worker, batch)
        #[arg(long)]
        priority: Option<String>,
        /// Assign to tenant ID
        #[arg(long)]
        tenant: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Manage tenant-level resource quota budgets
    Tenant {
        #[command(subcommand)]
        action: TenantQuotaCommands,
    },
    /// Enforce fair-share scheduling arbitration across active cgroups
    Balance {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum TenantQuotaCommands {
    /// List all tenant quota allocations and aggregate utilization
    List {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Get quota details and utilization for a specific tenant
    Get {
        /// Tenant ID
        tenant: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Set or update tenant quota allocations
    Set {
        /// Tenant ID
        tenant: String,
        /// Maximum allowed server instances for this tenant
        #[arg(long, alias = "servers")]
        max_servers: Option<usize>,
        /// Aggregate memory limit in MB across all tenant servers
        #[arg(long, alias = "memory")]
        max_memory: Option<u64>,
        /// Aggregate CPU quota percentage across all tenant servers
        #[arg(long, alias = "cpu")]
        max_cpu: Option<u32>,
        /// Aggregate storage limit in GB across all tenant servers
        #[arg(long)]
        max_storage_gb: Option<u64>,
        /// Allow CPU bursting beyond base quota
        #[arg(long)]
        allow_burst: Option<bool>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum TraceCommands {
    /// Show current tracing status, buffer metrics, sampler state, and OTLP exporter health
    Status {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List recorded distributed traces with optional duration and service filters
    List {
        /// Filter traces matching a service name
        #[arg(short, long)]
        service: Option<String>,
        /// Filter traces with minimum duration in milliseconds
        #[arg(short = 'd', long = "min-duration-ms")]
        min_duration_ms: Option<u64>,
        /// Maximum number of traces to display
        #[arg(short = 'l', long = "limit", default_value = "20")]
        limit: usize,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Inspect full causal span tree and attributes for a specific Trace ID
    Get {
        /// 16-byte hex Trace ID
        trace_id: String,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Force an immediate OTLP/HTTP push export of all buffered spans
    Export {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Configure runtime tracing settings (sampler, sample ratio, OTLP endpoint)
    Config {
        /// Enable or disable distributed tracing
        #[arg(long)]
        enabled: Option<bool>,
        /// Sampler strategy (always_on, always_off, ratio)
        #[arg(long)]
        sampler: Option<String>,
        /// Sampling probability ratio between 0.0 and 1.0
        #[arg(long)]
        sample_ratio: Option<f64>,
        /// OpenTelemetry OTLP/HTTP collector endpoint (e.g., http://localhost:4318/v1/traces)
        #[arg(long)]
        otlp_endpoint: Option<String>,
        /// Default service name for this node (default: craft-daemon)
        #[arg(long)]
        service_name: Option<String>,
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum AnvilCommands {
    /// Inspect background Anvil storage engine status, cache memory, hit ratio, and context-switch savings
    Status {
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect an Anvil (.mca) region file, chunk allocation table, sector fragmentation, and free runs
    Inspect {
        /// Server name or path (optional if direct path provided)
        server: Option<String>,
        /// Region file name (e.g. r.0.0.mca) or path
        #[arg(long)]
        file: String,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Prefetch chunks in a radius around chunk coordinates into LRU direct memory
    Prefetch {
        /// Server name or path
        server: String,
        /// World dimension name (default: world)
        #[arg(long)]
        world: Option<String>,
        /// Center chunk X coordinate
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        x: i32,
        /// Center chunk Z coordinate
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        z: i32,
        /// Radius in chunks (1 to 16)
        #[arg(long, default_value_t = 4)]
        radius: u32,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Benchmark Anvil sequential and random read/write throughput, compression ratio, and IO latency
    Bench {
        /// Number of synthetic chunk payloads to benchmark (4 to 1024)
        #[arg(long, default_value_t = 32)]
        chunks: usize,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// View or configure Anvil storage engine parameters (engine, cache limit, prefetch radius)
    Config {
        /// Enable or disable hardware-accelerated Anvil storage engine
        #[arg(long)]
        enabled: Option<bool>,
        /// Preferred I/O engine ("io_uring" or "threaded_fallback")
        #[arg(long)]
        engine: Option<String>,
        /// Direct memory LRU cache size limit in megabytes
        #[arg(long)]
        cache_mb: Option<usize>,
        /// Default prefetch radius in chunks
        #[arg(long)]
        prefetch_radius: Option<u32>,
        /// Batch size for asynchronous chunk operations
        #[arg(long)]
        batch_size: Option<usize>,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum NumaCommands {
    /// Inspect NUMA node topology, core allocations, hugepages, and DPDK driver statistics
    Status {
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Pin a server process to dedicated CPU cores and NUMA memory node
    Pin {
        /// Server name or path
        server: String,
        /// CPU cores range or list (e.g. "2-5" or "2,3,4,5")
        #[arg(long)]
        cpus: String,
        /// Preferred NUMA node index (e.g. 0, 1)
        #[arg(long)]
        node: Option<u32>,
        /// NUMA memory allocation policy (local, interleave, preferred, bind)
        #[arg(long, default_value = "local")]
        policy: String,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Update NUMA memory allocation policy for a server
    Policy {
        /// Server name or path
        server: String,
        /// NUMA memory allocation policy (local, interleave, preferred, bind)
        #[arg(long)]
        policy: String,
        /// Preferred or bound NUMA node index
        #[arg(long)]
        node: Option<u32>,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Benchmark local vs remote NUMA node memory bandwidth, latency, and packet ring throughput
    Bench {
        /// NUMA node index to benchmark (defaults to node 0)
        #[arg(long, default_value_t = 0)]
        node: u32,
        /// Buffer size in megabytes for memory throughput tests
        #[arg(long, default_value_t = 16)]
        size_mb: usize,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate recommended Linux kernel boot command-line arguments for zero-jitter core isolation
    BootArgs {
        /// CPU cores to isolate from kernel scheduling (e.g. "2-7")
        #[arg(long)]
        cores: String,
        /// Number of 1GB hugepages to reserve
        #[arg(long)]
        hugepages_1g: Option<usize>,
        /// Number of 2MB hugepages to reserve
        #[arg(long)]
        hugepages_2m: Option<usize>,
        /// Emit results as structured JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum MigrateCommands {
    /// Zero-downtime live game server migration with iterative dirty memory pre-copy and socket handoff
    Live {
        /// Server name to live migrate
        server: String,
        /// Destination node ID
        #[arg(long)]
        target_node: String,
        /// Destination node host address (defaults to node ID if omitted)
        #[arg(long)]
        target_host: Option<String>,
        /// Destination server port (defaults to 25565)
        #[arg(long, default_value_t = 25565)]
        target_port: u16,
        /// Maximum allowable freeze window in milliseconds (default: 250)
        #[arg(long, default_value_t = 250)]
        freeze_max_ms: u64,
        /// Output status as JSON
        #[arg(long)]
        json: bool,
    },
    /// Query status of active or completed live migrations
    Status {
        /// Migration ID to inspect (omit for all migrations)
        #[arg(long)]
        id: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Abort an in-flight live migration and trigger automatic rollback
    Abort {
        /// Migration ID to abort
        #[arg(long)]
        id: String,
        /// Abort reason explanation
        #[arg(long)]
        reason: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List all live migrations
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum AnycastCommands {
    /// Announce, withdraw, prepend, or list Anycast route prefixes
    Route {
        /// Action to perform: announce, withdraw, prepend, or list
        action: String,
        /// IP prefix (e.g. 198.51.100.0/24)
        #[arg(long)]
        prefix: Option<String>,
        /// Autonomous System Number (ASN)
        #[arg(long)]
        asn: Option<u32>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum BpfCommands {
    /// Attach an eBPF tracepoint probe or trace syscalls for a server
    Trace {
        /// Target server name
        server: String,
        /// Profiling duration in seconds (default: 30)
        #[arg(short = 'd', long = "duration", default_value = "30")]
        duration: u64,
        /// Event type to profile: read, write, futex, epoll, safepoint, gc, socket, or all (default: all)
        #[arg(short = 'e', long = "event", default_value = "all")]
        event: String,
        /// Sampling rate in Hertz (default: 99)
        #[arg(short = 'r', long = "rate", default_value = "99")]
        rate: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Query active eBPF probe status and syscall telemetry for a server
    Status {
        /// Target server name
        server: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Export or inspect collapsed hierarchical stack flame graphs
    Flamegraph {
        /// Target server name
        server: String,
        /// Output file path (defaults to stdout or ~/.craft/ebpf/flamegraphs/<server>.svg)
        #[arg(short = 'o', long = "out")]
        out: Option<PathBuf>,
        /// Output format: ascii or svg (default: ascii)
        #[arg(short = 'f', long = "format", default_value = "ascii")]
        format: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect deep JVM garbage collection pauses and safepoint synchronizations
    Gc {
        /// Target server name
        server: String,
        /// Maximum number of recent GC events to display (default: 10)
        #[arg(short = 'n', long = "limit", default_value = "10")]
        limit: usize,
        /// Continuously watch for real-time GC pause spikes
        #[arg(short = 'w', long = "watch")]
        watch: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Detach an active eBPF kernel profiling probe
    Stop {
        /// Target server name
        server: String,
        /// Specific probe ID to detach (optional, detaches first active probe if omitted)
        #[arg(long = "probe-id")]
        probe_id: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum AttestCommands {
    /// Verify cryptographic provenance and SLSA attestation for a jar or binary artifact
    Verify {
        /// Path to target artifact file (jar, tar, zip, binary)
        artifact: PathBuf,
        /// Explicit path to provenance attestation or Sigstore bundle file (optional)
        #[arg(short = 'a', long = "attestation")]
        attestation: Option<PathBuf>,
        /// Enforce strict block mode regardless of configured policy
        #[arg(long)]
        strict: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect decoded in-toto Statement v1 and SLSA predicate metadata
    Inspect {
        /// Identifier (path to attestation file or artifact SHA-256)
        identifier: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect or modify the active cryptographic supply chain policy
    Policy {
        /// Update enforcement mode: audit, strict, or disabled
        #[arg(long = "set-mode")]
        set_mode: Option<String>,
        /// Update minimum required SLSA level: 0, 1, 2, 3, or 4
        #[arg(long = "min-slsa")]
        min_slsa: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Sign an artifact with an in-toto SLSA provenance statement and generate DSSE envelope
    Sign {
        /// Path to target artifact file
        artifact: PathBuf,
        /// Output path for generated attestation file (default: <artifact>.json)
        #[arg(short = 'o', long = "out")]
        out: Option<PathBuf>,
        /// Signing key ID
        #[arg(short = 'k', long = "key-id", default_value = "default-builder-key")]
        key_id: String,
        /// Builder ID for SLSA predicate (default: craft-hermetic-builder)
        #[arg(short = 'b', long = "builder", default_value = "craft-hermetic-builder")]
        builder: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Execute a hermetic isolated build with environment scrubbing and reproducible packaging
    Hermetic {
        /// Target build directory containing sources
        build_dir: PathBuf,
        /// Command to execute inside isolated environment
        command: String,
        /// Arguments to pass to the build command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        /// Allow outbound network egress during build
        #[arg(long)]
        allow_network: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum PqcCommands {
    /// Inspect post-quantum cryptography status, harvest defense score, and active keypairs
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect or modify post-quantum policy enforcement mode and ciphersuite
    Policy {
        /// Set enforcement mode (classic_only, hybrid, post_quantum_only)
        #[arg(long = "set-mode")]
        set_mode: Option<String>,
        /// Preferred post-quantum ciphersuite (x25519, hybrid_x25519_ml_kem_768, ml_kem_768, ml_kem_1024)
        #[arg(long = "ciphersuite")]
        ciphersuite: Option<String>,
        /// Allow classical fallback if peer lacks post-quantum support (true/false)
        #[arg(long = "allow-fallback")]
        allow_fallback: Option<bool>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate a new NIST FIPS 203/204 post-quantum keypair (ML-KEM or ML-DSA)
    Keygen {
        /// Post-quantum key encapsulation suite (hybrid, mlkem768, mlkem1024)
        #[arg(long = "suite")]
        suite: Option<String>,
        /// Post-quantum signature algorithm (mldsa65)
        #[arg(long = "algo")]
        algo: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run pure-Rust hardware/software micro-benchmarks for ML-KEM and ML-DSA
    Bench {
        /// Number of benchmark iterations
        #[arg(long = "iterations", default_value = "10")]
        iterations: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Transition the cluster post-quantum migration phase (planning, dual_stack, enforced_pqc)
    Migrate {
        /// Target migration phase
        #[arg(long = "phase")]
        phase: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum HsmCommands {
    /// Inspect hardware security module tokens, TPM 2.0 PCR registers, and active hardware keys
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate a non-extractable hardware key handle inside secure token boundary
    Keygen {
        /// Unique key label
        #[arg(long = "label")]
        label: String,
        /// Key algorithm type (ed25519, mldsa65, rsa2048, rsa4096, ec_p256, ec_p384)
        #[arg(long = "type")]
        key_type: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Execute cryptographic signing directly within the secure hardware token boundary
    Sign {
        /// Key label or handle ID
        #[arg(long = "label")]
        label: String,
        /// Data payload string to sign
        #[arg(long = "data")]
        data: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate or verify a signed TPM 2.0 enclave attestation quote
    Attest {
        /// Platform Configuration Register (PCR) selection bitmask (e.g. 7 for PCR 0-2)
        #[arg(long = "pcr-mask")]
        pcr_mask: Option<u32>,
        /// Fresh 32-byte cryptographic challenge nonce (hex-encoded)
        #[arg(long = "nonce")]
        nonce: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate or verify Schnorr-Pedersen Zero-Knowledge Proof cluster membership
    ZkMember {
        /// Action to perform: prove or verify
        #[arg(long = "action")]
        action: String,
        /// Target cluster identity
        #[arg(long = "cluster")]
        cluster: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum MemoryCommands {
    /// Show memory fragmentation, buddy allocator distribution, THP status and page pool statistics
    Status {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Trigger proactive memory compaction and buddy page coalescing
    Compact {
        /// Target buddy allocator order (default: 9 for 2MB hugepages)
        #[arg(long, default_value = "9")]
        target_order: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Configure transparent hugepages (THP) mode and defragmentation strategy
    Thp {
        /// THP allocation mode: always, madvise, never
        #[arg(long)]
        mode: String,
        /// Defragmentation strategy: always, defer, defer+madvise, madvise, never
        #[arg(long)]
        defrag: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark zero-allocation socket page pool recycling throughput and DMA performance
    Pool {
        /// Number of simulated packets to cycle through page pool
        #[arg(long, default_value = "10000")]
        packets: usize,
        /// Packet slice payload size in bytes
        #[arg(long, default_value = "1024")]
        slice_size: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum XdpCommands {
    /// Show current eBPF XDP firewall status, drop metrics, active rules, and tracked flows
    Status {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Attach eBPF XDP driver to network interface
    Attach {
        /// Network interface name (e.g. eth0, ens3, lo)
        interface: String,
        /// Attachment mode: native, offload, generic
        #[arg(short, long, default_value = "generic")]
        mode: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Detach eBPF XDP driver from current network interface
    Detach {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Manage eBPF XDP firewall filtering rules
    Rule {
        #[command(subcommand)]
        action: XdpRuleSubcommands,
    },
    /// Reset packet counters and drop metrics
    ResetMetrics {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark pure-Rust XDP pipeline throughput with synthetic attack simulation
    Bench {
        /// Total packets to simulate
        #[arg(long, default_value = "100000")]
        packets: usize,
        /// Ratio of attack packets to generate (0.0 - 1.0)
        #[arg(long, default_value = "0.7")]
        attack_ratio: f64,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum PmuCommands {
    /// Show current hardware PMU counters, CMPI/BMPI metrics, and execution hotspots
    Status {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Start hardware PMU counter sampling or sample once
    Sample {
        /// Target process PID to profile (defaults to global system / Minecraft server)
        #[arg(short, long)]
        pid: Option<u32>,
        /// Sampling rate in Hertz (samples per second)
        #[arg(short, long, default_value = "100")]
        rate: u32,
        /// Stop active background sampling session
        #[arg(long)]
        stop: bool,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Inspect top execution hotspot symbols, JIT methods, and CPU percentage
    Hotspots {
        /// Number of top hotspots to display
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark hardware cache performance using synthetic sequential vs strided memory churn
    Bench {
        /// Total iterations to execute
        #[arg(short, long, default_value = "5000")]
        iterations: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset hardware PMU counter aggregates and clear sample ring buffers
    ResetMetrics {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum ShmCommands {
    /// Inspect active shared memory segments, ring buffer states, throughput, and lease health
    Status {
        /// Filter by target server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Create a high-speed zero-copy shared memory ring buffer channel
    Create {
        /// Target server name
        #[arg(short, long)]
        server: String,
        /// Channel / ring buffer identifier (e.g. packet_bus, state_sync)
        #[arg(short, long)]
        channel: String,
        /// Ring buffer slot payload size in bytes (default: 4096)
        #[arg(long, default_value_t = 4096)]
        slot_size: usize,
        /// Number of slots in the circular ring buffer (default: 1024)
        #[arg(long, default_value_t = 1024)]
        slots: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Close an active shared memory segment and unlink its POSIX handle
    Close {
        /// Target server name
        #[arg(short, long)]
        server: String,
        /// Channel / ring buffer identifier
        #[arg(short, long)]
        channel: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark microsecond zero-copy throughput and SPSC latency
    Bench {
        /// Number of messages to stream through the benchmark ring
        #[arg(long, default_value_t = 20_000)]
        messages: usize,
        /// Payload size in bytes per message
        #[arg(long, default_value_t = 128)]
        size: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset shared memory cumulative throughput metrics and clear expired leases
    ResetMetrics {
        /// Target server name (resets all if omitted)
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum PatchCommands {
    /// Inspect active dynamic binary patches, target symbols, and application latencies
    Status {
        /// Filter by target server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Apply an atomic dynamic binary trampoline patch or bytecode swap
    Apply {
        /// Target server name
        #[arg(short, long)]
        server: String,
        /// Patch identifier name
        #[arg(short, long)]
        patch: String,
        /// Target function symbol or method signature
        #[arg(short, long)]
        target: String,
        /// Hex-encoded replacement machine code or bytecode payload
        #[arg(short, long, default_value = "90909090")]
        bytes: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Rollback an active patch and restore original function prologue
    Rollback {
        /// Target server name
        #[arg(short, long)]
        server: String,
        /// Patch identifier name
        #[arg(short, long)]
        patch: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Display unified diff disassembly or bytecode instructions between original and patched code
    Diff {
        /// Target server name
        #[arg(short, long)]
        server: String,
        /// Patch identifier name
        #[arg(short, long)]
        patch: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark dynamic binary trampoline patch generation and safety verification throughput
    Bench {
        /// Number of iterations to execute
        #[arg(long, default_value_t = 1000)]
        iterations: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset dynamic binary patching metrics and latency counters
    ResetMetrics {
        /// Target server name (resets all if omitted)
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum VmCommands {
    /// Inspect active MicroVM sandboxes, allocated resources, and KVM capabilities
    Status {
        /// Filter by MicroVM ID or name
        #[arg(short, long)]
        id: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Provision a new hardware-isolated MicroVM sandbox with Firecracker/KVM virtualization
    Spawn {
        /// MicroVM human-readable name
        #[arg(short, long)]
        name: String,
        /// Number of allocated virtual CPUs (default: 1)
        #[arg(short, long, default_value_t = 1)]
        vcpus: u32,
        /// Allocated physical memory in megabytes (default: 256)
        #[arg(short, long, default_value_t = 256)]
        memory: u64,
        /// Explicit AF_VSOCK Context ID (CID >= 3)
        #[arg(short, long)]
        cid: Option<u32>,
        /// Comma-separated virtio devices: net,block,vsock,console (default: net,vsock,block)
        #[arg(short, long)]
        devices: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Stop and terminate a running MicroVM sandbox
    Stop {
        /// MicroVM ID or name to terminate
        #[arg(short, long)]
        id: String,
        /// Force immediate termination without guest shutdown signal
        #[arg(short, long)]
        force: bool,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Inspect low-level MicroVM descriptor, virtio devices, and cold start metrics
    Inspect {
        /// MicroVM ID or name to inspect
        #[arg(short, long)]
        id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark concurrent MicroVM cold starts and host-guest AF_VSOCK throughput
    Bench {
        /// Number of concurrent workers (default: 4)
        #[arg(short, long, default_value_t = 4)]
        concurrency: usize,
        /// Total number of MicroVM bootstrap iterations (default: 20)
        #[arg(long, default_value_t = 20)]
        iterations: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset MicroVM cumulative metrics and clear terminated instances
    ResetMetrics {
        /// Target MicroVM ID (resets all if omitted)
        #[arg(short, long)]
        id: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum CrashCommands {
    /// Inspect crash triage registry, active memory leaks, and triage summary
    Status {
        /// Filter by server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Triage an ELF core dump, hs_err_pid log, or leak history file
    Triage {
        /// Target server name
        #[arg(short, long)]
        server: Option<String>,
        /// Path to crash dump or log file
        #[arg(short, long)]
        file: String,
        /// Optional path to memory leak history JSON file
        #[arg(short, long)]
        leak_history: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List historical crash triage reports
    List {
        /// Filter by server name
        #[arg(short, long)]
        server: Option<String>,
        /// Maximum number of reports to display
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Inspect a detailed crash triage report by ID
    Inspect {
        /// Report ID (e.g. crash-1727190000)
        #[arg(short, long)]
        id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark ELF core dump parsing, hs_err parsing, and leak detection performance
    Bench {
        /// Benchmark iteration count
        #[arg(short, long, default_value_t = 50)]
        iterations: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset crash triage metrics and clear active leak tracking
    ResetMetrics {
        /// Filter by server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum RdmaCommands {
    /// Inspect RDMA transport status, NIC hardware info, active memory regions, and peer links
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Register a direct memory region (MR) with lkey/rkey protections for RDMA offloading
    MrRegister {
        /// Filter or associate with server name
        #[arg(short, long)]
        server: Option<String>,
        /// Memory region size in bytes
        #[arg(short = 'b', long = "size", default_value_t = 67108864)]
        size: usize,
        /// Register with read-only access (disallows remote writes)
        #[arg(long)]
        read_only: bool,
        /// Allow atomic operations (fetch-and-add, compare-and-swap)
        #[arg(long)]
        atomic: bool,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Connect a remote RDMA peer queue pair (QP)
    Connect {
        /// Peer identifier or node name
        #[arg(short, long)]
        peer: String,
        /// Peer socket address (ip:port)
        #[arg(short, long)]
        addr: String,
        /// Transport protocol: rocev2, rocev1, infiniband
        #[arg(short, long, default_value = "rocev2")]
        transport: String,
        /// Maximum Transmission Unit (MTU) size (1024, 2048, 4096)
        #[arg(short, long, default_value_t = 4096)]
        mtu: u32,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List active RDMA peers, link states, and round-trip latencies
    Peers {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark RDMA write/read memory throughput and sub-microsecond latency
    Bench {
        /// Target peer node (runs synthetic loopback benchmark if omitted)
        #[arg(short, long)]
        peer: Option<String>,
        /// Benchmark iteration count
        #[arg(short, long, default_value_t = 10000)]
        iterations: usize,
        /// Transfer payload size in bytes
        #[arg(short, long, default_value_t = 4096)]
        size: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset RDMA cumulative transfer counters and error metrics
    ResetMetrics {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum SmartNicCommands {
    /// Inspect SmartNIC ASIC hardware status, TCAM allocation, offload mode, and packet counters
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Add a match-action offload rule to SmartNIC hardware TCAM table
    RuleAdd {
        /// Unique identifier for the offload rule
        #[arg(short, long)]
        rule_id: String,
        /// Target offload protocol: slp, raknet, a2s, ddos, custom
        #[arg(short = 't', long = "protocol", default_value = "slp")]
        protocol: String,
        /// Offload action: drop, forward, pong, syn_cookie, pass
        #[arg(short = 'a', long = "action", default_value = "pong")]
        action: String,
        /// Ingress port to match
        #[arg(short = 'p', long = "port")]
        port: Option<u16>,
        /// Source CIDR subnet to match (e.g. 192.168.1.0/24)
        #[arg(short = 'c', long = "cidr")]
        cidr: Option<String>,
        /// Priority for rule ordering (higher number = higher priority)
        #[arg(long, default_value_t = 10)]
        priority: u32,
        /// Filter or associate with server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Remove an offload rule from SmartNIC hardware
    RuleRm {
        /// Identifier of the rule to remove
        #[arg(short, long)]
        rule_id: String,
        /// Filter or associate with server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List active in-hardware P4 match-action rules
    Rules {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark line-rate SmartNIC switching throughput and sub-microsecond latency
    Bench {
        /// Benchmark iteration count
        #[arg(short, long, default_value_t = 50000)]
        iterations: usize,
        /// Packet payload size in bytes
        #[arg(short = 'b', long = "packet-size", default_value_t = 64)]
        packet_size: usize,
        /// Filter or associate with server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset SmartNIC cumulative hardware packet counters and fallback events
    ResetMetrics {
        /// Filter or associate with server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum MemFabricCommands {
    /// Inspect memory fabric status, DRAM/NVRAM pool utilization, and node topology
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Allocate a virtual memory page in the designated memory tier
    PageAlloc {
        /// Identifier for the memory page
        #[arg(short, long)]
        page_id: String,
        /// Size of the page in bytes (default: 4096)
        #[arg(short = 'z', long = "size", default_value_t = 4096)]
        size: usize,
        /// Memory tier: local, cxl, remote, nvram
        #[arg(short = 't', long = "tier", default_value = "local")]
        tier: String,
        /// Optional associated Minecraft dimension (e.g. the_nether, the_end)
        #[arg(short = 'd', long = "dimension")]
        dimension: Option<String>,
        /// Associated server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Evict all resident pages belonging to a dormant dimension to remote NVRAM pool
    EvictDim {
        /// Name of the Minecraft dimension to evict (e.g. the_nether, the_end)
        #[arg(short, long)]
        dimension: String,
        /// Destination remote memory fabric node
        #[arg(short = 'n', long = "target-node")]
        target_node: Option<String>,
        /// Associated server name
        #[arg(short = 's', long = "server")]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List active memory fabric pages and their memory tier placements
    Pages {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark remote paging and userfaultfd resolution throughput
    Bench {
        /// Benchmark page iteration count
        #[arg(short, long, default_value_t = 50)]
        iterations: usize,
        /// Page size in bytes
        #[arg(short = 'z', long = "page-size", default_value_t = 4096)]
        page_size: usize,
        /// Associated server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset memory fabric telemetry counters and page fault history
    ResetMetrics {
        /// Filter or associate with server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum NvmeCommands {
    /// Inspect NVMe-oF target status, subsystem NQNs, and flash pool capacity
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Dynamically allocate a new storage namespace in the distributed flash pool
    #[command(name = "ns-create", alias = "create-ns")]
    NsCreate {
        /// Namespace ID (NSID 1..N)
        #[arg(short = 'n', long = "nsid")]
        nsid: u32,
        /// Size of the namespace in megabytes (MB)
        #[arg(short = 's', long = "size-mb", default_value_t = 1024)]
        size_mb: u64,
        /// Logical block size in bytes (default: 4096)
        #[arg(short = 'b', long = "block-size", default_value_t = 4096)]
        block_size: u32,
        /// Associated server name
        #[arg(long)]
        server: Option<String>,
        /// Optional associated Minecraft dimension (e.g. the_nether, the_end)
        #[arg(short = 'd', long = "dimension")]
        dimension: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Delete an existing storage namespace from the flash pool
    #[command(name = "ns-delete", alias = "delete-ns")]
    NsDelete {
        /// Target Namespace ID to remove
        #[arg(short = 'n', long = "nsid")]
        nsid: u32,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all allocated storage namespaces in the flash pool
    #[command(name = "namespaces", alias = "ns-list")]
    Namespaces {
        /// Filter by server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all configured NVMe-oF target subsystems and transport ports
    #[command(name = "subsystems", alias = "subs")]
    Subsystems {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark 4KB random flash block I/O throughput and multipath failover
    Bench {
        /// Benchmark iteration count
        #[arg(short = 'i', long = "iterations", default_value_t = 50)]
        iterations: usize,
        /// Block transfer size in bytes
        #[arg(short = 'b', long = "block-size", default_value_t = 4096)]
        block_size: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset NVMe storage fabric telemetry counters and performance statistics
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Associated server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum VpnCommands {
    /// Inspect WireGuard PQXDH VPN mesh status, active tunnels, and quantum defense grade
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all configured WireGuard PQXDH tunnels and peer topology
    #[command(name = "tunnels", alias = "list")]
    Tunnels {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Dynamically create a new WireGuard PQXDH mesh tunnel interface
    #[command(name = "tunnel-create", alias = "create-tunnel")]
    TunnelCreate {
        /// Tunnel interface identifier (e.g. craft-wg0)
        #[arg(short = 't', long = "tunnel-id")]
        tunnel_id: String,
        /// Local IPv4/IPv6 CIDR address (e.g. 10.42.0.1/24)
        #[arg(short = 'a', long = "address")]
        address: String,
        /// Listening UDP port (default: 51820)
        #[arg(short = 'p', long = "port", default_value_t = 51820)]
        port: u16,
        /// Cryptographic engine mode (hardware_p4, hybrid_kyber_chacha, software_kernel)
        #[arg(short = 'm', long = "crypto-mode")]
        crypto_mode: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Delete an existing WireGuard PQXDH mesh tunnel interface
    #[command(name = "tunnel-delete", alias = "delete-tunnel")]
    TunnelDelete {
        /// Tunnel identifier to remove
        #[arg(short = 't', long = "tunnel-id")]
        tunnel_id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Add or update a peer endpoint in a WireGuard PQXDH tunnel
    #[command(name = "peer-add", alias = "add-peer")]
    PeerAdd {
        /// Target tunnel identifier
        #[arg(short = 't', long = "tunnel-id")]
        tunnel_id: String,
        /// Unique peer identifier
        #[arg(short = 'p', long = "peer-id")]
        peer_id: String,
        /// Remote socket endpoint address (e.g. 198.51.100.10:51820)
        #[arg(short = 'e', long = "endpoint")]
        endpoint: String,
        /// Allowed IPs routed through peer tunnel
        #[arg(long = "allowed-ip")]
        allowed_ips: Vec<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Remove a peer endpoint from a WireGuard PQXDH tunnel
    #[command(name = "peer-rm", alias = "peer-delete", alias = "rm-peer")]
    PeerRm {
        /// Target tunnel identifier
        #[arg(short = 't', long = "tunnel-id")]
        tunnel_id: String,
        /// Target peer identifier to remove
        #[arg(short = 'p', long = "peer-id")]
        peer_id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Trigger zero-loss sub-100us ephemeral key rotation on a tunnel or peer
    #[command(name = "rotate-key", alias = "rekey")]
    RotateKey {
        /// Target tunnel identifier
        #[arg(short = 't', long = "tunnel-id")]
        tunnel_id: String,
        /// Specific peer identifier to re-key (all peers if omitted)
        #[arg(short = 'p', long = "peer-id")]
        peer_id: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark >10Gbps line-rate encryption, Kyber-1024 handshakes, and key renegotiation
    Bench {
        /// Number of benchmark packet iterations
        #[arg(short = 'i', long = "iterations", default_value_t = 1000)]
        iterations: usize,
        /// Packet payload size in bytes
        #[arg(short = 's', long = "packet-size", default_value_t = 1024)]
        packet_size: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset VPN mesh telemetry counters and performance statistics
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum BftCommands {
    /// Inspect BFT consensus cluster state, view pacemaker telemetry, and slashing status
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List active BFT consensus validators and cryptographic voting keys
    #[command(name = "validators", alias = "list")]
    Validators {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Register a new validator node into the BFT consensus quorum
    #[command(name = "validator-add", alias = "add-validator")]
    ValidatorAdd {
        /// Unique validator node identifier
        #[arg(short = 'i', long = "id")]
        id: String,
        /// Voting weight allocated to validator
        #[arg(short = 'w', long = "voting-weight", default_value_t = 1)]
        voting_weight: u64,
        /// BLS public key in hex format (auto-generated if omitted)
        #[arg(short = 'k', long = "public-key")]
        public_key: Option<String>,
        /// Validator role (Validator, Leader, Learner, Slashed)
        #[arg(short = 'r', long = "role", default_value = "Validator")]
        role: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Remove or slash a validator node from the BFT quorum
    #[command(name = "validator-rm", alias = "rm-validator", alias = "slash-validator")]
    ValidatorRm {
        /// Unique validator identifier to remove
        #[arg(short = 'i', long = "id")]
        id: String,
        /// Reason for validator removal or slashing
        #[arg(long = "reason")]
        reason: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Submit a state-transition transaction to the BFT consensus mempool
    #[command(name = "tx-submit", alias = "submit-tx")]
    TxSubmit {
        /// Transaction type (StateUpdate, ConfigChange, MembershipChange, Slashing)
        #[arg(short = 't', long = "type", default_value = "StateUpdate")]
        tx_type: String,
        /// Transaction JSON or raw payload
        #[arg(short = 'p', long = "payload")]
        payload: String,
        /// Transaction sender identifier
        #[arg(short = 's', long = "sender")]
        sender: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Manually trigger a BFT view change / pacemaker leader rotation
    #[command(name = "view-change", alias = "rotate-leader")]
    ViewChange {
        /// Reason for triggering view change
        #[arg(long = "reason")]
        reason: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Verify a zero-knowledge recursive state transition proof file
    #[command(name = "zk-verify", alias = "verify-proof")]
    ZkVerify {
        /// Path to the JSON zero-knowledge state proof file
        #[arg(short = 'p', long = "proof")]
        proof_path: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Run end-to-end BFT consensus benchmark (>5,000 TPS, 3-chain finality, slashing)
    Bench {
        /// Number of transactions to simulate
        #[arg(short = 't', long = "transactions", default_value_t = 500)]
        transactions: usize,
        /// Number of simulated BFT cluster validators
        #[arg(short = 'v', long = "validators", default_value_t = 4)]
        validators: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset BFT consensus telemetry counters and metrics
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum JitterCommands {
    /// Inspect kernel thread scheduling telemetry, micro-stalls, and jitter metrics
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Configure SCHED_FIFO real-time priority and core affinity isolation
    #[command(name = "isolate", alias = "set-realtime", alias = "shield")]
    Isolate {
        /// Target server name
        #[arg(short, long, default_value = "default")]
        server: String,
        /// Real-time SCHED_FIFO priority (1-99)
        #[arg(short, long, default_value_t = 80)]
        priority: u32,
        /// Comma-separated isolated CPU core list (e.g. 2,3)
        #[arg(short, long, default_value = "2,3")]
        cores: String,
        /// Process ID (defaults to current process or server lock PID)
        #[arg(long)]
        pid: Option<u32>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Display captured kernel micro-stall events and priority inversion incidents
    #[command(name = "stalls", alias = "events", alias = "list-stalls")]
    Stalls {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Maximum number of micro-stall events to display
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Rebalance hardware IRQ affinities away from game loop cores to housekeeping cores
    #[command(name = "irq", alias = "rebalance-irq", alias = "mitigate-irq")]
    Irq {
        /// Hardware IRQ interrupt line number
        #[arg(short, long, default_value_t = 33)]
        irq: u32,
        /// Comma-separated target housekeeping CPU core list (e.g. 0,1)
        #[arg(short = 'c', long = "target-cores", default_value = "0,1")]
        target_cores: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Display runqueue latency logarithmic micro-histogram (0-10us through >1ms)
    #[command(name = "histogram", alias = "hist", alias = "latencies")]
    Histogram {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Run synthetic kernel jitter benchmark (<100us P99 scheduling jitter, zero dropped ticks)
    Bench {
        /// Number of benchmark tick iterations
        #[arg(short, long, default_value_t = 1000)]
        iterations: usize,
        /// Number of synthetic micro-stall events to simulate
        #[arg(short = 's', long = "simulated-stalls", default_value_t = 5)]
        simulated_stalls: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset kernel jitter telemetry counters and micro-stall logs
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum NeuromorphicCommands {
    /// Inspect neuromorphic SNN tick scheduling telemetry, LIF neuron states, and STDP synaptic weights
    Status {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Configure neuromorphic tick scheduling mode (Disabled, ShadowInference, PredictiveThrottle, AutonomousMicrosecond)
    #[command(name = "mode", alias = "set-mode", alias = "schedule-mode")]
    Mode {
        /// Target server name
        #[arg(short, long, default_value = "default")]
        server: String,
        /// Scheduling mode (disabled, shadow-inference, predictive-throttle, autonomous-microsecond)
        #[arg(short, long, default_value = "autonomous-microsecond")]
        mode: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Inject a synthetic sensory spike into the neuromorphic SNN input layer
    #[command(name = "inject", alias = "spike-in", alias = "send-spike")]
    Inject {
        /// Target server name
        #[arg(short, long, default_value = "default")]
        server: String,
        /// Target input neuron ID (0-15)
        #[arg(short, long, default_value_t = 0)]
        neuron: u32,
        /// Injected spike synaptic current or weight (0.0 to 1.0)
        #[arg(short, long, default_value_t = 1.0)]
        current: f32,
        /// Sensory spike source type (packet-arrival, scheduler-timer, entity-collision, block-change, redstone-update)
        #[arg(long, default_value = "packet-arrival")]
        source: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Display membrane potential raster plot across input, reservoir, and output neurons
    #[command(name = "raster", alias = "plot", alias = "spikes")]
    Raster {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Maximum number of raster points to display
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Display latest neuromorphic SNN tick prediction, latency forecast, and dynamic sleep duration
    #[command(name = "predict", alias = "prediction", alias = "forecast")]
    Predict {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Run neuromorphic scheduler benchmark (<1.0us forward inference, >95% idle power reduction)
    Bench {
        /// Number of benchmark spike iterations
        #[arg(short, long, default_value_t = 2000)]
        iterations: usize,
        /// Ratio of high-frequency spike bursts to idle periods (0.0 to 1.0)
        #[arg(short = 'b', long = "burst-ratio", default_value_t = 0.2)]
        burst_ratio: f64,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset neuromorphic telemetry counters, raster ring buffer, and STDP synaptic weights
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Filter by server or instance name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum OpticalCommands {
    /// Inspect optical network switching telemetry, MEMS micro-mirror topology, and DWDM wavelengths
    Status {
        /// Filter by circuit or server name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Configure optical switching routing mode (Autonomous, CircuitSwitched, WavelengthRouted, HybridElectronic, Passthrough)
    #[command(name = "mode", alias = "set-mode", alias = "routing-mode")]
    Mode {
        /// Target server or cluster name
        #[arg(short, long, default_value = "default")]
        server: String,
        /// Routing mode (autonomous, circuit-switched, wavelength-routed, hybrid-electronic, passthrough)
        #[arg(short, long, default_value = "autonomous")]
        mode: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Provision a dynamic optical lightpath circuit across photonic crossbars
    #[command(name = "circuit-add", alias = "lightpath-add", alias = "connect")]
    CircuitAdd {
        /// Target server or lightpath tag
        #[arg(short, long, default_value = "default")]
        server: String,
        /// Circuit identifier
        #[arg(short = 'c', long)]
        circuit_id: String,
        /// Ingress port ID (0 to N-1)
        #[arg(short = 'i', long, default_value_t = 0)]
        ingress: u32,
        /// Egress port ID (0 to N-1)
        #[arg(short = 'e', long, default_value_t = 1)]
        egress: u32,
        /// DWDM ITU-T channel index (1 to 64)
        #[arg(short = 'w', long, default_value_t = 1)]
        channel: u16,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Tear down an established optical circuit lightpath
    #[command(name = "circuit-rm", alias = "lightpath-rm", alias = "disconnect")]
    CircuitRm {
        /// Circuit identifier to tear down
        #[arg(short = 'c', long)]
        circuit_id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all active optical lightpath circuits
    #[command(name = "circuits", alias = "list")]
    Circuits {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Benchmark MEMS optical crossbar switching throughput and switching latency
    Bench {
        /// Number of benchmark frames to inject
        #[arg(short, long, default_value_t = 5000)]
        frames: usize,
        /// Benchmark crossbar dimension (N x N, e.g. 16, 32, 64)
        #[arg(short = 'd', long = "dimension", default_value_t = 32)]
        dimension: usize,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Reset optical telemetry counters, collision events, and error stats
    #[command(name = "reset-metrics", alias = "reset")]
    ResetMetrics {
        /// Target server or cluster name
        #[arg(short, long)]
        server: Option<String>,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}


#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum XdpRuleSubcommands {
    /// Add a filtering rule
    Add {
        /// Unique rule identifier
        id: String,
        /// Target CIDR network or IP (e.g. 192.168.1.0/24, 10.0.0.1, or 'any')
        #[arg(long, default_value = "any")]
        cidr: String,
        /// Optional destination port
        #[arg(short, long)]
        port: Option<u16>,
        /// Transport protocol: any, tcp, udp, icmp
        #[arg(long, default_value = "any")]
        proto: String,
        /// Action: pass, drop, redirect, tx
        #[arg(short, long, default_value = "drop")]
        action: String,
        /// Optional rate limit in packets per second
        #[arg(long)]
        rate_limit: Option<u64>,
        /// Optional temporary ban duration in seconds
        #[arg(long)]
        ban_seconds: Option<u64>,
        /// Rule evaluation priority (higher evaluated first)
        #[arg(long, default_value = "100")]
        priority: u32,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// Remove a filtering rule by ID
    Remove {
        /// Unique rule identifier
        id: String,
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all configured filtering rules
    List {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SoftwareCommands {
    /// List all available software definitions (built-in and custom)
    List {
        /// Filter by game ID (e.g. minecraft, palworld, terraria)
        #[arg(short, long)]
        game: Option<String>,
    },
    /// Inspect details and schema of a specific software definition
    Inspect {
        /// Software ID (e.g. paper, palserver, custom)
        id: String,
    },
    /// Package a definition directory into a .zip bundle archive
    Package {
        /// Path to directory containing software.toml
        dir: PathBuf,
        /// Output .zip file path (defaults to <id>.zip)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Install a .zip package or directory into ~/.craft/softwares/
    Install {
        /// Path to .zip file or definition directory
        package: PathBuf,
        /// Force overwrite if software already exists
        #[arg(short, long)]
        force: bool,
    },
    /// Remove an installed custom software definition
    Remove {
        /// Software ID to remove
        id: String,
    },
    /// Reset default software definitions to factory defaults in ~/.craft/softwares/
    Reset {
        /// Specific software ID to reset (e.g. paper, purpur, palserver)
        id: Option<String>,
        /// Reset all default software definitions
        #[arg(short, long)]
        all: bool,
    },
    /// Scaffold a new custom server software definition directory with templates
    Template {
        /// Software ID for the new definition
        id: String,
        /// Destination directory path (defaults to ./<id>)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum WebhookCommands {
    /// Add a new notification webhook endpoint
    Add {
        /// Unique endpoint name/identifier
        name: String,
        /// Target webhook URL
        url: String,
        /// Webhook type/platform: discord, slack, or generic
        #[arg(long, default_value = "generic")]
        kind: String,
        /// List of subscribed events (crash, restart, circuit_trip, backup, storage, start, stop, all)
        #[arg(long, value_delimiter = ',', default_value = "all")]
        events: Vec<String>,
        /// Optional secret for HMAC-SHA256 signature verification (X-Craft-Signature)
        #[arg(long)]
        secret: Option<String>,
    },
    /// Remove an existing webhook endpoint
    #[command(alias = "rm")]
    Remove {
        /// Webhook endpoint name to remove
        name: String,
    },
    /// List all configured webhook endpoints and subscriptions
    #[command(alias = "ls")]
    List,
    /// Send a test notification ping to a webhook endpoint
    Test {
        /// Webhook endpoint name to test
        name: String,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum GatewayCommands {
    /// Display gateway server status and configuration
    Status,
    /// Enable or disable the gateway server
    Enable {
        /// Set to true to enable, false to disable
        #[arg(default_value_t = true)]
        enabled: bool,
    },
    /// Set or update the gateway authentication Bearer token
    SetToken {
        /// New Bearer token secret (or omit to generate a secure random token)
        token: Option<String>,
    },
    /// Fetch and display Prometheus metrics from the daemon gateway
    Metrics {
        /// Display raw Prometheus metric lines
        #[arg(long)]
        raw: bool,
    },
}

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Show detailed cache usage statistics
    Info,
    /// Clean cached assets (use --force to clear all, or --expired for expired entries)
    Clean {
        #[arg(long)]
        force: bool,
        /// Only clean expired metadata entries
        #[arg(long)]
        expired: bool,
    },
    /// Prune cache down to low watermark (75% of limit) using LRU eviction
    Prune,
    /// Set maximum cache storage limit (e.g. 500MB, 2GB, 5GB)
    SetLimit {
        /// New cache limit with unit (e.g. 500MB, 2GB, 10GB)
        limit: String,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum CatalogCommands {
    /// Build a new compiled catalog by querying upstream providers and compressing to zstd
    Build {
        /// Target output file path (default: versions.zst)
        #[arg(short, long, default_value = "versions.zst")]
        output: PathBuf,
    },
    /// Fetch and update the local cached version catalog from remote worker
    Update {
        /// Custom remote catalog URL
        #[arg(long)]
        url: Option<String>,
    },
    /// Display information and cache status about the local version catalog
    Info,
    /// List available software implementations or versions in the catalog
    List {
        /// Server software ID or name (omit to list all softwares)
        software: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ServiceCommands {
    /// Start the background daemon
    Start {
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the background daemon
    Stop,
    /// Restart the background daemon
    Restart,
    /// Check the background daemon status
    Status,
    /// Install Craft daemon as an automated OS background service (systemd/launchd/Windows)
    Install,
    /// Uninstall Craft daemon automated OS background service
    Uninstall,
    /// View or reset crash circuit breakers for managed servers
    #[command(alias = "breakers")]
    CircuitBreakers {
        /// Reset circuit breaker for specific server (by name or directory path)
        #[arg(long)]
        reset: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AutoCommands {
    /// Instructions for setting up system startup
    How,
    /// Add a server to the auto-run list
    Add {
        name: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Remove a server from the auto-run list
    Rm {
        name: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Start the auto-run service
    Start,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommands {
    /// Search for plugins across Modrinth, Hangar, and Poggit
    Search {
        query: String,
        /// Filter by Minecraft game version (e.g. 1.21.1)
        #[arg(long = "game-version", alias = "version")]
        game_version: Option<String>,
        /// Filter by platform loader (e.g. paper, spigot, velocity, bungeecord)
        #[arg(long)]
        loader: Option<String>,
    },
    /// Install a plugin from Modrinth by project ID or slug with dependency resolution
    Install {
        project_id: String,
        /// Server name or path
        server: String,
        /// Explicit version number or release ID
        #[arg(long)]
        version: Option<String>,
        /// Automatically resolve and download missing hard dependencies
        #[arg(long, default_value_t = true)]
        resolve_deps: bool,
    },
    /// Check for and apply atomic plugin updates from Modrinth
    Update {
        /// Server name or path
        server: String,
        /// Check for available updates without applying them
        #[arg(long)]
        check: bool,
        /// Automatically proceed with updates without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Inspect a plugin JAR archive and display its bytecode manifest metadata
    Inspect {
        /// Path to the plugin JAR file
        file: PathBuf,
    },
    /// Run diagnostic audit on installed plugins to check dependencies, API versions, and collisions
    Doctor {
        /// Server name or path
        server: String,
    },
    /// List installed plugins on a server
    List {
        /// Server name or path
        server: String,
    },
    /// Remove an installed plugin from a server
    Remove {
        /// Server name or path
        server: String,
        /// Plugin jar filename
        filename: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ModCommands {
    /// Search for mods on Modrinth
    Search {
        query: String,
        /// Filter by Minecraft game version (e.g. 1.21.1)
        #[arg(long = "game-version", alias = "version")]
        game_version: Option<String>,
        /// Filter by mod loader (fabric, forge, neoforge, quilt)
        #[arg(long)]
        loader: Option<String>,
    },
    /// Install a mod from Modrinth to a modded server with dependency resolution
    Install {
        project_id: String,
        /// Server name or path
        server: String,
        /// Explicit version number or release ID
        #[arg(long)]
        version: Option<String>,
        /// Automatically resolve and download missing hard dependencies
        #[arg(long, default_value_t = true)]
        resolve_deps: bool,
    },
    /// Check for and apply atomic mod updates from Modrinth
    Update {
        /// Server name or path
        server: String,
        /// Check for available updates without applying them
        #[arg(long)]
        check: bool,
        /// Automatically proceed with updates without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Inspect a mod JAR archive and display its bytecode manifest metadata
    Inspect {
        /// Path to the mod JAR file
        file: PathBuf,
    },
    /// Run diagnostic audit on installed mods to check dependencies and version bounds
    Doctor {
        /// Server name or path
        server: String,
    },
    /// List installed mods on a server
    List {
        /// Server name or path
        server: String,
    },
    /// Remove an installed mod from a server
    Remove {
        /// Server name or path
        server: String,
        /// Mod jar filename
        filename: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DatapackCommands {
    /// Search for datapacks on Modrinth
    Search { query: String },
    /// Install a datapack from Modrinth to a Minecraft Java server world
    Install {
        project_id: String,
        /// Server name or path
        server: String,
        /// Optional target world (defaults to server active level-name)
        #[arg(long)]
        world: Option<String>,
    },
    /// List installed datapacks on a server
    List {
        /// Server name or path
        server: String,
        /// Optional target world (defaults to server active level-name)
        #[arg(long)]
        world: Option<String>,
    },
    /// Remove an installed datapack from a server
    Remove {
        /// Server name or path
        server: String,
        /// Datapack filename (zip or directory)
        filename: String,
        /// Optional target world (defaults to server active level-name)
        #[arg(long)]
        world: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum LogCommands {
    /// Export server logs, crash reports, configuration, and host diagnostic bundle into an archive
    Export {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Custom output archive path (defaults to <server>-diagnostics-<timestamp>.tar.zst)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Compression format: zstd (default, ultra-fast) or gzip
        #[arg(long, default_value = "zstd")]
        format: String,
    },
    /// View / attach to live console of a running server
    View {
        /// Server name
        #[arg(default_value = "")]
        name: String,
        /// Server directory path
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// High-speed regex and token search across multi-server inverted log index blocks
    Search {
        /// Search pattern or keyword
        #[arg(default_value = "")]
        pattern: String,
        /// Target server name (searches all servers if omitted)
        #[arg(short = 's', long)]
        server: Option<String>,
        /// Filter by log level (FATAL, ERROR, WARN, INFO, DEBUG, TRACE)
        #[arg(short = 'l', long)]
        level: Option<String>,
        /// Treat pattern as regular expression
        #[arg(short = 'r', long)]
        regex: bool,
        /// Minimum timestamp filter (RFC3339 or HH:MM:SS)
        #[arg(long)]
        since: Option<String>,
        /// Maximum timestamp filter (RFC3339 or HH:MM:SS)
        #[arg(long)]
        until: Option<String>,
        /// Maximum matching entries to return
        #[arg(long, default_value = "100")]
        limit: usize,
        /// Remote host alias to dispatch federated search query across SSH
        #[arg(long)]
        remote: Option<String>,
        /// Output matching entries in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Automated post-mortem incident forensics, stack demangling, and authenticity proof
    Forensics {
        /// Target server name
        server: String,
        /// Specific incident ID (defaults to most recent incident)
        incident: Option<String>,
        /// Output forensic timeline in JSON format
        #[arg(long)]
        json: bool,
        /// Path to export incident forensic timeline report (.md or .json)
        #[arg(short = 'e', long)]
        export: Option<PathBuf>,
    },
    /// List historical crash incidents and forensic post-mortem reports
    Incidents {
        /// Target server name (lists all if omitted)
        server: Option<String>,
        /// Maximum incidents to display
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Output incidents in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Trigger on-demand indexing of server console logs into compressed inverted blocks
    Index {
        /// Target server name (indexes all registered servers if omitted)
        server: Option<String>,
        /// Force re-indexing from scratch
        #[arg(short = 'f', long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ForecastCommands {
    /// Show workload forecast, diurnal seasonality curves, and surge alerts
    Show {
        /// Target server name
        #[arg(default_value = "")]
        server: String,
        /// Forecast horizon in hours (1..168, default: 24)
        #[arg(short = 'H', long, default_value = "24")]
        horizon: u32,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Generate financial cost optimization ledger and projected savings
    Cost {
        /// Filter by specific server (aggregates fleet if omitted)
        #[arg(short = 's', long)]
        server: Option<String>,
        /// Target remote host alias
        #[arg(long)]
        remote: Option<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// View or configure proactive wake schedules and quiet windows
    Schedule {
        /// Target server name
        #[arg(default_value = "")]
        server: String,
        /// Proactive wake-up lead time in minutes ahead of predicted surge
        #[arg(long)]
        lead_mins: Option<u32>,
        /// Quiet window start hour in UTC (0..23)
        #[arg(long)]
        quiet_start: Option<u8>,
        /// Quiet window end hour in UTC (0..23)
        #[arg(long)]
        quiet_end: Option<u8>,
        /// Enable or disable predictive auto-scaling
        #[arg(long)]
        enabled: Option<bool>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Trigger proactive auto-scaling or downscaling evaluation immediately
    Optimize {
        /// Target server name
        #[arg(default_value = "")]
        server: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum BackupCommands {
    /// Create a compressed backup of a server
    Create {
        server: String,
        /// Only backup world folders and configuration files (excludes logs, caches)
        #[arg(long)]
        world_only: bool,
        /// Compression format: zstd (default, ultra-fast) or gzip
        #[arg(short, long, default_value = "zstd")]
        format: String,
    },
    /// List backups for a server
    List { server: String },
    /// Restore a backup archive to a server
    Restore {
        server: String,
        backup_file: PathBuf,
    },
    /// Inspect or configure the automated backup policy for a server
    Policy {
        server: String,
        /// Enable automated backups
        #[arg(long)]
        enable: Option<bool>,
        /// Backup interval in hours (e.g. 6)
        #[arg(long)]
        interval: Option<u32>,
        /// Cron expression (e.g. "0 3 * * *" or "every 6h")
        #[arg(long)]
        cron: Option<String>,
        /// Retention count: maximum number of backups to keep
        #[arg(long)]
        retention: Option<usize>,
        /// Compression format (zstd or gzip)
        #[arg(long)]
        format: Option<String>,
        /// Enable/disable automated upload to Amazon S3
        #[arg(long)]
        s3: Option<bool>,
        /// Enable/disable automated upload to Google Drive
        #[arg(long)]
        gdrive: Option<bool>,
        /// Restrict automated backups to world folders only
        #[arg(long)]
        world_only: Option<bool>,
    },
}

#[derive(Subcommand)]
pub enum FirewallCommands {
    /// Allow an IP to connect to server port
    Allow { server: String, ip: String },
}

#[derive(Subcommand)]
pub enum LoopbackCommands {
    /// Enable UWP loopback exemption
    Enable,
    /// Check loopback exemption status
    Status,
}

#[derive(Subcommand)]
pub enum TemplateCommands {
    /// Create a Paper/Spigot/Velocity plugin template
    Plugin {
        name: String,
        #[arg(default_value = "paper")]
        platform: String,
    },
    /// Create a Java datapack template
    Datapack { name: String },
}

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// Add a new remote host (e.g. craft remote add my-vps user@192.168.1.100)
    Add {
        alias: String,
        connection: String,
        #[arg(long)]
        key: Option<PathBuf>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// List all configured remote hosts
    Ls,
    /// Remove a remote host configuration
    Rm { alias: String },
    /// Test connection and authentication to a remote host
    Test { alias: String },
    /// Automatically setup and bootstrap a remote host (installs Java, Craft daemon, Firewall)
    Setup { alias: String },
    /// Synchronize files from local to remote server
    Sync {
        alias: String,
        local_dir: PathBuf,
        remote_dir: PathBuf,
    },
    /// Deploy Craft container stack to a remote host over SSH
    Deploy { alias: String },
    /// Setup Docker on a remote VDS host over safe SSH and launch Craft container stack
    #[command(alias = "docker-setup", alias = "vds-setup")]
    SetupDocker {
        /// Target remote host alias (e.g. 'saga' from ~/.ssh/config) or user@host
        target: String,
        /// Custom remote deployment directory (default: /opt/craft or ~/craft-deploy)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClusterCommands {
    /// Create a new multi-server cluster definition
    Create {
        /// Unique cluster name
        name: String,
        /// Primary proxy server name (optional)
        #[arg(long)]
        proxy: Option<String>,
    },
    /// Add a server to an existing cluster
    Add {
        /// Target cluster name
        cluster: String,
        /// Server name
        server: String,
        /// Cluster role: backend, proxy, or lobby
        #[arg(long, default_value = "backend")]
        role: String,
        /// Optional remote host alias if hosted on a remote node
        #[arg(long)]
        remote: Option<String>,
        /// Comma-separated list of servers this server depends on
        #[arg(long, value_delimiter = ',')]
        depends_on: Vec<String>,
    },
    /// Remove a server from a cluster
    Remove {
        /// Target cluster name
        cluster: String,
        /// Server name to remove
        server: String,
    },
    /// List all configured clusters and their node topologies
    #[command(alias = "list")]
    Ls,
    /// Display status and dependency DAG for a cluster
    Status {
        /// Target cluster name
        cluster: String,
    },
    /// Start all servers in a cluster according to topological DAG dependency order
    Start {
        /// Target cluster name
        cluster: String,
    },
    /// Stop all servers in a cluster in reverse dependency order (proxies first)
    Stop {
        /// Target cluster name
        cluster: String,
    },
    /// Synchronize proxy routing configuration (Velocity or BungeeCord) to route to backend nodes
    #[command(alias = "sync")]
    SyncRouting {
        /// Target cluster name
        cluster: String,
        /// Preview configuration changes without writing them to disk
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a cluster definition
    #[command(alias = "rm")]
    Delete {
        /// Target cluster name
        cluster: String,
    },
    /// Initiate a canary, blue-green, or rolling upgrade rollout for a cluster
    Rollout {
        /// Target cluster name
        cluster: String,
        /// Target software version (e.g. 1.21.1)
        version: String,
        /// Rollout strategy: canary, bluegreen, or rolling
        #[arg(long, default_value = "canary")]
        strategy: String,
        /// Canary bake duration in seconds before promoting
        #[arg(long, default_value_t = 60)]
        bake_seconds: u64,
        /// Canary traffic or node percentage (e.g. 25)
        #[arg(long, default_value_t = 25)]
        percentage: u8,
        /// Maximum parallel nodes for rolling upgrade
        #[arg(long, default_value_t = 1)]
        max_parallel: usize,
    },
    /// View the active rollout progress and live canary bake status for a cluster
    #[command(alias = "rollout_status")]
    RolloutStatus {
        /// Target cluster name
        cluster: String,
    },
    /// Abort an in-progress rollout and restore nodes to pre-rollout snapshots
    Rollback {
        /// Target cluster name
        cluster: String,
        /// Reason for aborting and triggering rollback
        #[arg(long, default_value = "Manual operator abort")]
        reason: String,
    },
    /// Trigger an autonomous fleet healing action on a degraded or failed cluster node
    Heal {
        /// Target cluster name
        cluster: String,
        /// Node ID to heal
        #[arg(long)]
        node: String,
        /// Healing action: restart, rollback, drain, or promote
        #[arg(long, default_value = "restart")]
        action: String,
        /// Optional snapshot identifier for rollback action
        #[arg(long)]
        snapshot: Option<String>,
        /// Reason for healing action
        #[arg(long, default_value = "Manual operator intervention")]
        reason: String,
    },
    /// Query real-time fleet health across all nodes in a cluster
    #[command(alias = "fleet")]
    FleetStatus {
        /// Target cluster name
        cluster: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DeployCommands {
    /// Setup Docker on a remote VDS via safe SSH and deploy Craft container stack
    Vds {
        /// Remote host alias (e.g. 'saga' from ~/.ssh/config) or user@host
        target: String,
        /// Custom remote deployment directory (default: /opt/craft or ~/craft-deploy)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Deploy and start the containerized Craft daemon and servers
    Up {
        /// Run containers in background (detached)
        #[arg(short, long, default_value_t = true)]
        detach: bool,
        /// Rebuild Docker image before starting
        #[arg(long)]
        build: bool,
    },
    /// Stop and remove containerized Craft stack
    Down {
        /// Remove named volumes as well
        #[arg(short, long)]
        volumes: bool,
    },
    /// Show status, health, and port bindings of Craft container
    Status,
    /// View or stream live logs from Craft container
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show from the end of the logs
        #[arg(short = 'n', long)]
        tail: Option<String>,
    },
    /// Execute a Craft command inside the running container
    Exec {
        /// Command and arguments to execute
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Initialize or generate Dockerfile and docker-compose.yml in the current directory
    Init {
        /// Overwrite existing files if present
        #[arg(short, long)]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manage_flags_parsing() {
        let cli = Cli::try_parse_from(["craft", "manage", "--remote-node", "saga"]).unwrap();
        match cli.command {
            Some(Commands::Manage {
                remote_node,
                remote,
            }) => {
                assert_eq!(remote_node.as_deref(), Some("saga"));
                assert_eq!(remote, None);
            }
            _ => panic!("Expected Manage command"),
        }

        let cli_ui = Cli::try_parse_from(["craft", "ui", "--remote", "saga"]).unwrap();
        match cli_ui.command {
            Some(Commands::Manage {
                remote_node,
                remote,
            }) => {
                assert_eq!(remote_node, None);
                assert_eq!(remote.as_deref(), Some("saga"));
            }
            _ => panic!("Expected Manage command"),
        }
    }

    #[test]
    fn test_content_commands_parsing() {
        // Plugin install
        let cli_p =
            Cli::try_parse_from(["craft", "plugin", "install", "luckperms", "myserver"]).unwrap();
        match cli_p.command {
            Some(Commands::Plugin {
                action:
                    Some(PluginCommands::Install {
                        project_id,
                        server,
                        ..
                    }),
            }) => {
                assert_eq!(project_id, "luckperms");
                assert_eq!(server, "myserver");
            }
            _ => panic!("Expected Plugin Install command"),
        }

        // Mod install
        let cli_m =
            Cli::try_parse_from(["craft", "mod", "install", "fabric-api", "mymodded"]).unwrap();
        match cli_m.command {
            Some(Commands::Mod {
                action:
                    Some(ModCommands::Install {
                        project_id,
                        server,
                        ..
                    }),
            }) => {
                assert_eq!(project_id, "fabric-api");
                assert_eq!(server, "mymodded");
            }
            _ => panic!("Expected Mod Install command"),
        }

        // Datapack install
        let cli_d = Cli::try_parse_from([
            "craft",
            "datapack",
            "install",
            "terralith",
            "myserver",
            "--world",
            "custom_world",
        ])
        .unwrap();
        match cli_d.command {
            Some(Commands::Datapack {
                action:
                    Some(DatapackCommands::Install {
                        project_id,
                        server,
                        world,
                    }),
            }) => {
                assert_eq!(project_id, "terralith");
                assert_eq!(server, "myserver");
                assert_eq!(world.as_deref(), Some("custom_world"));
            }
            _ => panic!("Expected Datapack Install command"),
        }
    }

    #[test]
    fn test_catalog_commands_parsing() {
        let cli_build =
            Cli::try_parse_from(["craft", "catalog", "build", "--output", "/tmp/versions.zst"])
                .unwrap();
        match cli_build.command {
            Some(Commands::Catalog {
                action: Some(CatalogCommands::Build { output }),
            }) => {
                assert_eq!(output, PathBuf::from("/tmp/versions.zst"));
            }
            _ => panic!("Expected Catalog Build command"),
        }

        let cli_update = Cli::try_parse_from([
            "craft",
            "catalog",
            "update",
            "--url",
            "https://example.com/versions.zst",
        ])
        .unwrap();
        match cli_update.command {
            Some(Commands::Catalog {
                action: Some(CatalogCommands::Update { url }),
            }) => {
                assert_eq!(url.as_deref(), Some("https://example.com/versions.zst"));
            }
            _ => panic!("Expected Catalog Update command"),
        }

        let cli_list = Cli::try_parse_from(["craft", "catalog", "list", "paper"]).unwrap();
        match cli_list.command {
            Some(Commands::Catalog {
                action: Some(CatalogCommands::List { software }),
            }) => {
                assert_eq!(software.as_deref(), Some("paper"));
            }
            _ => panic!("Expected Catalog List command"),
        }

        let cli_info = Cli::try_parse_from(["craft", "catalog", "info"]).unwrap();
        match cli_info.command {
            Some(Commands::Catalog {
                action: Some(CatalogCommands::Info),
            }) => {}
            _ => panic!("Expected Catalog Info command"),
        }
    }

    #[test]
    fn test_forecast_cli_parsing() {
        let cli_show = Cli::try_parse_from(["craft", "forecast", "show", "survival", "--horizon", "48", "--json"]).unwrap();
        match cli_show.command {
            Some(Commands::Forecast {
                action: Some(ForecastCommands::Show { server, horizon, json, .. }),
                ..
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(horizon, 48);
                assert!(json);
            }
            _ => panic!("Expected Forecast Show command"),
        }

        let cli_cost = Cli::try_parse_from(["craft", "forecast", "cost", "--server", "lobby"]).unwrap();
        match cli_cost.command {
            Some(Commands::Forecast {
                action: Some(ForecastCommands::Cost { server, .. }),
                ..
            }) => {
                assert_eq!(server.as_deref(), Some("lobby"));
            }
            _ => panic!("Expected Forecast Cost command"),
        }

        let cli_schedule = Cli::try_parse_from(["craft", "forecast", "schedule", "survival", "--lead-mins", "30", "--quiet-start", "2", "--quiet-end", "7"]).unwrap();
        match cli_schedule.command {
            Some(Commands::Forecast {
                action: Some(ForecastCommands::Schedule { server, lead_mins, quiet_start, quiet_end, .. }),
                ..
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(lead_mins, Some(30));
                assert_eq!(quiet_start, Some(2));
                assert_eq!(quiet_end, Some(7));
            }
            _ => panic!("Expected Forecast Schedule command"),
        }

        let cli_opt = Cli::try_parse_from(["craft", "forecast", "optimize", "survival"]).unwrap();
        match cli_opt.command {
            Some(Commands::Forecast {
                action: Some(ForecastCommands::Optimize { server, .. }),
                ..
            }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected Forecast Optimize command"),
        }
    }

    #[test]
    fn test_remote_setup_docker_parsing() {
        let cli = Cli::try_parse_from(["craft", "remote", "setup-docker", "saga"]).unwrap();
        match cli.command {
            Some(Commands::Remote {
                action: Some(RemoteCommands::SetupDocker { target, dir }),
            }) => {
                assert_eq!(target, "saga");
                assert_eq!(dir, None);
            }
            _ => panic!("Expected Remote SetupDocker command"),
        }

        let cli_vds =
            Cli::try_parse_from(["craft", "deploy", "vds", "saga", "--dir", "/opt/craft"]).unwrap();
        match cli_vds.command {
            Some(Commands::Deploy {
                action: Some(DeployCommands::Vds { target, dir }),
            }) => {
                assert_eq!(target, "saga");
                assert_eq!(dir, Some(PathBuf::from("/opt/craft")));
            }
            _ => panic!("Expected Deploy Vds command"),
        }
    }

    #[test]
    fn test_log_commands_parsing() {
        let cli_export = Cli::try_parse_from([
            "craft",
            "log",
            "export",
            "paper-server",
            "--format",
            "zstd",
        ])
        .unwrap();
        match cli_export.command {
            Some(Commands::Log {
                action: Some(LogCommands::Export { name, format, .. }),
                ..
            }) => {
                assert_eq!(name, "paper-server");
                assert_eq!(format, "zstd");
            }
            _ => panic!("Expected Log Export command"),
        }

        let cli_view = Cli::try_parse_from(["craft", "log", "view", "paper-server"]).unwrap();
        match cli_view.command {
            Some(Commands::Log {
                action: Some(LogCommands::View { name, .. }),
                ..
            }) => {
                assert_eq!(name, "paper-server");
            }
            _ => panic!("Expected Log View command"),
        }

        let cli_direct = Cli::try_parse_from(["craft", "log", "paper-server"]).unwrap();
        match cli_direct.command {
            Some(Commands::Log { name, action, .. }) => {
                assert_eq!(name, "paper-server");
                assert!(action.is_none());
            }
            _ => panic!("Expected direct Log command with name"),
        }
    }

    #[test]
    fn test_migrate_parsing() {
        let cli = Cli::try_parse_from([
            "craft",
            "migrate",
            "survival",
            "--to",
            "hetzner-vps",
            "--remote-name",
            "survival-prod",
            "--remote-port",
            "25570",
            "--trash-source",
            "--start",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Migrate {
                action: None,
                server,
                to,
                remote_name,
                remote_port,
                trash_source,
                start,
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(to, "hetzner-vps");
                assert_eq!(remote_name.as_deref(), Some("survival-prod"));
                assert_eq!(remote_port, Some(25570));
                assert!(trash_source);
                assert!(start);
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test]
    fn test_migrate_live_and_anycast_parsing() {
        let cli_live = Cli::try_parse_from([
            "craft",
            "migrate",
            "live",
            "lobby",
            "--target-node",
            "node-eu-west",
            "--target-host",
            "10.0.0.5",
            "--freeze-max-ms",
            "150",
            "--json",
        ])
        .unwrap();

        match cli_live.command {
            Some(Commands::Migrate {
                action:
                    Some(MigrateCommands::Live {
                        server,
                        target_node,
                        target_host,
                        target_port,
                        freeze_max_ms,
                        json,
                    }),
                ..
            }) => {
                assert_eq!(server, "lobby");
                assert_eq!(target_node, "node-eu-west");
                assert_eq!(target_host.as_deref(), Some("10.0.0.5"));
                assert_eq!(target_port, 25565);
                assert_eq!(freeze_max_ms, 150);
                assert!(json);
            }
            _ => panic!("Expected Migrate live command"),
        }

        let cli_status = Cli::try_parse_from(["craft", "migrate", "status", "--id", "mig-123"]).unwrap();
        match cli_status.command {
            Some(Commands::Migrate {
                action: Some(MigrateCommands::Status { id, json }),
                ..
            }) => {
                assert_eq!(id.as_deref(), Some("mig-123"));
                assert!(!json);
            }
            _ => panic!("Expected Migrate status command"),
        }

        let cli_abort = Cli::try_parse_from(["craft", "migrate", "abort", "--id", "mig-123", "--reason", "timeout"]).unwrap();
        match cli_abort.command {
            Some(Commands::Migrate {
                action: Some(MigrateCommands::Abort { id, reason, json }),
                ..
            }) => {
                assert_eq!(id, "mig-123");
                assert_eq!(reason.as_deref(), Some("timeout"));
                assert!(!json);
            }
            _ => panic!("Expected Migrate abort command"),
        }

        let cli_anycast = Cli::try_parse_from([
            "craft",
            "anycast",
            "route",
            "announce",
            "--prefix",
            "198.51.100.0/24",
            "--asn",
            "65001",
        ])
        .unwrap();

        match cli_anycast.command {
            Some(Commands::Anycast {
                action: AnycastCommands::Route { action, prefix, asn, json },
            }) => {
                assert_eq!(action, "announce");
                assert_eq!(prefix.as_deref(), Some("198.51.100.0/24"));
                assert_eq!(asn, Some(65001));
                assert!(!json);
            }
            _ => panic!("Expected Anycast route command"),
        }
    }

    #[test]
    fn test_cluster_parsing() {
        let cli_create =
            Cli::try_parse_from(["craft", "cluster", "create", "network", "--proxy", "velocity"])
                .unwrap();
        match cli_create.command {
            Some(Commands::Cluster {
                action: Some(ClusterCommands::Create { name, proxy }),
            }) => {
                assert_eq!(name, "network");
                assert_eq!(proxy.as_deref(), Some("velocity"));
            }
            _ => panic!("Expected Cluster Create command"),
        }

        let cli_add = Cli::try_parse_from([
            "craft",
            "cluster",
            "add",
            "network",
            "minigames",
            "--role",
            "backend",
            "--depends-on",
            "lobby,database",
        ])
        .unwrap();
        match cli_add.command {
            Some(Commands::Cluster {
                action:
                    Some(ClusterCommands::Add {
                        cluster,
                        server,
                        role,
                        depends_on,
                        ..
                    }),
            }) => {
                assert_eq!(cluster, "network");
                assert_eq!(server, "minigames");
                assert_eq!(role, "backend");
                assert_eq!(depends_on, vec!["lobby", "database"]);
            }
            _ => panic!("Expected Cluster Add command"),
        }

        let cli_sync =
            Cli::try_parse_from(["craft", "cluster", "sync-routing", "network", "--dry-run"])
                .unwrap();
        match cli_sync.command {
            Some(Commands::Cluster {
                action: Some(ClusterCommands::SyncRouting { cluster, dry_run }),
            }) => {
                assert_eq!(cluster, "network");
                assert!(dry_run);
            }
            _ => panic!("Expected Cluster SyncRouting command"),
        }
    }

    #[test]
    fn test_plugin_commands_parsing() {
        let cli_search = Cli::try_parse_from([
            "craft",
            "plugin",
            "search",
            "essentials",
            "--game-version",
            "1.21.1",
            "--loader",
            "paper",
        ])
        .unwrap();
        match cli_search.command {
            Some(Commands::Plugin {
                action:
                    Some(PluginCommands::Search {
                        query,
                        game_version,
                        loader,
                    }),
            }) => {
                assert_eq!(query, "essentials");
                assert_eq!(game_version.as_deref(), Some("1.21.1"));
                assert_eq!(loader.as_deref(), Some("paper"));
            }
            _ => panic!("Expected Plugin Search command"),
        }

        let cli_update =
            Cli::try_parse_from(["craft", "plugin", "update", "lobby", "--check"]).unwrap();
        match cli_update.command {
            Some(Commands::Plugin {
                action: Some(PluginCommands::Update { server, check, yes }),
            }) => {
                assert_eq!(server, "lobby");
                assert!(check);
                assert!(!yes);
            }
            _ => panic!("Expected Plugin Update command"),
        }

        let cli_inspect =
            Cli::try_parse_from(["craft", "plugin", "inspect", "/tmp/Vault.jar"]).unwrap();
        match cli_inspect.command {
            Some(Commands::Plugin {
                action: Some(PluginCommands::Inspect { file }),
            }) => {
                assert_eq!(file, PathBuf::from("/tmp/Vault.jar"));
            }
            _ => panic!("Expected Plugin Inspect command"),
        }

        let cli_doctor =
            Cli::try_parse_from(["craft", "plugin", "doctor", "survival"]).unwrap();
        match cli_doctor.command {
            Some(Commands::Plugin {
                action: Some(PluginCommands::Doctor { server }),
            }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected Plugin Doctor command"),
        }
    }

    #[test]
    fn test_webhook_and_gateway_cli_parsing() {
        let cli_wh_add = Cli::try_parse_from([
            "craft", "webhook", "add", "alerts", "https://discord.com/api/webhooks/1/2",
            "--kind", "discord", "--events", "crash,circuit_trip", "--secret", "mysecret"
        ]).unwrap();
        match cli_wh_add.command {
            Some(Commands::Webhook {
                action: WebhookCommands::Add { name, url, kind, events, secret },
            }) => {
                assert_eq!(name, "alerts");
                assert_eq!(url, "https://discord.com/api/webhooks/1/2");
                assert_eq!(kind, "discord");
                assert_eq!(events, vec!["crash".to_string(), "circuit_trip".to_string()]);
                assert_eq!(secret, Some("mysecret".to_string()));
            }
            _ => panic!("Expected Webhook Add command"),
        }

        let cli_wh_ls = Cli::try_parse_from(["craft", "webhook", "ls"]).unwrap();
        match cli_wh_ls.command {
            Some(Commands::Webhook { action: WebhookCommands::List }) => {}
            _ => panic!("Expected Webhook List command"),
        }

        let cli_gw_status = Cli::try_parse_from(["craft", "gateway", "status"]).unwrap();
        match cli_gw_status.command {
            Some(Commands::Gateway { action: GatewayCommands::Status }) => {}
            _ => panic!("Expected Gateway Status command"),
        }

        let cli_gw_metrics = Cli::try_parse_from(["craft", "gateway", "metrics", "--raw"]).unwrap();
        match cli_gw_metrics.command {
            Some(Commands::Gateway { action: GatewayCommands::Metrics { raw } }) => {
                assert!(raw);
            }
            _ => panic!("Expected Gateway Metrics command"),
        }
    }

    #[test]
    fn test_phase7_cli_parsing() {
        let cli_opt = Cli::try_parse_from(["craft", "optimize", "survival", "--profile", "aggressive", "--apply"]).unwrap();
        match cli_opt.command {
            Some(Commands::Optimize { name, profile, apply }) => {
                assert_eq!(name, "survival");
                assert_eq!(profile, "aggressive");
                assert!(apply);
            }
            _ => panic!("Expected Optimize command"),
        }

        let cli_mp_inspect = Cli::try_parse_from(["craft", "modpack", "inspect", "pack.mrpack"]).unwrap();
        match cli_mp_inspect.command {
            Some(Commands::Modpack { action: ModpackCommands::Inspect { archive } }) => {
                assert_eq!(archive, PathBuf::from("pack.mrpack"));
            }
            _ => panic!("Expected Modpack Inspect command"),
        }

        let cli_mp_install = Cli::try_parse_from(["craft", "modpack", "install", "lobby", "pack.mrpack", "--no-cache"]).unwrap();
        match cli_mp_install.command {
            Some(Commands::Modpack { action: ModpackCommands::Install { server, archive, no_cache } }) => {
                assert_eq!(server, "lobby");
                assert_eq!(archive, PathBuf::from("pack.mrpack"));
                assert!(no_cache);
            }
            _ => panic!("Expected Modpack Install command"),
        }

        let cli_scale = Cli::try_parse_from(["craft", "autoscale", "lobby", "--enable", "--idle-timeout", "30"]).unwrap();
        match cli_scale.command {
            Some(Commands::Autoscale { name, enable, disable, idle_timeout, motd, status }) => {
                assert_eq!(name, "lobby");
                assert!(enable);
                assert!(!disable);
                assert_eq!(idle_timeout, Some(30));
                assert_eq!(motd, None);
                assert!(!status);
            }
            _ => panic!("Expected Autoscale command"),
        }

        let cli_hibernate = Cli::try_parse_from(["craft", "hibernate", "survival", "--wake"]).unwrap();
        match cli_hibernate.command {
            Some(Commands::Hibernate { name, wake }) => {
                assert_eq!(name, "survival");
                assert!(wake);
            }
            _ => panic!("Expected Hibernate command"),
        }
    }

    #[test]
    fn test_phase8_cli_parsing() {
        let cli_user_add = Cli::try_parse_from([
            "craft", "user", "add", "alice", "--role", "ServerOperator",
            "--password", "s3cret", "--servers", "survival,creative"
        ]).unwrap();
        match cli_user_add.command {
            Some(Commands::User {
                action: UserCommands::Add {
                    username,
                    role,
                    password,
                    servers,
                },
            }) => {
                assert_eq!(username, "alice");
                assert_eq!(role, "ServerOperator");
                assert_eq!(password, Some("s3cret".to_string()));
                assert_eq!(servers, vec!["survival".to_string(), "creative".to_string()]);
            }
            _ => panic!("Expected User Add command"),
        }

        let cli_user_ls = Cli::try_parse_from(["craft", "user", "ls"]).unwrap();
        match cli_user_ls.command {
            Some(Commands::User { action: UserCommands::Ls }) => {}
            _ => panic!("Expected User Ls command"),
        }

        let cli_user_rm = Cli::try_parse_from(["craft", "user", "rm", "bob"]).unwrap();
        match cli_user_rm.command {
            Some(Commands::User { action: UserCommands::Rm { username } }) => {
                assert_eq!(username, "bob");
            }
            _ => panic!("Expected User Rm command"),
        }

        let cli_user_passwd = Cli::try_parse_from(["craft", "user", "passwd", "charlie", "--password", "newpass"]).unwrap();
        match cli_user_passwd.command {
            Some(Commands::User { action: UserCommands::Passwd { username, password } }) => {
                assert_eq!(username, "charlie");
                assert_eq!(password, Some("newpass".to_string()));
            }
            _ => panic!("Expected User Passwd command"),
        }

        let cli_audit_ls = Cli::try_parse_from(["craft", "audit", "ls", "--limit", "25"]).unwrap();
        match cli_audit_ls.command {
            Some(Commands::Audit { action: AuditCommands::Ls { limit } }) => {
                assert_eq!(limit, 25);
            }
            _ => panic!("Expected Audit Ls command"),
        }

        let cli_audit_verify = Cli::try_parse_from(["craft", "audit", "verify"]).unwrap();
        match cli_audit_verify.command {
            Some(Commands::Audit { action: AuditCommands::Verify }) => {}
            _ => panic!("Expected Audit Verify command"),
        }
    }

    #[test]
    fn test_phase9_cli_parsing() {
        let cli_dr_plan = Cli::try_parse_from(["craft", "dr", "plan", "survival"]).unwrap();
        match cli_dr_plan.command {
            Some(Commands::Dr { action: DrCommands::Plan { server } }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected Dr Plan command"),
        }

        let cli_dr_test = Cli::try_parse_from(["craft", "dr", "test", "survival"]).unwrap();
        match cli_dr_test.command {
            Some(Commands::Dr { action: DrCommands::Test { server } }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected Dr Test command"),
        }

        let cli_dr_failover = Cli::try_parse_from(["craft", "dr", "failover", "survival", "--target", "node2", "--live"]).unwrap();
        match cli_dr_failover.command {
            Some(Commands::Dr { action: DrCommands::Failover { server, target, live } }) => {
                assert_eq!(server, "survival");
                assert_eq!(target, "node2");
                assert!(live);
            }
            _ => panic!("Expected Dr Failover command"),
        }

        let cli_dr_status = Cli::try_parse_from(["craft", "dr", "status"]).unwrap();
        match cli_dr_status.command {
            Some(Commands::Dr { action: DrCommands::Status }) => {}
            _ => panic!("Expected Dr Status command"),
        }

        let cli_dr_verify = Cli::try_parse_from(["craft", "dr", "verify", "survival", "--sample", "15.0"]).unwrap();
        match cli_dr_verify.command {
            Some(Commands::Dr { action: DrCommands::Verify { server, sample } }) => {
                assert_eq!(server, "survival");
                assert_eq!(sample, 15.0);
            }
            _ => panic!("Expected Dr Verify command"),
        }

        let cli_mesh_ls = Cli::try_parse_from(["craft", "mesh", "ls"]).unwrap();
        match cli_mesh_ls.command {
            Some(Commands::Mesh { action: MeshCommands::Ls }) => {}
            _ => panic!("Expected Mesh Ls command"),
        }

        let cli_mesh_add = Cli::try_parse_from([
            "craft", "mesh", "add", "r2-us", "Cloudflare R2",
            "--kind", "r2", "--bucket-or-path", "craft-backups",
            "--endpoint", "https://r2.example.com", "--priority", "5"
        ]).unwrap();
        match cli_mesh_add.command {
            Some(Commands::Mesh {
                action: MeshCommands::Add {
                    id,
                    name,
                    kind,
                    bucket_or_path,
                    endpoint,
                    priority,
                    ..
                },
            }) => {
                assert_eq!(id, "r2-us");
                assert_eq!(name, "Cloudflare R2");
                assert_eq!(kind, "r2");
                assert_eq!(bucket_or_path, "craft-backups");
                assert_eq!(endpoint, Some("https://r2.example.com".to_string()));
                assert_eq!(priority, 5);
            }
            _ => panic!("Expected Mesh Add command"),
        }

        let cli_mesh_rm = Cli::try_parse_from(["craft", "mesh", "rm", "r2-us"]).unwrap();
        match cli_mesh_rm.command {
            Some(Commands::Mesh { action: MeshCommands::Rm { id } }) => {
                assert_eq!(id, "r2-us");
            }
            _ => panic!("Expected Mesh Rm command"),
        }

        let cli_mesh_sync = Cli::try_parse_from(["craft", "mesh", "sync", "survival"]).unwrap();
        match cli_mesh_sync.command {
            Some(Commands::Mesh { action: MeshCommands::Sync { server } }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected Mesh Sync command"),
        }

        let cli_mesh_health = Cli::try_parse_from(["craft", "mesh", "health"]).unwrap();
        match cli_mesh_health.command {
            Some(Commands::Mesh { action: MeshCommands::Health }) => {}
            _ => panic!("Expected Mesh Health command"),
        }
    }

    #[test]
    fn test_phase10_cli_parsing() {
        let cli_status = Cli::try_parse_from(["craft", "ai", "status"]).unwrap();
        match cli_status.command {
            Some(Commands::Ai { action: AiCommands::Status { server: None } }) => {}
            _ => panic!("Expected AI Status without server"),
        }

        let cli_status_srv = Cli::try_parse_from(["craft", "ai", "status", "survival"]).unwrap();
        match cli_status_srv.command {
            Some(Commands::Ai { action: AiCommands::Status { server: Some(s) } }) => {
                assert_eq!(s, "survival");
            }
            _ => panic!("Expected AI Status with server"),
        }

        let cli_analyze = Cli::try_parse_from(["craft", "ai", "analyze", "survival"]).unwrap();
        match cli_analyze.command {
            Some(Commands::Ai { action: AiCommands::Analyze { server } }) => {
                assert_eq!(server, "survival");
            }
            _ => panic!("Expected AI Analyze command"),
        }

        let cli_profile = Cli::try_parse_from(["craft", "ai", "profile", "survival", "--duration", "45"]).unwrap();
        match cli_profile.command {
            Some(Commands::Ai { action: AiCommands::Profile { server, duration } }) => {
                assert_eq!(server, "survival");
                assert_eq!(duration, 45);
            }
            _ => panic!("Expected AI Profile command"),
        }

        let cli_remediate = Cli::try_parse_from(["craft", "ai", "remediate", "survival", "--action", "cull", "--dry-run"]).unwrap();
        match cli_remediate.command {
            Some(Commands::Ai { action: AiCommands::Remediate { server, action, dry_run } }) => {
                assert_eq!(server, "survival");
                assert_eq!(action, "cull");
                assert!(dry_run);
            }
            _ => panic!("Expected AI Remediate command"),
        }

        let cli_policy = Cli::try_parse_from(["craft", "ai", "policy", "survival", "--mode", "autonomous", "--warn-mspt", "35.5"]).unwrap();
        match cli_policy.command {
            Some(Commands::Ai { action: AiCommands::Policy { server, mode, warn_mspt, .. } }) => {
                assert_eq!(server, "survival");
                assert_eq!(mode, Some("autonomous".to_string()));
                assert_eq!(warn_mspt, Some(35.5));
            }
            _ => panic!("Expected AI Policy command"),
        }
    }

    #[test]
    fn test_phase11_cli_parsing() {
        let cli_status = Cli::try_parse_from(["craft", "edge", "status"]).unwrap();
        match cli_status.command {
            Some(Commands::Edge { action: EdgeCommands::Status }) => {}
            _ => panic!("Expected Edge Status command"),
        }

        let cli_add = Cli::try_parse_from([
            "craft", "edge", "add", "edge-us-east",
            "--region", "us-east",
            "--endpoint", "198.51.100.1:25565",
            "--weight", "150",
            "--tags", "primary,east",
        ])
        .unwrap();
        match cli_add.command {
            Some(Commands::Edge {
                action: EdgeCommands::Add {
                    name,
                    region,
                    endpoint,
                    weight,
                    tags,
                },
            }) => {
                assert_eq!(name, "edge-us-east");
                assert_eq!(region, "us-east");
                assert_eq!(endpoint, "198.51.100.1:25565");
                assert_eq!(weight, 150);
                assert_eq!(tags, vec!["primary", "east"]);
            }
            _ => panic!("Expected Edge Add command"),
        }

        let cli_probe = Cli::try_parse_from(["craft", "edge", "probe", "--count", "5"]).unwrap();
        match cli_probe.command {
            Some(Commands::Edge { action: EdgeCommands::Probe { count, .. } }) => {
                assert_eq!(count, 5);
            }
            _ => panic!("Expected Edge Probe command"),
        }

        let cli_sync = Cli::try_parse_from(["craft", "edge", "sync-routing", "--dry-run"]).unwrap();
        match cli_sync.command {
            Some(Commands::Edge { action: EdgeCommands::SyncRouting { dry_run, .. } }) => {
                assert!(dry_run);
            }
            _ => panic!("Expected Edge SyncRouting command"),
        }

        let cli_handoff = Cli::try_parse_from([
            "craft", "edge", "handoff", "Player1",
            "--from", "lobby-1",
            "--to", "survival-1",
            "--ttl", "60",
        ])
        .unwrap();
        match cli_handoff.command {
            Some(Commands::Edge {
                action: EdgeCommands::Handoff {
                    player,
                    from,
                    to,
                    ttl,
                    ..
                },
            }) => {
                assert_eq!(player, "Player1");
                assert_eq!(from, "lobby-1");
                assert_eq!(to, "survival-1");
                assert_eq!(ttl, 60);
            }
            _ => panic!("Expected Edge Handoff command"),
        }

        let cli_opt = Cli::try_parse_from([
            "craft", "edge", "optimize", "survival-1",
            "--preset", "performance",
            "--dry-run",
        ])
        .unwrap();
        match cli_opt.command {
            Some(Commands::Edge {
                action: EdgeCommands::Optimize {
                    server,
                    preset,
                    dry_run,
                },
            }) => {
                assert_eq!(server, "survival-1");
                assert_eq!(preset, "performance");
                assert!(dry_run);
            }
            _ => panic!("Expected Edge Optimize command"),
        }
    }

    #[test]
    fn test_modpack_cli_parsing() {
        let cli_build = Cli::try_parse_from([
            "craft", "modpack", "build", "/tmp/server",
            "--name", "speedcraft",
            "--version", "1.2.0",
            "--loader", "fabric",
            "--mc-version", "1.20.4",
            "--target", "both",
            "--json",
        ])
        .unwrap();

        match cli_build.command {
            Some(Commands::Modpack {
                action: ModpackCommands::Build {
                    dir,
                    name,
                    version,
                    loader,
                    mc_version,
                    target,
                    json,
                    ..
                },
            }) => {
                assert_eq!(dir, PathBuf::from("/tmp/server"));
                assert_eq!(name, Some("speedcraft".to_string()));
                assert_eq!(version, "1.2.0");
                assert_eq!(loader, "fabric");
                assert_eq!(mc_version, "1.20.4");
                assert_eq!(target, "both");
                assert!(json);
            }
            _ => panic!("Expected Modpack Build command"),
        }

        let cli_delta = Cli::try_parse_from([
            "craft", "modpack", "delta", "/tmp/v1.tar.zst", "/tmp/v2.tar.zst",
            "--name", "speedcraft",
            "--src-version", "1.0.0",
            "--target-version", "1.1.0",
            "--output", "/tmp/patch.delta",
            "--json",
        ])
        .unwrap();

        match cli_delta.command {
            Some(Commands::Modpack {
                action: ModpackCommands::Delta {
                    source,
                    target,
                    name,
                    src_version,
                    target_version,
                    output,
                    json,
                },
            }) => {
                assert_eq!(source, PathBuf::from("/tmp/v1.tar.zst"));
                assert_eq!(target, PathBuf::from("/tmp/v2.tar.zst"));
                assert_eq!(name, "speedcraft");
                assert_eq!(src_version, "1.0.0");
                assert_eq!(target_version, "1.1.0");
                assert_eq!(output, Some(PathBuf::from("/tmp/patch.delta")));
                assert!(json);
            }
            _ => panic!("Expected Modpack Delta command"),
        }

        let cli_patch = Cli::try_parse_from([
            "craft", "modpack", "patch", "/tmp/base.tar.zst", "/tmp/patch.delta",
            "--output", "/tmp/reconstructed.tar.zst",
            "--json",
        ])
        .unwrap();

        match cli_patch.command {
            Some(Commands::Modpack {
                action: ModpackCommands::Patch {
                    base,
                    patch,
                    output,
                    json,
                },
            }) => {
                assert_eq!(base, PathBuf::from("/tmp/base.tar.zst"));
                assert_eq!(patch, PathBuf::from("/tmp/patch.delta"));
                assert_eq!(output, PathBuf::from("/tmp/reconstructed.tar.zst"));
                assert!(json);
            }
            _ => panic!("Expected Modpack Patch command"),
        }

        let cli_sync = Cli::try_parse_from([
            "craft", "modpack", "sync", "speedcraft",
            "--version", "1.2.0",
            "--client-dir", "/tmp/client",
            "--json",
        ])
        .unwrap();

        match cli_sync.command {
            Some(Commands::Modpack {
                action: ModpackCommands::Sync {
                    name,
                    version,
                    client_dir,
                    json,
                },
            }) => {
                assert_eq!(name, "speedcraft");
                assert_eq!(version, Some("1.2.0".to_string()));
                assert_eq!(client_dir, PathBuf::from("/tmp/client"));
                assert!(json);
            }
            _ => panic!("Expected Modpack Sync command"),
        }

        let cli_sdn_status = Cli::try_parse_from(["craft", "sdn", "status", "--json"]).unwrap();
        match cli_sdn_status.command {
            Some(Commands::Sdn { action: SdnCommands::Status { json } }) => {
                assert!(json);
            }
            _ => panic!("Expected Sdn Status command"),
        }

        let cli_sdn_policy = Cli::try_parse_from([
            "craft", "sdn", "policy", "apply", "--default-verdict", "drop", "--json"
        ]).unwrap();
        match cli_sdn_policy.command {
            Some(Commands::Sdn {
                action: SdnCommands::Policy {
                    action,
                    default_verdict,
                    json,
                },
            }) => {
                assert_eq!(action, "apply");
                assert_eq!(default_verdict.as_deref(), Some("drop"));
                assert!(json);
            }
            _ => panic!("Expected Sdn Policy command"),
        }

        let cli_sdn_peers = Cli::try_parse_from([
            "craft", "sdn", "peers", "--zone", "backend-world"
        ]).unwrap();
        match cli_sdn_peers.command {
            Some(Commands::Sdn {
                action: SdnCommands::Peers { zone, json },
            }) => {
                assert_eq!(zone.as_deref(), Some("backend-world"));
                assert!(!json);
            }
            _ => panic!("Expected Sdn Peers command"),
        }

        let cli_sdn_rotate = Cli::try_parse_from([
            "craft", "sdn", "rotate-keys", "--node", "node-alpha"
        ]).unwrap();
        match cli_sdn_rotate.command {
            Some(Commands::Sdn {
                action: SdnCommands::RotateKeys { node, json },
            }) => {
                assert_eq!(node.as_deref(), Some("node-alpha"));
                assert!(!json);
            }
            _ => panic!("Expected Sdn RotateKeys command"),
        }

        let cli_sdn_audit = Cli::try_parse_from([
            "craft", "sdn", "audit", "--from-zone", "ingress-proxy", "--to-zone", "storage-mesh"
        ]).unwrap();
        match cli_sdn_audit.command {
            Some(Commands::Sdn {
                action: SdnCommands::Audit { from_zone, to_zone, json },
            }) => {
                assert_eq!(from_zone.as_deref(), Some("ingress-proxy"));
                assert_eq!(to_zone.as_deref(), Some("storage-mesh"));
                assert!(!json);
            }
            _ => panic!("Expected Sdn Audit command"),
        }
    }

    #[test]
    fn test_raft_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "raft", "status", "--group", "data-1", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Raft {
                action: RaftCommands::Status { group, json },
            }) => {
                assert_eq!(group, Some("data-1".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Raft Status command"),
        }

        // Consensus alias
        let cli_consensus = Cli::try_parse_from(["craft", "consensus", "status"]).unwrap();
        match cli_consensus.command {
            Some(Commands::Raft {
                action: RaftCommands::Status { group, json },
            }) => {
                assert_eq!(group, None);
                assert!(!json);
            }
            _ => panic!("Expected Raft Status via consensus alias"),
        }

        // Propose command
        let cli_propose = Cli::try_parse_from([
            "craft", "raft", "propose", "--group", "default", "--action", "config_update", "--data", "max_players=50",
        ])
        .unwrap();
        match cli_propose.command {
            Some(Commands::Raft {
                action: RaftCommands::Propose { group, action, data, json },
            }) => {
                assert_eq!(group, "default");
                assert_eq!(action, "config_update");
                assert_eq!(data, "max_players=50");
                assert!(!json);
            }
            _ => panic!("Expected Raft Propose command"),
        }

        // Reconfigure command
        let cli_reconfig = Cli::try_parse_from([
            "craft", "raft", "reconfigure", "--group", "default", "--add", "node-3", "--remove", "node-2", "--learner", "node-4", "--promote", "node-4",
        ])
        .unwrap();
        match cli_reconfig.command {
            Some(Commands::Raft {
                action: RaftCommands::Reconfigure { group, add, remove, learner, promote, demote, json },
            }) => {
                assert_eq!(group, "default");
                assert_eq!(add, vec!["node-3".to_string()]);
                assert_eq!(remove, vec!["node-2".to_string()]);
                assert_eq!(learner, vec!["node-4".to_string()]);
                assert_eq!(promote, Some("node-4".to_string()));
                assert_eq!(demote, None);
                assert!(!json);
            }
            _ => panic!("Expected Raft Reconfigure command"),
        }

        // Compact command
        let cli_compact = Cli::try_parse_from([
            "craft", "raft", "compact", "--group", "default", "--index", "100", "--force",
        ])
        .unwrap();
        match cli_compact.command {
            Some(Commands::Raft {
                action: RaftCommands::Compact { group, index, force, json },
            }) => {
                assert_eq!(group, "default");
                assert_eq!(index, Some(100));
                assert!(force);
                assert!(!json);
            }
            _ => panic!("Expected Raft Compact command"),
        }

        // Partition List & Route command
        let cli_part_list = Cli::try_parse_from(["craft", "raft", "partition", "list", "--json"]).unwrap();
        match cli_part_list.command {
            Some(Commands::Raft {
                action: RaftCommands::Partition { action: RaftPartitionCommands::List { json } },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Raft Partition List command"),
        }

        let cli_part_route = Cli::try_parse_from(["craft", "raft", "partition", "route", "server-42"]).unwrap();
        match cli_part_route.command {
            Some(Commands::Raft {
                action: RaftCommands::Partition { action: RaftPartitionCommands::Route { key, json } },
            }) => {
                assert_eq!(key, "server-42");
                assert!(!json);
            }
            _ => panic!("Expected Raft Partition Route command"),
        }

        let cli_part_create = Cli::try_parse_from([
            "craft", "raft", "partition", "create", "--group", "2", "--name", "data-shard-2",
            "--range-start", "80000000", "--range-end", "ffffffff",
        ]).unwrap();
        match cli_part_create.command {
            Some(Commands::Raft {
                action: RaftCommands::Partition { action: RaftPartitionCommands::Create { group, name, range_start, range_end, .. } },
            }) => {
                assert_eq!(group, 2);
                assert_eq!(name, "data-shard-2");
                assert_eq!(range_start, "80000000");
                assert_eq!(range_end, "ffffffff");
            }
            _ => panic!("Expected Raft Partition Create command"),
        }

        // Lock command
        let cli_lock = Cli::try_parse_from([
            "craft", "raft", "lock", "--name", "world-lock", "--holder", "worker-1", "--lease",
            "120",
        ])
        .unwrap();
        match cli_lock.command {
            Some(Commands::Raft {
                action:
                    RaftCommands::Lock {
                        name,
                        holder,
                        lease,
                        json,
                    },
            }) => {
                assert_eq!(name, "world-lock");
                assert_eq!(holder, "worker-1");
                assert_eq!(lease, 120);
                assert!(!json);
            }
            _ => panic!("Expected Raft Lock command"),
        }

        // Unlock command
        let cli_unlock = Cli::try_parse_from([
            "craft", "raft", "unlock", "--name", "world-lock", "--holder", "worker-1",
        ])
        .unwrap();
        match cli_unlock.command {
            Some(Commands::Raft {
                action: RaftCommands::Unlock { name, holder, json },
            }) => {
                assert_eq!(name, "world-lock");
                assert_eq!(holder, "worker-1");
                assert!(!json);
            }
            _ => panic!("Expected Raft Unlock command"),
        }

        // StepDown command
        let cli_stepdown = Cli::try_parse_from(["craft", "raft", "step-down"]).unwrap();
        match cli_stepdown.command {
            Some(Commands::Raft {
                action: RaftCommands::StepDown { json },
            }) => {
                assert!(!json);
            }
            _ => panic!("Expected Raft StepDown command"),
        }

        // Transfer command
        let cli_transfer =
            Cli::try_parse_from(["craft", "raft", "transfer", "--target", "node-beta"]).unwrap();
        match cli_transfer.command {
            Some(Commands::Raft {
                action: RaftCommands::Transfer { target, json },
            }) => {
                assert_eq!(target, "node-beta");
                assert!(!json);
            }
            _ => panic!("Expected Raft Transfer command"),
        }

        // Logs command
        let cli_logs = Cli::try_parse_from(["craft", "raft", "logs", "--limit", "15"]).unwrap();
        match cli_logs.command {
            Some(Commands::Raft {
                action: RaftCommands::Logs { limit, json },
            }) => {
                assert_eq!(limit, 15);
                assert!(!json);
            }
            _ => panic!("Expected Raft Logs command"),
        }
    }

    #[test]
    fn test_quota_cli_parsing() {
        // List command with alias cgroup
        let cli_list = Cli::try_parse_from(["craft", "cgroup", "list", "--tenant", "tenant-1", "--json"]).unwrap();
        match cli_list.command {
            Some(Commands::Quota {
                action: QuotaCommands::List { tenant, json },
            }) => {
                assert_eq!(tenant, Some("tenant-1".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Quota List command"),
        }

        // Set command
        let cli_set = Cli::try_parse_from([
            "craft", "quota", "set", "lobby-server", "--cpu", "150", "--memory", "2048", "--priority", "gateway",
        ]).unwrap();
        match cli_set.command {
            Some(Commands::Quota {
                action: QuotaCommands::Set { server, cpu, memory, priority, .. },
            }) => {
                assert_eq!(server, "lobby-server");
                assert_eq!(cpu, Some(150));
                assert_eq!(memory, Some(2048));
                assert_eq!(priority, Some("gateway".to_string()));
            }
            _ => panic!("Expected Quota Set command"),
        }

        // Balance command
        let cli_balance = Cli::try_parse_from(["craft", "limits", "balance", "--json"]).unwrap();
        match cli_balance.command {
            Some(Commands::Quota {
                action: QuotaCommands::Balance { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Quota Balance command"),
        }
    }

    #[test]
    fn test_trace_cli_parsing() {
        // Trace status with alias otel
        let cli_status = Cli::try_parse_from(["craft", "otel", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Trace {
                action: TraceCommands::Status { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Trace Status command"),
        }

        // Trace list with alias tracing
        let cli_list = Cli::try_parse_from([
            "craft", "tracing", "list", "--service", "craft-daemon", "--min-duration-ms", "50", "--limit", "10", "--json",
        ]).unwrap();
        match cli_list.command {
            Some(Commands::Trace {
                action: TraceCommands::List { service, min_duration_ms, limit, json },
            }) => {
                assert_eq!(service, Some("craft-daemon".to_string()));
                assert_eq!(min_duration_ms, Some(50));
                assert_eq!(limit, 10);
                assert!(json);
            }
            _ => panic!("Expected Trace List command"),
        }

        // Trace get
        let cli_get = Cli::try_parse_from([
            "craft", "trace", "get", "4bf92f3577b34da6a3ce929d0e0e4736",
        ]).unwrap();
        match cli_get.command {
            Some(Commands::Trace {
                action: TraceCommands::Get { trace_id, json },
            }) => {
                assert_eq!(trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
                assert!(!json);
            }
            _ => panic!("Expected Trace Get command"),
        }

        // Trace export
        let cli_export = Cli::try_parse_from(["craft", "trace", "export", "--json"]).unwrap();
        match cli_export.command {
            Some(Commands::Trace {
                action: TraceCommands::Export { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Trace Export command"),
        }

        // Trace config
        let cli_cfg = Cli::try_parse_from([
            "craft", "trace", "config", "--enabled", "true", "--sampler", "ratio", "--sample-ratio", "0.25", "--otlp-endpoint", "http://localhost:4318/v1/traces",
        ]).unwrap();
        match cli_cfg.command {
            Some(Commands::Trace {
                action: TraceCommands::Config { enabled, sampler, sample_ratio, otlp_endpoint, service_name, json },
            }) => {
                assert_eq!(enabled, Some(true));
                assert_eq!(sampler, Some("ratio".to_string()));
                assert_eq!(sample_ratio, Some(0.25));
                assert_eq!(otlp_endpoint, Some("http://localhost:4318/v1/traces".to_string()));
                assert_eq!(service_name, None);
                assert!(!json);
            }
            _ => panic!("Expected Trace Config command"),
        }
    }

    #[test]
    fn test_anvil_cli_parsing() {
        // Anvil status with alias mca
        let cli_status = Cli::try_parse_from(["craft", "mca", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Anvil {
                action: AnvilCommands::Status { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Anvil Status command"),
        }

        // Anvil inspect with alias chunk
        let cli_inspect = Cli::try_parse_from([
            "craft", "chunk", "inspect", "my-server", "--file", "r.0.0.mca", "--json",
        ])
        .unwrap();
        match cli_inspect.command {
            Some(Commands::Anvil {
                action: AnvilCommands::Inspect { server, file, json },
            }) => {
                assert_eq!(server, Some("my-server".to_string()));
                assert_eq!(file, "r.0.0.mca");
                assert!(json);
            }
            _ => panic!("Expected Anvil Inspect command"),
        }

        // Anvil prefetch
        let cli_prefetch = Cli::try_parse_from([
            "craft", "anvil", "prefetch", "my-server", "--world", "world_nether", "--x", "10", "--z", "-5", "--radius", "3",
        ])
        .unwrap();
        match cli_prefetch.command {
            Some(Commands::Anvil {
                action: AnvilCommands::Prefetch { server, world, x, z, radius, json },
            }) => {
                assert_eq!(server, "my-server");
                assert_eq!(world, Some("world_nether".to_string()));
                assert_eq!(x, 10);
                assert_eq!(z, -5);
                assert_eq!(radius, 3);
                assert!(!json);
            }
            _ => panic!("Expected Anvil Prefetch command"),
        }

        // Anvil bench
        let cli_bench = Cli::try_parse_from(["craft", "anvil", "bench", "--chunks", "64", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Anvil {
                action: AnvilCommands::Bench { chunks, json },
            }) => {
                assert_eq!(chunks, 64);
                assert!(json);
            }
            _ => panic!("Expected Anvil Bench command"),
        }

        // Anvil config
        let cli_config = Cli::try_parse_from([
            "craft", "anvil", "config", "--engine", "io_uring", "--cache-mb", "128", "--prefetch-radius", "6",
        ])
        .unwrap();
        match cli_config.command {
            Some(Commands::Anvil {
                action: AnvilCommands::Config { engine, cache_mb, prefetch_radius, .. },
            }) => {
                assert_eq!(engine, Some("io_uring".to_string()));
                assert_eq!(cache_mb, Some(128));
                assert_eq!(prefetch_radius, Some(6));
            }
            _ => panic!("Expected Anvil Config command"),
        }
    }

    #[test]
    fn test_numa_cli_parsing() {
        // NUMA status and aliases
        let cli_status = Cli::try_parse_from(["craft", "numa", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Numa {
                action: NumaCommands::Status { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Numa Status command"),
        }

        let cli_alias_dpdk = Cli::try_parse_from(["craft", "dpdk", "status"]).unwrap();
        assert!(matches!(
            cli_alias_dpdk.command,
            Some(Commands::Numa {
                action: NumaCommands::Status { json: false }
            })
        ));

        let cli_alias_pinning = Cli::try_parse_from(["craft", "pinning", "status"]).unwrap();
        assert!(matches!(
            cli_alias_pinning.command,
            Some(Commands::Numa {
                action: NumaCommands::Status { json: false }
            })
        ));

        // NUMA pin
        let cli_pin = Cli::try_parse_from([
            "craft", "numa", "pin", "survival-01", "--cpus", "2-5", "--node", "0", "--policy", "bind",
        ])
        .unwrap();
        match cli_pin.command {
            Some(Commands::Numa {
                action:
                    NumaCommands::Pin {
                        server,
                        cpus,
                        node,
                        policy,
                        json,
                    },
            }) => {
                assert_eq!(server, "survival-01");
                assert_eq!(cpus, "2-5");
                assert_eq!(node, Some(0));
                assert_eq!(policy, "bind");
                assert!(!json);
            }
            _ => panic!("Expected Numa Pin command"),
        }

        // NUMA policy
        let cli_policy = Cli::try_parse_from([
            "craft", "numa", "policy", "survival-01", "--policy", "interleave", "--node", "1",
        ])
        .unwrap();
        match cli_policy.command {
            Some(Commands::Numa {
                action:
                    NumaCommands::Policy {
                        server,
                        policy,
                        node,
                        json,
                    },
            }) => {
                assert_eq!(server, "survival-01");
                assert_eq!(policy, "interleave");
                assert_eq!(node, Some(1));
                assert!(!json);
            }
            _ => panic!("Expected Numa Policy command"),
        }

        // NUMA bench
        let cli_bench = Cli::try_parse_from(["craft", "numa", "bench", "--node", "0", "--size-mb", "32", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Numa {
                action: NumaCommands::Bench { node, size_mb, json },
            }) => {
                assert_eq!(node, 0);
                assert_eq!(size_mb, 32);
                assert!(json);
            }
            _ => panic!("Expected Numa Bench command"),
        }

        // NUMA boot-args
        let cli_boot = Cli::try_parse_from([
            "craft", "numa", "boot-args", "--cores", "4-15", "--hugepages-1g", "8", "--hugepages-2m", "1024",
        ])
        .unwrap();
        match cli_boot.command {
            Some(Commands::Numa {
                action:
                    NumaCommands::BootArgs {
                        cores,
                        hugepages_1g,
                        hugepages_2m,
                        json,
                    },
            }) => {
                assert_eq!(cores, "4-15");
                assert_eq!(hugepages_1g, Some(8));
                assert_eq!(hugepages_2m, Some(1024));
                assert!(!json);
            }
            _ => panic!("Expected Numa BootArgs command"),
        }
    }

    #[test]
    fn test_pqc_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "pqc", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Pqc {
                action: Some(PqcCommands::Status { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Pqc Status command"),
        }

        // Aliases test
        let cli_alias1 = Cli::try_parse_from(["craft", "post-quantum", "status"]).unwrap();
        assert!(matches!(
            cli_alias1.command,
            Some(Commands::Pqc {
                action: Some(PqcCommands::Status { json: false })
            })
        ));

        let cli_alias2 = Cli::try_parse_from(["craft", "quantum", "status"]).unwrap();
        assert!(matches!(
            cli_alias2.command,
            Some(Commands::Pqc {
                action: Some(PqcCommands::Status { json: false })
            })
        ));

        let cli_alias3 = Cli::try_parse_from(["craft", "mlkem", "status"]).unwrap();
        assert!(matches!(
            cli_alias3.command,
            Some(Commands::Pqc {
                action: Some(PqcCommands::Status { json: false })
            })
        ));

        // Policy command
        let cli_policy = Cli::try_parse_from([
            "craft",
            "pqc",
            "policy",
            "--set-mode",
            "post_quantum_only",
            "--ciphersuite",
            "pure_mlkem1024",
            "--allow-fallback",
            "false",
        ])
        .unwrap();
        match cli_policy.command {
            Some(Commands::Pqc {
                action:
                    Some(PqcCommands::Policy {
                        set_mode,
                        ciphersuite,
                        allow_fallback,
                        json,
                    }),
            }) => {
                assert_eq!(set_mode, Some("post_quantum_only".to_string()));
                assert_eq!(ciphersuite, Some("pure_mlkem1024".to_string()));
                assert_eq!(allow_fallback, Some(false));
                assert!(!json);
            }
            _ => panic!("Expected Pqc Policy command"),
        }

        // Keygen command
        let cli_keygen =
            Cli::try_parse_from(["craft", "pqc", "keygen", "--suite", "mlkem768", "--json"])
                .unwrap();
        match cli_keygen.command {
            Some(Commands::Pqc {
                action: Some(PqcCommands::Keygen { suite, algo, json }),
            }) => {
                assert_eq!(suite, Some("mlkem768".to_string()));
                assert_eq!(algo, None);
                assert!(json);
            }
            _ => panic!("Expected Pqc Keygen command"),
        }

        // Bench command
        let cli_bench =
            Cli::try_parse_from(["craft", "pqc", "bench", "--iterations", "25"]).unwrap();
        match cli_bench.command {
            Some(Commands::Pqc {
                action: Some(PqcCommands::Bench { iterations, json }),
            }) => {
                assert_eq!(iterations, 25);
                assert!(!json);
            }
            _ => panic!("Expected Pqc Bench command"),
        }

        // Migrate command
        let cli_migrate =
            Cli::try_parse_from(["craft", "pqc", "migrate", "--phase", "enforced_pqc"]).unwrap();
        match cli_migrate.command {
            Some(Commands::Pqc {
                action: Some(PqcCommands::Migrate { phase, json }),
            }) => {
                assert_eq!(phase, "enforced_pqc");
                assert!(!json);
            }
            _ => panic!("Expected Pqc Migrate command"),
        }
    }

    #[test]
    fn test_hsm_cli_parsing_and_aliases() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "hsm", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Hsm {
                action: Some(HsmCommands::Status { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Hsm Status command"),
        }

        // Aliases test: pkcs11, tpm, enclave, zkp
        for alias in &["pkcs11", "tpm", "enclave", "zkp"] {
            let parsed = Cli::try_parse_from(["craft", alias, "status"]).unwrap();
            assert!(matches!(
                parsed.command,
                Some(Commands::Hsm {
                    action: Some(HsmCommands::Status { json: false })
                })
            ));
        }

        // Keygen command
        let cli_keygen = Cli::try_parse_from([
            "craft", "hsm", "keygen", "--label", "test-key", "--type", "ed25519", "--json",
        ])
        .unwrap();
        match cli_keygen.command {
            Some(Commands::Hsm {
                action: Some(HsmCommands::Keygen { label, key_type, json }),
            }) => {
                assert_eq!(label, "test-key");
                assert_eq!(key_type, Some("ed25519".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Hsm Keygen command"),
        }

        // Sign command
        let cli_sign = Cli::try_parse_from([
            "craft", "hsm", "sign", "--label", "test-key", "--data", "hello-world",
        ])
        .unwrap();
        match cli_sign.command {
            Some(Commands::Hsm {
                action: Some(HsmCommands::Sign { label, data, json }),
            }) => {
                assert_eq!(label, "test-key");
                assert_eq!(data, "hello-world");
                assert!(!json);
            }
            _ => panic!("Expected Hsm Sign command"),
        }

        // Attest command
        let cli_attest = Cli::try_parse_from([
            "craft", "hsm", "attest", "--pcr-mask", "7", "--json",
        ])
        .unwrap();
        match cli_attest.command {
            Some(Commands::Hsm {
                action: Some(HsmCommands::Attest { pcr_mask, nonce, json }),
            }) => {
                assert_eq!(pcr_mask, Some(7));
                assert_eq!(nonce, None);
                assert!(json);
            }
            _ => panic!("Expected Hsm Attest command"),
        }

        // ZkMember command
        let cli_zk = Cli::try_parse_from([
            "craft", "hsm", "zk-member", "--action", "prove", "--cluster", "cluster-alpha",
        ])
        .unwrap();
        match cli_zk.command {
            Some(Commands::Hsm {
                action: Some(HsmCommands::ZkMember { action, cluster, json }),
            }) => {
                assert_eq!(action, "prove");
                assert_eq!(cluster, "cluster-alpha");
                assert!(!json);
            }
            _ => panic!("Expected Hsm ZkMember command"),
        }
    }

    #[test]
    fn test_memory_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "memory", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Memory {
                action: Some(MemoryCommands::Status { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Memory Status command"),
        }

        // Aliases
        for alias in &["compaction", "hugepages", "pagepool", "thp"] {
            let cli_alias = Cli::try_parse_from(["craft", alias, "status"]).unwrap();
            assert!(matches!(
                cli_alias.command,
                Some(Commands::Memory {
                    action: Some(MemoryCommands::Status { json: false })
                })
            ));
        }

        // Compact command
        let cli_compact = Cli::try_parse_from([
            "craft", "memory", "compact", "--target-order", "9", "--json",
        ])
        .unwrap();
        match cli_compact.command {
            Some(Commands::Memory {
                action: Some(MemoryCommands::Compact { target_order, json }),
            }) => {
                assert_eq!(target_order, 9);
                assert!(json);
            }
            _ => panic!("Expected Memory Compact command"),
        }

        // Thp command
        let cli_thp = Cli::try_parse_from([
            "craft", "memory", "thp", "--mode", "always", "--defrag", "defer+madvise", "--json",
        ])
        .unwrap();
        match cli_thp.command {
            Some(Commands::Memory {
                action: Some(MemoryCommands::Thp { mode, defrag, json }),
            }) => {
                assert_eq!(mode, "always");
                assert_eq!(defrag, Some("defer+madvise".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Memory Thp command"),
        }

        // Pool command
        let cli_pool = Cli::try_parse_from([
            "craft", "memory", "pool", "--packets", "5000", "--slice-size", "2048", "--json",
        ])
        .unwrap();
        match cli_pool.command {
            Some(Commands::Memory {
                action: Some(MemoryCommands::Pool { packets, slice_size, json }),
            }) => {
                assert_eq!(packets, 5000);
                assert_eq!(slice_size, 2048);
                assert!(json);
            }
            _ => panic!("Expected Memory Pool command"),
        }
    }

    #[test]
    fn test_xdp_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "xdp", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Xdp {
                action: Some(XdpCommands::Status { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Xdp Status command"),
        }

        // Aliases
        for alias in &["xdp-firewall", "antiddos", "ddos", "bpf-xdp"] {
            let cli_alias = Cli::try_parse_from(["craft", alias, "status"]).unwrap();
            assert!(matches!(
                cli_alias.command,
                Some(Commands::Xdp {
                    action: Some(XdpCommands::Status { json: false })
                })
            ));
        }

        // Attach command
        let cli_attach = Cli::try_parse_from([
            "craft", "xdp", "attach", "eth0", "--mode", "native", "--json",
        ])
        .unwrap();
        match cli_attach.command {
            Some(Commands::Xdp {
                action: Some(XdpCommands::Attach { interface, mode, json }),
            }) => {
                assert_eq!(interface, "eth0");
                assert_eq!(mode, "native");
                assert!(json);
            }
            _ => panic!("Expected Xdp Attach command"),
        }

        // Detach command
        let cli_detach = Cli::try_parse_from(["craft", "xdp", "detach"]).unwrap();
        match cli_detach.command {
            Some(Commands::Xdp {
                action: Some(XdpCommands::Detach { json }),
            }) => {
                assert!(!json);
            }
            _ => panic!("Expected Xdp Detach command"),
        }

        // Rule Add command
        let cli_rule_add = Cli::try_parse_from([
            "craft", "xdp", "rule", "add", "block-bad-cidr",
            "--cidr", "10.0.0.0/8",
            "--port", "25565",
            "--proto", "tcp",
            "--action", "drop",
            "--rate-limit", "100",
            "--ban-seconds", "300",
            "--priority", "200",
            "--json",
        ])
        .unwrap();
        match cli_rule_add.command {
            Some(Commands::Xdp {
                action: Some(XdpCommands::Rule {
                    action: XdpRuleSubcommands::Add {
                        id,
                        cidr,
                        port,
                        proto,
                        action,
                        rate_limit,
                        ban_seconds,
                        priority,
                        json,
                    },
                }),
            }) => {
                assert_eq!(id, "block-bad-cidr");
                assert_eq!(cidr, "10.0.0.0/8");
                assert_eq!(port, Some(25565));
                assert_eq!(proto, "tcp");
                assert_eq!(action, "drop");
                assert_eq!(rate_limit, Some(100));
                assert_eq!(ban_seconds, Some(300));
                assert_eq!(priority, 200);
                assert!(json);
            }
            _ => panic!("Expected Xdp Rule Add command"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from([
            "craft", "xdp", "bench", "--packets", "50000", "--attack-ratio", "0.8", "--json",
        ])
        .unwrap();
        match cli_bench.command {
            Some(Commands::Xdp {
                action: Some(XdpCommands::Bench { packets, attack_ratio, json }),
            }) => {
                assert_eq!(packets, 50000);
                assert!((attack_ratio - 0.8).abs() < 1e-6);
                assert!(json);
            }
            _ => panic!("Expected Xdp Bench command"),
        }
    }

    #[test]
    fn test_pmu_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "pmu", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Pmu { action: Some(PmuCommands::Status { json }) }) => {
                assert!(json);
            }
            _ => panic!("Expected Pmu Status command"),
        }

        // Alias: counters status
        let cli_counters = Cli::try_parse_from(["craft", "counters", "status"]).unwrap();
        match cli_counters.command {
            Some(Commands::Pmu { action: Some(PmuCommands::Status { json }) }) => {
                assert!(!json);
            }
            _ => panic!("Expected Pmu Status command via alias 'counters'"),
        }

        // Alias: hw-counters sample
        let cli_sample = Cli::try_parse_from([
            "craft", "hw-counters", "sample", "--pid", "1234", "--rate", "250", "--json",
        ])
        .unwrap();
        match cli_sample.command {
            Some(Commands::Pmu {
                action: Some(PmuCommands::Sample { pid, rate, stop, json }),
            }) => {
                assert_eq!(pid, Some(1234));
                assert_eq!(rate, 250);
                assert!(!stop);
                assert!(json);
            }
            _ => panic!("Expected Pmu Sample command via alias 'hw-counters'"),
        }

        // Alias: cache-profile hotspots
        let cli_hotspots = Cli::try_parse_from(["craft", "cache-profile", "hotspots", "--limit", "5"]).unwrap();
        match cli_hotspots.command {
            Some(Commands::Pmu {
                action: Some(PmuCommands::Hotspots { limit, json }),
            }) => {
                assert_eq!(limit, 5);
                assert!(!json);
            }
            _ => panic!("Expected Pmu Hotspots command via alias 'cache-profile'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "pmu", "bench", "--iterations", "3000", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Pmu {
                action: Some(PmuCommands::Bench { iterations, json }),
            }) => {
                assert_eq!(iterations, 3000);
                assert!(json);
            }
            _ => panic!("Expected Pmu Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "pmu", "reset-metrics", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::Pmu { action: Some(PmuCommands::ResetMetrics { json }) }) => {
                assert!(json);
            }
            _ => panic!("Expected Pmu ResetMetrics command"),
        }
    }

    #[test]
    fn test_shm_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "shm", "status", "--server", "lobby", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Shm { action: Some(ShmCommands::Status { server, json }) }) => {
                assert_eq!(server, Some("lobby".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Shm Status command"),
        }

        // Alias: shared-memory status
        let cli_shm_alias = Cli::try_parse_from(["craft", "shared-memory", "status"]).unwrap();
        match cli_shm_alias.command {
            Some(Commands::Shm { action: Some(ShmCommands::Status { server, json }) }) => {
                assert_eq!(server, None);
                assert!(!json);
            }
            _ => panic!("Expected Shm Status command via alias 'shared-memory'"),
        }

        // Alias: shmem create
        let cli_create = Cli::try_parse_from([
            "craft", "shmem", "create", "-s", "survival", "-c", "packet_bus", "--slot-size", "8192", "--slots", "512", "--json",
        ])
        .unwrap();
        match cli_create.command {
            Some(Commands::Shm {
                action: Some(ShmCommands::Create { server, channel, slot_size, slots, json }),
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(channel, "packet_bus");
                assert_eq!(slot_size, 8192);
                assert_eq!(slots, 512);
                assert!(json);
            }
            _ => panic!("Expected Shm Create command via alias 'shmem'"),
        }

        // Alias: ipc-ring close
        let cli_close = Cli::try_parse_from(["craft", "ipc-ring", "close", "-s", "survival", "-c", "packet_bus"]).unwrap();
        match cli_close.command {
            Some(Commands::Shm {
                action: Some(ShmCommands::Close { server, channel, json }),
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(channel, "packet_bus");
                assert!(!json);
            }
            _ => panic!("Expected Shm Close command via alias 'ipc-ring'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "shm", "bench", "--messages", "10000", "--size", "256", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Shm {
                action: Some(ShmCommands::Bench { messages, size, json }),
            }) => {
                assert_eq!(messages, 10000);
                assert_eq!(size, 256);
                assert!(json);
            }
            _ => panic!("Expected Shm Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "shm", "reset-metrics", "-s", "survival", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::Shm {
                action: Some(ShmCommands::ResetMetrics { server, json }),
            }) => {
                assert_eq!(server, Some("survival".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Shm ResetMetrics command"),
        }
    }

    #[test]
    fn test_patch_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "patch", "status", "-s", "survival", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::Status { server, json }),
            }) => {
                assert_eq!(server, Some("survival".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Patch Status command"),
        }

        // Apply command
        let cli_apply = Cli::try_parse_from([
            "craft", "patch", "apply", "-s", "survival", "-p", "fix_exploit", "-t", "handle_packet", "-b", "90909090", "--json",
        ])
        .unwrap();
        match cli_apply.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::Apply { server, patch, target, bytes, json }),
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(patch, "fix_exploit");
                assert_eq!(target, "handle_packet");
                assert_eq!(bytes, "90909090");
                assert!(json);
            }
            _ => panic!("Expected Patch Apply command"),
        }

        // Rollback command via alias 'hotpatch'
        let cli_rb = Cli::try_parse_from(["craft", "hotpatch", "rollback", "-s", "survival", "-p", "fix_exploit"]).unwrap();
        match cli_rb.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::Rollback { server, patch, json }),
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(patch, "fix_exploit");
                assert!(!json);
            }
            _ => panic!("Expected Patch Rollback command via alias 'hotpatch'"),
        }

        // Diff command
        let cli_diff = Cli::try_parse_from(["craft", "patch", "diff", "-s", "survival", "-p", "fix_exploit", "--json"]).unwrap();
        match cli_diff.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::Diff { server, patch, json }),
            }) => {
                assert_eq!(server, "survival");
                assert_eq!(patch, "fix_exploit");
                assert!(json);
            }
            _ => panic!("Expected Patch Diff command"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "patch", "bench", "--iterations", "500", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::Bench { iterations, json }),
            }) => {
                assert_eq!(iterations, 500);
                assert!(json);
            }
            _ => panic!("Expected Patch Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "patch", "reset-metrics", "-s", "survival", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::Patch {
                action: Some(PatchCommands::ResetMetrics { server, json }),
            }) => {
                assert_eq!(server, Some("survival".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Patch ResetMetrics command"),
        }
    }

    #[test]
    fn test_vm_cli_parsing() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "vm", "status", "-i", "vm-101", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Vm {
                action: Some(VmCommands::Status { id, json }),
            }) => {
                assert_eq!(id, Some("vm-101".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Vm Status command"),
        }

        // Spawn command via alias 'microvm'
        let cli_spawn = Cli::try_parse_from([
            "craft", "microvm", "spawn", "-n", "sandbox-alpha", "-v", "2", "-m", "512", "-c", "4", "--devices", "net,vsock", "--json",
        ])
        .unwrap();
        match cli_spawn.command {
            Some(Commands::Vm {
                action: Some(VmCommands::Spawn { name, vcpus, memory, cid, devices, json }),
            }) => {
                assert_eq!(name, "sandbox-alpha");
                assert_eq!(vcpus, 2);
                assert_eq!(memory, 512);
                assert_eq!(cid, Some(4));
                assert_eq!(devices, Some("net,vsock".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Vm Spawn command via alias 'microvm'"),
        }

        // Stop command via alias 'firecracker'
        let cli_stop = Cli::try_parse_from(["craft", "firecracker", "stop", "-i", "vm-101", "--force"]).unwrap();
        match cli_stop.command {
            Some(Commands::Vm {
                action: Some(VmCommands::Stop { id, force, json }),
            }) => {
                assert_eq!(id, "vm-101");
                assert!(force);
                assert!(!json);
            }
            _ => panic!("Expected Vm Stop command via alias 'firecracker'"),
        }

        // Inspect command via alias 'kvm'
        let cli_inspect = Cli::try_parse_from(["craft", "kvm", "inspect", "-i", "vm-101", "--json"]).unwrap();
        match cli_inspect.command {
            Some(Commands::Vm {
                action: Some(VmCommands::Inspect { id, json }),
            }) => {
                assert_eq!(id, "vm-101");
                assert!(json);
            }
            _ => panic!("Expected Vm Inspect command via alias 'kvm'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "vm", "bench", "-c", "8", "--iterations", "50", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Vm {
                action: Some(VmCommands::Bench { concurrency, iterations, json }),
            }) => {
                assert_eq!(concurrency, 8);
                assert_eq!(iterations, 50);
                assert!(json);
            }
            _ => panic!("Expected Vm Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "vm", "reset-metrics", "-i", "vm-101", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::Vm {
                action: Some(VmCommands::ResetMetrics { id, json }),
            }) => {
                assert_eq!(id, Some("vm-101".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Vm ResetMetrics command"),
        }
    }

    #[test]
    fn test_rdma_cli_commands() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "rdma", "status", "-s", "lobby", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::Status { server, json }),
            }) => {
                assert_eq!(server, Some("lobby".to_string()));
                assert!(json);
            }
            _ => panic!("Expected Rdma Status command"),
        }

        // MrRegister command via alias 'roce'
        let cli_mr = Cli::try_parse_from(["craft", "roce", "mr-register", "--size", "1048576", "--read-only", "--json"]).unwrap();
        match cli_mr.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::MrRegister { size, read_only, atomic, json, .. }),
            }) => {
                assert_eq!(size, 1048576);
                assert!(read_only);
                assert!(!atomic);
                assert!(json);
            }
            _ => panic!("Expected Rdma MrRegister command via alias 'roce'"),
        }

        // Connect command via alias 'infiniband'
        let cli_connect = Cli::try_parse_from([
            "craft", "infiniband", "connect", "-p", "node-2", "-a", "192.168.10.2:4791", "-t", "rocev2", "-m", "4096", "--json",
        ])
        .unwrap();
        match cli_connect.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::Connect { peer, addr, transport, mtu, json }),
            }) => {
                assert_eq!(peer, "node-2");
                assert_eq!(addr, "192.168.10.2:4791");
                assert_eq!(transport, "rocev2");
                assert_eq!(mtu, 4096);
                assert!(json);
            }
            _ => panic!("Expected Rdma Connect command via alias 'infiniband'"),
        }

        // Peers command via alias 'verbs'
        let cli_peers = Cli::try_parse_from(["craft", "verbs", "peers", "--json"]).unwrap();
        match cli_peers.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::Peers { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Rdma Peers command via alias 'verbs'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "rdma", "bench", "-p", "node-2", "-i", "5000", "-s", "2048", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::Bench { peer, iterations, size, json }),
            }) => {
                assert_eq!(peer, Some("node-2".to_string()));
                assert_eq!(iterations, 5000);
                assert_eq!(size, 2048);
                assert!(json);
            }
            _ => panic!("Expected Rdma Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "rdma", "reset-metrics", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::Rdma {
                action: Some(RdmaCommands::ResetMetrics { json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Rdma ResetMetrics command"),
        }
    }

    #[test]
    fn test_smartnic_cli_commands() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "smartnic", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::Status { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected SmartNic Status command"),
        }

        // RuleAdd command via alias 'p4'
        let cli_add = Cli::try_parse_from([
            "craft", "p4", "rule-add",
            "--rule-id", "slp-rule-1",
            "--protocol", "slp",
            "--action", "pong",
            "--port", "25565",
            "--priority", "100",
            "--json",
        ])
        .unwrap();
        match cli_add.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::RuleAdd {
                    rule_id,
                    protocol,
                    action,
                    port,
                    cidr: None,
                    priority,
                    server: None,
                    json,
                }),
            }) => {
                assert_eq!(rule_id, "slp-rule-1");
                assert_eq!(protocol, "slp");
                assert_eq!(action, "pong");
                assert_eq!(port, Some(25565));
                assert_eq!(priority, 100);
                assert!(json);
            }
            _ => panic!("Expected SmartNic RuleAdd command via alias 'p4'"),
        }

        // RuleRm command
        let cli_rm = Cli::try_parse_from(["craft", "smartnic", "rule-rm", "-r", "slp-rule-1", "--json"]).unwrap();
        match cli_rm.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::RuleRm { rule_id, server: None, json }),
            }) => {
                assert_eq!(rule_id, "slp-rule-1");
                assert!(json);
            }
            _ => panic!("Expected SmartNic RuleRm command"),
        }

        // Rules command via alias 'nic-offload'
        let cli_rules = Cli::try_parse_from(["craft", "nic-offload", "rules", "--json"]).unwrap();
        match cli_rules.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::Rules { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected SmartNic Rules command via alias 'nic-offload'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from(["craft", "smartnic", "bench", "-i", "10000", "-b", "128", "--json"]).unwrap();
        match cli_bench.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::Bench { iterations, packet_size, server: None, json }),
            }) => {
                assert_eq!(iterations, 10000);
                assert_eq!(packet_size, 128);
                assert!(json);
            }
            _ => panic!("Expected SmartNic Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "smartnic", "reset-metrics", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::SmartNic {
                action: Some(SmartNicCommands::ResetMetrics { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected SmartNic ResetMetrics command"),
        }
    }

    #[test]
    fn test_memfabric_cli_commands() {
        // Status command
        let cli_status = Cli::try_parse_from(["craft", "memfabric", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::Status { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected MemFabric Status command"),
        }

        // PageAlloc command via alias 'cxl'
        let cli_alloc = Cli::try_parse_from([
            "craft", "cxl", "page-alloc",
            "--page-id", "nether-chunk-1",
            "--size", "8192",
            "--tier", "cxl",
            "--dimension", "the_nether",
            "--json",
        ]).unwrap();
        match cli_alloc.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::PageAlloc {
                    page_id,
                    size,
                    tier,
                    dimension,
                    server: None,
                    json,
                }),
            }) => {
                assert_eq!(page_id, "nether-chunk-1");
                assert_eq!(size, 8192);
                assert_eq!(tier, "cxl");
                assert_eq!(dimension.as_deref(), Some("the_nether"));
                assert!(json);
            }
            _ => panic!("Expected MemFabric PageAlloc command via alias 'cxl'"),
        }

        // EvictDim command via alias 'nvram'
        let cli_evict = Cli::try_parse_from([
            "craft", "nvram", "evict-dim",
            "--dimension", "the_nether",
            "--target-node", "nvram-node-2",
            "--json",
        ]).unwrap();
        match cli_evict.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::EvictDim {
                    dimension,
                    target_node,
                    server: None,
                    json,
                }),
            }) => {
                assert_eq!(dimension, "the_nether");
                assert_eq!(target_node.as_deref(), Some("nvram-node-2"));
                assert!(json);
            }
            _ => panic!("Expected MemFabric EvictDim command via alias 'nvram'"),
        }

        // Pages command via alias 'paging'
        let cli_pages = Cli::try_parse_from(["craft", "paging", "pages", "--json"]).unwrap();
        match cli_pages.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::Pages { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected MemFabric Pages command via alias 'paging'"),
        }

        // Bench command
        let cli_bench = Cli::try_parse_from([
            "craft", "memfabric", "bench",
            "-i", "25",
            "-z", "4096",
            "--json",
        ]).unwrap();
        match cli_bench.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::Bench {
                    iterations,
                    page_size,
                    server: None,
                    json,
                }),
            }) => {
                assert_eq!(iterations, 25);
                assert_eq!(page_size, 4096);
                assert!(json);
            }
            _ => panic!("Expected MemFabric Bench command"),
        }

        // ResetMetrics command
        let cli_reset = Cli::try_parse_from(["craft", "memfabric", "reset-metrics", "--json"]).unwrap();
        match cli_reset.command {
            Some(Commands::MemFabric {
                action: Some(MemFabricCommands::ResetMetrics { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected MemFabric ResetMetrics command"),
        }
    }

    #[test]
    fn test_nvme_cli_commands() {
        // NVMe Status command
        let cli_nvme_status = Cli::try_parse_from(["craft", "nvme", "status", "--json"]).unwrap();
        match cli_nvme_status.command {
            Some(Commands::Nvme {
                action: Some(NvmeCommands::Status { server: None, json }),
            }) => {
                assert!(json);
            }
            _ => panic!("Expected Nvme Status command"),
        }

        // NVMe NsCreate command
        let cli_nvme_create = Cli::try_parse_from([
            "craft", "nvme", "ns-create",
            "-n", "1",
            "-s", "512",
            "-b", "4096",
            "-d", "overworld",
            "--json",
        ]).unwrap();
        match cli_nvme_create.command {
            Some(Commands::Nvme {
                action: Some(NvmeCommands::NsCreate {
                    nsid,
                    size_mb,
                    block_size,
                    server: None,
                    dimension,
                    json,
                }),
            }) => {
                assert_eq!(nsid, 1);
                assert_eq!(size_mb, 512);
                assert_eq!(block_size, 4096);
                assert_eq!(dimension.as_deref(), Some("overworld"));
                assert!(json);
            }
            _ => panic!("Expected Nvme NsCreate command"),
        }

        // NVMe Bench command
        let cli_nvme_bench = Cli::try_parse_from([
            "craft", "nvme", "bench",
            "-i", "50",
            "-b", "4096",
            "--json",
        ]).unwrap();
        match cli_nvme_bench.command {
            Some(Commands::Nvme {
                action: Some(NvmeCommands::Bench {
                    iterations,
                    block_size,
                    json,
                }),
            }) => {
                assert_eq!(iterations, 50);
                assert_eq!(block_size, 4096);
                assert!(json);
            }
            _ => panic!("Expected Nvme Bench command"),
        }
    }

    #[test]
    fn test_vpn_cli_commands() {
        // VPN Status command
        let cli_vpn_status = Cli::try_parse_from(["craft", "vpn", "status", "--json"]).unwrap();
        match cli_vpn_status.command {
            Some(Commands::Vpn {
                action: Some(VpnCommands::Status { server: None, json }),
            }) => assert!(json),
            _ => panic!("Expected Vpn Status command"),
        }

        // VPN wireguard alias and tunnel-create
        let cli_wg = Cli::try_parse_from([
            "craft", "wireguard", "tunnel-create",
            "-t", "craft-wg0",
            "-a", "10.42.0.1/24",
            "-p", "51820",
            "-m", "hardware_p4",
            "--json",
        ]).unwrap();
        match cli_wg.command {
            Some(Commands::Vpn {
                action: Some(VpnCommands::TunnelCreate {
                    tunnel_id,
                    address,
                    port,
                    crypto_mode,
                    json,
                }),
            }) => {
                assert_eq!(tunnel_id, "craft-wg0");
                assert_eq!(address, "10.42.0.1/24");
                assert_eq!(port, 51820);
                assert_eq!(crypto_mode.as_deref(), Some("hardware_p4"));
                assert!(json);
            }
            _ => panic!("Expected Vpn TunnelCreate command"),
        }

        // VPN pqxdh alias and rotate-key
        let cli_pqxdh = Cli::try_parse_from([
            "craft", "pqxdh", "rotate-key",
            "-t", "craft-wg0",
            "-p", "peer-eu",
            "--json",
        ]).unwrap();
        match cli_pqxdh.command {
            Some(Commands::Vpn {
                action: Some(VpnCommands::RotateKey {
                    tunnel_id,
                    peer_id,
                    json,
                }),
            }) => {
                assert_eq!(tunnel_id, "craft-wg0");
                assert_eq!(peer_id.as_deref(), Some("peer-eu"));
                assert!(json);
            }
            _ => panic!("Expected Vpn RotateKey command"),
        }

        // VPN mesh-vpn alias and bench
        let cli_mesh = Cli::try_parse_from([
            "craft", "mesh-vpn", "bench",
            "-i", "100",
            "-s", "512",
            "--json",
        ]).unwrap();
        match cli_mesh.command {
            Some(Commands::Vpn {
                action: Some(VpnCommands::Bench {
                    iterations,
                    packet_size,
                    json,
                }),
            }) => {
                assert_eq!(iterations, 100);
                assert_eq!(packet_size, 512);
                assert!(json);
            }
            _ => panic!("Expected Vpn Bench command"),
        }
    }

    #[test]
    fn test_bft_cli_commands() {
        // BFT status
        let cli_status = Cli::try_parse_from(["craft", "bft", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Bft {
                action: Some(BftCommands::Status { server: None, json }),
            }) => assert!(json),
            _ => panic!("Expected Bft Status command"),
        }

        // BFT hotstuff alias and validators
        let cli_val = Cli::try_parse_from(["craft", "hotstuff", "validators", "--json"]).unwrap();
        match cli_val.command {
            Some(Commands::Bft {
                action: Some(BftCommands::Validators { server: None, json }),
            }) => assert!(json),
            _ => panic!("Expected Bft Validators command"),
        }

        // BFT pbft alias and validator-add
        let cli_add = Cli::try_parse_from([
            "craft", "pbft", "validator-add",
            "-i", "validator-4",
            "-w", "10",
            "-k", "0102030405",
            "-r", "Validator",
            "--json",
        ]).unwrap();
        match cli_add.command {
            Some(Commands::Bft {
                action: Some(BftCommands::ValidatorAdd {
                    id,
                    voting_weight,
                    public_key,
                    role,
                    json,
                }),
            }) => {
                assert_eq!(id, "validator-4");
                assert_eq!(voting_weight, 10);
                assert_eq!(public_key.as_deref(), Some("0102030405"));
                assert_eq!(role, "Validator");
                assert!(json);
            }
            _ => panic!("Expected Bft ValidatorAdd command"),
        }

        // BFT byzantine alias and validator-rm
        let cli_rm = Cli::try_parse_from([
            "craft", "byzantine", "validator-rm",
            "-i", "val-bad",
            "--reason", "double-signing",
            "--json",
        ]).unwrap();
        match cli_rm.command {
            Some(Commands::Bft {
                action: Some(BftCommands::ValidatorRm { id, reason, json }),
            }) => {
                assert_eq!(id, "val-bad");
                assert_eq!(reason.as_deref(), Some("double-signing"));
                assert!(json);
            }
            _ => panic!("Expected Bft ValidatorRm command"),
        }

        // BFT tx-submit
        let cli_tx = Cli::try_parse_from([
            "craft", "bft", "tx-submit",
            "-t", "StateUpdate",
            "-p", "{\"key\":\"val\"}",
            "-s", "sender-node-1",
            "--json",
        ]).unwrap();
        match cli_tx.command {
            Some(Commands::Bft {
                action: Some(BftCommands::TxSubmit {
                    tx_type,
                    payload,
                    sender,
                    json,
                }),
            }) => {
                assert_eq!(tx_type, "StateUpdate");
                assert_eq!(payload, "{\"key\":\"val\"}");
                assert_eq!(sender.as_deref(), Some("sender-node-1"));
                assert!(json);
            }
            _ => panic!("Expected Bft TxSubmit command"),
        }

        // BFT view-change
        let cli_vc = Cli::try_parse_from([
            "craft", "bft", "view-change",
            "--reason", "leader-timeout",
            "--json",
        ]).unwrap();
        match cli_vc.command {
            Some(Commands::Bft {
                action: Some(BftCommands::ViewChange { reason, json }),
            }) => {
                assert_eq!(reason.as_deref(), Some("leader-timeout"));
                assert!(json);
            }
            _ => panic!("Expected Bft ViewChange command"),
        }

        // BFT zk-verify
        let cli_zk = Cli::try_parse_from([
            "craft", "bft", "zk-verify",
            "-p", "/var/craft/bft/proof.json",
            "--json",
        ]).unwrap();
        match cli_zk.command {
            Some(Commands::Bft {
                action: Some(BftCommands::ZkVerify { proof_path, json }),
            }) => {
                assert_eq!(proof_path, "/var/craft/bft/proof.json");
                assert!(json);
            }
            _ => panic!("Expected Bft ZkVerify command"),
        }

        // BFT quorum alias and bench
        let cli_bench = Cli::try_parse_from([
            "craft", "quorum", "bench",
            "-t", "200",
            "-v", "7",
            "--json",
        ]).unwrap();
        match cli_bench.command {
            Some(Commands::Bft {
                action: Some(BftCommands::Bench {
                    transactions,
                    validators,
                    json,
                }),
            }) => {
                assert_eq!(transactions, 200);
                assert_eq!(validators, 7);
                assert!(json);
            }
            _ => panic!("Expected Bft Bench command"),
        }
    }

    #[test]
    fn test_jitter_cli_commands() {
        // Jitter status
        let cli_status = Cli::try_parse_from(["craft", "jitter", "status", "--json"]).unwrap();
        match cli_status.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Status { server: None, json }),
            }) => assert!(json),
            _ => panic!("Expected Jitter Status command"),
        }

        // Jitter sched alias and isolate
        let cli_iso = Cli::try_parse_from([
            "craft", "sched", "isolate",
            "-s", "lobby",
            "-p", "85",
            "-c", "2,3",
            "--json",
        ]).unwrap();
        match cli_iso.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Isolate {
                    server,
                    priority,
                    cores,
                    pid: None,
                    json,
                }),
            }) => {
                assert_eq!(server, "lobby");
                assert_eq!(priority, 85);
                assert_eq!(cores, "2,3");
                assert!(json);
            }
            _ => panic!("Expected Jitter Isolate command"),
        }

        // Jitter microstall alias and stalls
        let cli_stalls = Cli::try_parse_from([
            "craft", "microstall", "stalls",
            "-s", "lobby",
            "-l", "10",
            "--json",
        ]).unwrap();
        match cli_stalls.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Stalls {
                    server: Some(s),
                    limit,
                    json,
                }),
            }) => {
                assert_eq!(s, "lobby");
                assert_eq!(limit, 10);
                assert!(json);
            }
            _ => panic!("Expected Jitter Stalls command"),
        }

        // Jitter realtime alias and irq
        let cli_irq = Cli::try_parse_from([
            "craft", "realtime", "irq",
            "-i", "45",
            "-c", "0,1",
            "--json",
        ]).unwrap();
        match cli_irq.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Irq {
                    irq,
                    target_cores,
                    json,
                }),
            }) => {
                assert_eq!(irq, 45);
                assert_eq!(target_cores, "0,1");
                assert!(json);
            }
            _ => panic!("Expected Jitter Irq command"),
        }

        // Jitter sched-trace alias and histogram
        let cli_hist = Cli::try_parse_from([
            "craft", "sched-trace", "histogram",
            "--json",
        ]).unwrap();
        match cli_hist.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Histogram {
                    server: None,
                    json,
                }),
            }) => assert!(json),
            _ => panic!("Expected Jitter Histogram command"),
        }

        // Jitter bench
        let cli_bench = Cli::try_parse_from([
            "craft", "jitter", "bench",
            "-i", "500",
            "-s", "3",
            "--json",
        ]).unwrap();
        match cli_bench.command {
            Some(Commands::Jitter {
                action: Some(JitterCommands::Bench {
                    iterations,
                    simulated_stalls,
                    json,
                }),
            }) => {
                assert_eq!(iterations, 500);
                assert_eq!(simulated_stalls, 3);
                assert!(json);
            }
            _ => panic!("Expected Jitter Bench command"),
        }
    }

    #[test]
    fn test_neuromorphic_cli_commands() {
        // Neuromorphic status
        let cli_snn_status = Cli::try_parse_from([
            "craft", "neuromorphic", "status", "--server", "survival", "--json"
        ]).unwrap();
        match cli_snn_status.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Status { server, json }),
            }) => {
                assert_eq!(server.as_deref(), Some("survival"));
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Status command"),
        }

        // Neuromorphic snn alias and mode
        let cli_snn_mode = Cli::try_parse_from([
            "craft", "snn", "mode", "--server", "creative", "--mode", "predictive-throttle", "--json"
        ]).unwrap();
        match cli_snn_mode.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Mode { server, mode, json }),
            }) => {
                assert_eq!(server, "creative");
                assert_eq!(mode, "predictive-throttle");
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Mode command"),
        }

        // Neuromorphic lif alias and inject
        let cli_snn_inject = Cli::try_parse_from([
            "craft", "lif", "inject", "--server", "creative", "--neuron", "3", "--current", "0.95", "--source", "packet-arrival", "--json"
        ]).unwrap();
        match cli_snn_inject.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Inject { server, neuron, current, source, json }),
            }) => {
                assert_eq!(server, "creative");
                assert_eq!(neuron, 3);
                assert_eq!(current, 0.95);
                assert_eq!(source, "packet-arrival");
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Inject command"),
        }

        // Neuromorphic spike alias and raster
        let cli_snn_raster = Cli::try_parse_from([
            "craft", "spike", "raster", "--limit", "25", "--json"
        ]).unwrap();
        match cli_snn_raster.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Raster { limit, json, .. }),
            }) => {
                assert_eq!(limit, 25);
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Raster command"),
        }

        // Neuromorphic predict
        let cli_snn_pred = Cli::try_parse_from([
            "craft", "neuromorph", "predict", "--server", "lobby", "--json"
        ]).unwrap();
        match cli_snn_pred.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Predict { server, json }),
            }) => {
                assert_eq!(server.as_deref(), Some("lobby"));
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Predict command"),
        }

        // Neuromorphic bench
        let cli_snn_bench = Cli::try_parse_from([
            "craft", "neuromorphic", "bench", "-i", "1000", "-b", "0.25", "--json"
        ]).unwrap();
        match cli_snn_bench.command {
            Some(Commands::Neuromorphic {
                action: Some(NeuromorphicCommands::Bench { iterations, burst_ratio, json }),
            }) => {
                assert_eq!(iterations, 1000);
                assert_eq!(burst_ratio, 0.25);
                assert!(json);
            }
            _ => panic!("Expected Neuromorphic Bench command"),
        }

        // Optical status
        let cli_opt_status = Cli::try_parse_from([
            "craft", "optical", "status", "-s", "lobby", "--json"
        ]).unwrap();
        match cli_opt_status.command {
            Some(Commands::Optical {
                action: Some(OpticalCommands::Status { server, json }),
            }) => {
                assert_eq!(server.as_deref(), Some("lobby"));
                assert!(json);
            }
            _ => panic!("Expected Optical Status command"),
        }

        // Optical mode alias and set
        let cli_opt_mode = Cli::try_parse_from([
            "craft", "ocs", "mode", "-m", "circuit-switched", "-s", "default", "--json"
        ]).unwrap();
        match cli_opt_mode.command {
            Some(Commands::Optical {
                action: Some(OpticalCommands::Mode { mode, server, json }),
            }) => {
                assert_eq!(mode, "circuit-switched");
                assert_eq!(server, "default");
                assert!(json);
            }
            _ => panic!("Expected Optical Mode command"),
        }

        // Optical circuit-add alias
        let cli_opt_circuit_add = Cli::try_parse_from([
            "craft", "photonic", "circuit-add", "-c", "lp-01", "-i", "2", "-e", "5", "-w", "16", "--json"
        ]).unwrap();
        match cli_opt_circuit_add.command {
            Some(Commands::Optical {
                action: Some(OpticalCommands::CircuitAdd { circuit_id, ingress, egress, channel, json, .. }),
            }) => {
                assert_eq!(circuit_id, "lp-01");
                assert_eq!(ingress, 2);
                assert_eq!(egress, 5);
                assert_eq!(channel, 16);
                assert!(json);
            }
            _ => panic!("Expected Optical CircuitAdd command"),
        }

        // Optical circuit-rm alias
        let cli_opt_circuit_rm = Cli::try_parse_from([
            "craft", "waveguide", "circuit-rm", "-c", "lp-01", "--json"
        ]).unwrap();
        match cli_opt_circuit_rm.command {
            Some(Commands::Optical {
                action: Some(OpticalCommands::CircuitRm { circuit_id, json }),
            }) => {
                assert_eq!(circuit_id, "lp-01");
                assert!(json);
            }
            _ => panic!("Expected Optical CircuitRm command"),
        }

        // Optical bench alias
        let cli_opt_bench = Cli::try_parse_from([
            "craft", "wdm", "bench", "-f", "2000", "-d", "16", "--json"
        ]).unwrap();
        match cli_opt_bench.command {
            Some(Commands::Optical {
                action: Some(OpticalCommands::Bench { frames, dimension, json }),
            }) => {
                assert_eq!(frames, 2000);
                assert_eq!(dimension, 16);
                assert!(json);
            }
            _ => panic!("Expected Optical Bench command"),
        }
    }
}




