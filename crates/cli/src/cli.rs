use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "craft")]
#[command(author = "OguzhanUmutlu")]
#[command(version)]
#[command(about = "High-performance Minecraft server management CLI and daemon", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Open the interactive Craft Server Manager Dashboard
    #[command(alias = "dashboard", alias = "ui", alias = "tui")]
    Manage,

    /// Set up a new Minecraft server (interactive wizard if software omitted)
    #[command(alias = "create", alias = "init")]
    New {
        /// Server name (or omit to launch the interactive setup wizard)
        #[arg(default_value = "")]
        name: String,
        /// Server software (paper, purpur, folia, fabric, quilt, neoforge, velocity, etc.)
        #[arg(value_name = "SOFTWARE")]
        software: Option<String>,
        /// Software version (or "latest")
        #[arg(value_name = "VERSION")]
        version: Option<String>,
        /// Explicit server software option
        #[arg(short = 's', long = "software-id")]
        software_opt: Option<String>,
        /// Explicit software version option
        #[arg(short = 'v', long = "version-id")]
        version_opt: Option<String>,
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
    #[command(alias = "attach", alias = "console", alias = "logs")]
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

    /// Search and install plugins, mods, and datapacks
    Plugin {
        #[command(subcommand)]
        action: Option<PluginCommands>,
    },

    /// Ping a Minecraft Java or Bedrock server
    Ping {
        /// Target address (e.g. localhost:25565 or server name)
        #[arg(default_value = "")]
        target: String,
        /// Bedrock server ping
        #[arg(long)]
        bedrock: bool,
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

    /// Deploy and manage Craft container stack with Docker Compose
    Deploy {
        #[command(subcommand)]
        action: Option<DeployCommands>,
    },
}

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Clean all cached assets
    Clean {
        #[arg(long)]
        force: bool,
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

#[derive(Subcommand)]
pub enum PluginCommands {
    /// Search for plugins across Modrinth, Hangar, and Poggit
    Search {
        query: String,
    },
    /// Install a plugin from Modrinth by project ID
    Install {
        project_id: String,
        /// Server name or path
        server: String,
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
    },
    /// List backups for a server
    List {
        server: String,
    },
    /// Restore a backup archive to a server
    Restore {
        server: String,
        backup_file: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum FirewallCommands {
    /// Allow an IP to connect to server port
    Allow {
        server: String,
        ip: String,
    },
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
    Datapack {
        name: String,
    },
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
    Rm {
        alias: String,
    },
    /// Test connection and authentication to a remote host
    Test {
        alias: String,
    },
    /// Automatically setup and bootstrap a remote host (installs Java, Craft daemon, Firewall)
    Setup {
        alias: String,
    },
    /// Synchronize files from local to remote server
    Sync {
        alias: String,
        local_dir: PathBuf,
        remote_dir: PathBuf,
    },
    /// Deploy Craft container stack to a remote host over SSH
    Deploy {
        alias: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DeployCommands {
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

