use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use sysinfo::System;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;
use crate::cli::{BackupCommands, CacheCommands, PluginCommands, RemoteCommands, ServiceCommands};
use crate::commands::{
    backup::handle_backup,
    cache::handle_cache,
    net::handle_ping,
    new::handle_new,
    plugin::handle_plugin,
    remote::handle_remote,
    restart::handle_restart,
    rm::handle_rm,
    run::handle_run,
    service::handle_service,
    stop::handle_stop,
    view::handle_view,
};

fn press_enter() {
    println!();
    print!("{}", "Press Enter to continue...".dimmed());
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

fn get_system_summary() -> (String, f64, f64, f64) {
    let mut sys = System::new();
    sys.refresh_memory();
    let total_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let pct = if total_gb > 0.0 { (used_gb / total_gb) * 100.0 } else { 0.0 };
    let os_name = System::name().unwrap_or_else(|| "Linux".to_string());
    (os_name, total_gb, used_gb, pct)
}

fn print_dashboard_header(
    os: &str,
    total_ram: f64,
    used_ram: f64,
    ram_pct: f64,
    daemon_online: bool,
    registered_count: usize,
    running_count: usize,
) {
    let daemon_badge = if daemon_online {
        "[ONLINE]".green().bold()
    } else {
        "[OFFLINE]".yellow().bold()
    };

    println!();
    println!("{}", "================================================================================".cyan().bold());
    println!("{}", "                         CRAFT SERVER MANAGER DASHBOARD                         ".cyan().bold());
    println!("{}", "================================================================================".cyan().bold());
    println!(
        " Host: {:<16} | RAM: {:.1} / {:.1} GB ({:.1}%) | Daemon: {}",
        os.white().bold(),
        used_ram,
        total_ram,
        ram_pct,
        daemon_badge
    );
    println!(
        " Registered Servers: {:<4} | Active Running: {:<4}",
        registered_count.to_string().cyan().bold(),
        running_count.to_string().green().bold()
    );
    println!("{}", "--------------------------------------------------------------------------------".dimmed());
}

pub async fn handle_dashboard(paths: &CraftPaths) -> Result<()> {
    if !io::stdin().is_terminal() {
        println!("{}", "Craft dashboard requires an interactive terminal (TTY).".yellow());
        return Ok(());
    }

    let theme = ColorfulTheme::default();

    loop {
        let (os, total_ram, used_ram, ram_pct) = get_system_summary();
        let daemon_online = DaemonClient::is_daemon_running(paths);
        let registry = ServersRegistry::load(paths).unwrap_or_default();
        let total_servers = registry.servers.len();

        let running_paths = if daemon_online {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                c.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let running_count = running_paths.len();

        print_dashboard_header(
            &os,
            total_ram,
            used_ram,
            ram_pct,
            daemon_online,
            total_servers,
            running_count,
        );

        let options = &[
            "[1] Manage Servers (Start, Stop, Restart, Console, Delete)",
            "[2] Create New Server (Interactive Wizard)",
            "[3] Quick Start Server",
            "[4] Stop Running Server",
            "[5] Restart Server",
            "[6] Attach Live Console (craft view)",
            "[7] Server Network Ping (Java SLP & Bedrock)",
            "[8] World Snapshots & Backup Manager",
            "[9] Browse & Install Plugins (Modrinth / Hangar)",
            "[10] Remote VPS Hosts (SSH Management)",
            "[11] Service Daemon Control (Start / Stop / Restart)",
            "[12] Cache & Storage Management",
            "[0] Exit Craft",
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Select action")
            .items(options)
            .default(0)
            .interact();

        let choice = match selection {
            Ok(idx) => idx,
            Err(_) => break,
        };

        match choice {
            0 => {
                manage_servers_menu(paths, &theme).await?;
            }
            1 => {
                println!();
                let res = handle_new(
                    "",
                    None,
                    None,
                    None,
                    None,
                    false,
                    false,
                    false,
                    false,
                    false,
                    false,
                    false,
                    None,
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Setup Error".red().bold(), e);
                }
                press_enter();
            }
            2 => {
                println!();
                let res = handle_run("", None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            3 => {
                println!();
                let res = handle_stop("", None, false, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            4 => {
                println!();
                let res = handle_restart("", None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            5 => {
                println!();
                let res = handle_view("", None, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            6 => {
                ping_menu(&theme).await?;
            }
            7 => {
                backups_menu(paths, &theme).await?;
            }
            8 => {
                plugins_menu(paths, &theme).await?;
            }
            9 => {
                remotes_menu(paths, &theme).await?;
            }
            10 => {
                daemon_menu(paths, &theme).await?;
            }
            11 => {
                cache_menu(paths, &theme)?;
            }
            12 => {
                println!("{}", "Exiting Craft. Goodbye!".cyan());
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

async fn manage_servers_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            println!("{}", "\nNo servers registered. Create one with option [2].".yellow());
            press_enter();
            return Ok(());
        }

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let running_paths = if daemon_running {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                c.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        println!();
        println!("{}", "=== Registered Servers ===".cyan().bold());

        let mut items: Vec<String> = registry
            .servers
            .iter()
            .map(|s| {
                let is_running = running_paths.contains(&s.path)
                    || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);
                let status_str = if is_running {
                    "[RUNNING]".green().bold().to_string()
                } else {
                    "[STOPPED]".dimmed().to_string()
                };
                format!("{:<20} {:<10} {:<10} {}", s.name, s.software, s.version, status_str)
            })
            .collect();
        items.push("[0] Back to Main Menu".to_string());

        let sel = Select::with_theme(theme)
            .with_prompt("Select a server to manage")
            .items(&items)
            .default(0)
            .interact();

        let idx = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        if idx >= registry.servers.len() {
            return Ok(());
        }

        let chosen = &registry.servers[idx];
        server_control_panel(&chosen.name, paths, theme).await?;
    }
}

async fn server_control_panel(name: &str, paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.find_by_name(name) {
            Some(s) => s.clone(),
            None => {
                println!("{}", format!("Server '{}' is no longer registered.", name).yellow());
                press_enter();
                return Ok(());
            }
        };

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let is_running = if daemon_running {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                let running = c.get_running().await.unwrap_or_default();
                running.contains(&server.path)
                    || server.path.canonicalize().map(|p| running.contains(&p)).unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let status_badge = if is_running {
            "[RUNNING]".green().bold()
        } else {
            "[STOPPED]".dimmed()
        };

        println!();
        println!("{}", format!("=== Server Control: {} {} ===", server.name, status_badge).cyan().bold());
        println!("  Platform: {} {}", server.software, server.version);
        println!("  Path:     {}", server.path.display());
        println!("  Memory:   {}", server.memory.as_deref().unwrap_or("Default (2G)"));
        println!("{}", "--------------------------------------------------------------------------------".dimmed());

        let actions = &[
            "[1] Start Server (Background Daemon)",
            "[2] Start Server (Foreground Terminal)",
            "[3] Stop Server",
            "[4] Restart Server",
            "[5] Attach Live Console (craft view)",
            "[6] Create World Snapshot Backup",
            "[7] List Existing Backups",
            "[8] Delete / Unregister Server",
            "[0] Back to Server List",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Server Action")
            .items(actions)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                println!();
                let res = handle_run(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                println!();
                let res = handle_run(&server.name, None, true, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            2 => {
                println!();
                let res = handle_stop(&server.name, None, false, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            3 => {
                println!();
                let res = handle_restart(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            4 => {
                println!();
                let res = handle_view(&server.name, None, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            5 => {
                println!();
                let res = handle_backup(
                    BackupCommands::Create {
                        server: server.name.clone(),
                        world_only: false,
                    },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            6 => {
                println!();
                let res = handle_backup(
                    BackupCommands::List {
                        server: server.name.clone(),
                    },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            7 => {
                println!();
                let res = handle_rm(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
                return Ok(());
            }
            8 => return Ok(()),
            _ => return Ok(()),
        }
    }
}

async fn ping_menu(theme: &ColorfulTheme) -> Result<()> {
    println!();
    println!("{}", "=== Ping Minecraft Server ===".cyan().bold());

    let target: String = Input::with_theme(theme)
        .with_prompt("Server address (host or host:port)")
        .default("127.0.0.1:25565".to_string())
        .interact_text()?;

    let proto_items = &["[1] Java Edition (SLP Protocol)", "[2] Bedrock Edition (RakNet Protocol)"];
    let proto = Select::with_theme(theme)
        .with_prompt("Protocol")
        .items(proto_items)
        .default(0)
        .interact()?;

    let is_bedrock = proto == 1;

    let res = handle_ping(&target, is_bedrock).await;
    if let Err(e) = res {
        eprintln!("{}: {}", "Ping Error".red().bold(), e);
    }
    press_enter();
    Ok(())
}

async fn backups_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        println!();
        println!("{}", "=== World Snapshots & Backups ===".cyan().bold());

        let options = &[
            "[1] Create Snapshot Backup",
            "[2] List Server Backups",
            "[3] Restore Server from Backup",
            "[0] Back",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Backup Action")
            .items(options)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                let server = match prompt_select_server(paths, theme)? {
                    Some(s) => s,
                    None => continue,
                };
                let world_only = Confirm::with_theme(theme)
                    .with_prompt("Snapshot world directories only (faster, skips logs)?")
                    .default(false)
                    .interact()?;

                let res = handle_backup(
                    BackupCommands::Create { server, world_only },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Backup Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                let server = match prompt_select_server(paths, theme)? {
                    Some(s) => s,
                    None => continue,
                };
                let res = handle_backup(
                    BackupCommands::List { server },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Backup Error".red().bold(), e);
                }
                press_enter();
            }
            2 => {
                let server = match prompt_select_server(paths, theme)? {
                    Some(s) => s,
                    None => continue,
                };
                let backup_file_str: String = Input::with_theme(theme)
                    .with_prompt("Path to backup archive (.tar.gz / .zip)")
                    .interact_text()?;

                let res = handle_backup(
                    BackupCommands::Restore {
                        server,
                        backup_file: PathBuf::from(backup_file_str),
                    },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Restore Error".red().bold(), e);
                }
                press_enter();
            }
            _ => return Ok(()),
        }
    }
}

async fn plugins_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        println!();
        println!("{}", "=== Plugins Manager (Modrinth / Hangar / Poggit) ===".cyan().bold());

        let options = &[
            "[1] Search Plugins Online",
            "[2] Install Plugin to Server",
            "[0] Back",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Action")
            .items(options)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                let query: String = Input::with_theme(theme)
                    .with_prompt("Search keyword (e.g. essentials, viaversion, worldedit)")
                    .interact_text()?;

                let res = handle_plugin(PluginCommands::Search { query }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Plugin Search Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                let server = match prompt_select_server(paths, theme)? {
                    Some(s) => s,
                    None => continue,
                };
                let project_id: String = Input::with_theme(theme)
                    .with_prompt("Plugin ID or slug from Modrinth")
                    .interact_text()?;

                let res = handle_plugin(
                    PluginCommands::Install { project_id, server },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Install Error".red().bold(), e);
                }
                press_enter();
            }
            _ => return Ok(()),
        }
    }
}

async fn remotes_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        println!();
        println!("{}", "=== Remote VPS Hosts (SSH) ===".cyan().bold());

        let options = &[
            "[1] List Configured Remote Hosts",
            "[2] Test Remote Host Connection",
            "[3] Add New Remote Host",
            "[4] Remove Remote Host",
            "[0] Back",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Remote Action")
            .items(options)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                let res = handle_remote(RemoteCommands::Ls, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Remote Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                let alias: String = Input::with_theme(theme)
                    .with_prompt("Remote host alias")
                    .interact_text()?;
                let res = handle_remote(RemoteCommands::Test { alias }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Test Error".red().bold(), e);
                }
                press_enter();
            }
            2 => {
                let alias: String = Input::with_theme(theme)
                    .with_prompt("New alias (e.g. prod-vps)")
                    .interact_text()?;
                let connection: String = Input::with_theme(theme)
                    .with_prompt("Connection string (e.g. user@192.168.1.100 or user@host:22)")
                    .interact_text()?;

                let res = handle_remote(
                    RemoteCommands::Add {
                        alias,
                        connection,
                        key: None,
                        password: None,
                        dir: None,
                    },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Add Error".red().bold(), e);
                }
                press_enter();
            }
            3 => {
                let alias: String = Input::with_theme(theme)
                    .with_prompt("Remote alias to remove")
                    .interact_text()?;
                let res = handle_remote(RemoteCommands::Rm { alias }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Remove Error".red().bold(), e);
                }
                press_enter();
            }
            _ => return Ok(()),
        }
    }
}

async fn daemon_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        println!();
        println!("{}", "=== Craft Service Daemon ===".cyan().bold());

        let options = &[
            "[1] Daemon Status",
            "[2] Start Daemon",
            "[3] Stop Daemon",
            "[4] Restart Daemon",
            "[0] Back",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Daemon Action")
            .items(options)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                let res = handle_service(ServiceCommands::Status, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                let res = handle_service(ServiceCommands::Start { foreground: false }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            2 => {
                let res = handle_service(ServiceCommands::Stop, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            3 => {
                let res = handle_service(ServiceCommands::Restart, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            _ => return Ok(()),
        }
    }
}

fn cache_menu(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<()> {
    loop {
        println!();
        println!("{}", "=== Cache & Storage Management ===".cyan().bold());

        let options = &[
            "[1] Show Current Cache Size",
            "[2] Purge All Download Caches",
            "[0] Back",
        ];

        let sel = Select::with_theme(theme)
            .with_prompt("Cache Action")
            .items(options)
            .default(0)
            .interact();

        let choice = match sel {
            Ok(i) => i,
            Err(_) => return Ok(()),
        };

        match choice {
            0 => {
                let res = handle_cache(None, paths);
                if let Err(e) = res {
                    eprintln!("{}: {}", "Cache Error".red().bold(), e);
                }
                press_enter();
            }
            1 => {
                let confirmed = Confirm::with_theme(theme)
                    .with_prompt("Are you sure you want to delete all cached downloads?")
                    .default(false)
                    .interact()?;

                if confirmed {
                    let res = handle_cache(Some(CacheCommands::Clean { force: true }), paths);
                    if let Err(e) = res {
                        eprintln!("{}: {}", "Cache Error".red().bold(), e);
                    }
                } else {
                    println!("{}", "Cache purge cancelled.".cyan());
                }
                press_enter();
            }
            _ => return Ok(()),
        }
    }
}

fn prompt_select_server(paths: &CraftPaths, theme: &ColorfulTheme) -> Result<Option<String>> {
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        println!("{}", "No registered servers found.".yellow());
        return Ok(None);
    }

    let mut items: Vec<String> = registry.servers.iter().map(|s| format!("{:<20} [{} {}]", s.name, s.software, s.version)).collect();
    items.push("[0] Cancel".to_string());

    let sel = Select::with_theme(theme)
        .with_prompt("Select target server")
        .items(&items)
        .default(0)
        .interact()?;

    if sel >= registry.servers.len() {
        Ok(None)
    } else {
        Ok(Some(registry.servers[sel].name.clone()))
    }
}
