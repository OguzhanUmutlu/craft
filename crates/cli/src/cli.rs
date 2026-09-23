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

    /// Migrate a server to a remote host with atomic snapshot and verification
    Migrate {
        /// Local server name to migrate
        server: String,
        /// Target remote host alias (e.g. --to my-vps)
        #[arg(long)]
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

#[derive(Subcommand, Debug, Clone)]
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
}


