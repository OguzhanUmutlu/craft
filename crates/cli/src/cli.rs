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
    Search { query: String },
    /// Install a plugin from Modrinth by project ID or slug
    Install {
        project_id: String,
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
    Search { query: String },
    /// Install a mod from Modrinth to a modded server
    Install {
        project_id: String,
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
    List { server: String },
    /// Restore a backup archive to a server
    Restore {
        server: String,
        backup_file: PathBuf,
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
                action: Some(PluginCommands::Install { project_id, server }),
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
                action: Some(ModCommands::Install { project_id, server }),
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
}
