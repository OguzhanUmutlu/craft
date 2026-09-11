use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
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

pub struct MenuEntry {
    pub hotkey: String,
    pub label: String,
}

impl MenuEntry {
    pub fn new(hotkey: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            hotkey: hotkey.into(),
            label: label.into(),
        }
    }
}

pub fn run_menu(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    let result = (|| -> Result<Option<usize>> {
        if *selected_idx >= entries.len() {
            *selected_idx = 0;
        }

        loop {
            execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

            for line in header.lines() {
                print!("{}\r\n", line);
            }
            print!("\r\n");

            for (idx, entry) in entries.iter().enumerate() {
                if idx == *selected_idx {
                    print!(
                        "  \x1B[1;36m>\x1B[0m \x1B[1;97;44m {:<4} {:<68} \x1B[0m\r\n",
                        format!("[{}]", entry.hotkey),
                        entry.label
                    );
                } else {
                    print!(
                        "    \x1B[1;36m{:<4}\x1B[0m {:<68}\r\n",
                        format!("[{}]", entry.hotkey),
                        entry.label
                    );
                }
            }

            print!("\r\n\x1B[2m--------------------------------------------------------------------------------\r\n");
            print!(" [HOTKEYS] Press 0-9 / keys directly  |  [↑/↓/j/k] Move  |  [Enter] Select  |  [q] Exit\x1B[0m\r\n");
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        } else {
                            *selected_idx = entries.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if *selected_idx + 1 < entries.len() {
                            *selected_idx += 1;
                        } else {
                            *selected_idx = 0;
                        }
                    }
                    KeyCode::Home => {
                        *selected_idx = 0;
                    }
                    KeyCode::End => {
                        *selected_idx = entries.len().saturating_sub(1);
                    }
                    KeyCode::Enter => {
                        return Ok(Some(*selected_idx));
                    }
                    KeyCode::Esc => {
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        let c_lower = c.to_ascii_lowercase();
                        if c_lower == 'q' {
                            if let Some(pos) = entries.iter().position(|e| e.hotkey.eq_ignore_ascii_case("q")) {
                                *selected_idx = pos;
                                return Ok(Some(pos));
                            }
                            return Ok(None);
                        }

                        let c_str = c_lower.to_string();
                        if let Some(pos) = entries.iter().position(|e| e.hotkey.eq_ignore_ascii_case(&c_str)) {
                            *selected_idx = pos;
                            return Ok(Some(pos));
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show);
    result
}

fn press_enter() {
    print!("\r\n  \x1B[2mPress Enter to continue...\x1B[0m");
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

fn prompt_text(prompt: &str, default: Option<&str>) -> io::Result<String> {
    print!("  \x1B[1;36m?\x1B[0m {} ", prompt);
    if let Some(d) = default {
        print!("\x1B[2m[{}]\x1B[0m: ", d);
    } else {
        print!(": ");
    }
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        if let Some(d) = default {
            return Ok(d.to_string());
        }
    }
    Ok(trimmed.to_string())
}

fn prompt_confirm(prompt: &str, default_yes: bool) -> io::Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("  \x1B[1;33m?\x1B[0m {} \x1B[2m[{}]\x1B[0m: ", prompt, hint);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default_yes);
    }
    Ok(trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes"))
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

fn build_dashboard_header(
    os: &str,
    total_ram: f64,
    used_ram: f64,
    ram_pct: f64,
    daemon_online: bool,
    registered_count: usize,
    running_count: usize,
) -> String {
    let daemon_badge = if daemon_online {
        "[ONLINE]".green().bold()
    } else {
        "[OFFLINE]".yellow().bold()
    };

    format!(
        "{}\r\n{}\r\n{}\r\n Host: {:<16} | RAM: {:.1} / {:.1} GB ({:.1}%) | Daemon: {}\r\n Registered Servers: {:<4} | Active Running: {:<4}\r\n{}",
        "================================================================================".cyan().bold(),
        "                         CRAFT SERVER MANAGER DASHBOARD                         ".cyan().bold(),
        "================================================================================".cyan().bold(),
        os.white().bold(),
        used_ram,
        total_ram,
        ram_pct,
        daemon_badge,
        registered_count.to_string().cyan().bold(),
        running_count.to_string().green().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    )
}

pub async fn handle_dashboard(paths: &CraftPaths) -> Result<()> {
    if !io::stdin().is_terminal() {
        println!("{}", "Craft dashboard requires an interactive terminal (TTY).".yellow());
        return Ok(());
    }

    let mut selected_main = 0;

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

        let header = build_dashboard_header(
            &os,
            total_ram,
            used_ram,
            ram_pct,
            daemon_online,
            total_servers,
            running_count,
        );

        let entries = vec![
            MenuEntry::new("1", "Manage Servers (Start, Stop, Restart, Console, Delete)"),
            MenuEntry::new("2", "Create New Server (Interactive Wizard)"),
            MenuEntry::new("3", "Quick Start Server"),
            MenuEntry::new("4", "Stop Running Server"),
            MenuEntry::new("5", "Restart Server"),
            MenuEntry::new("6", "Attach Live Console (craft view)"),
            MenuEntry::new("7", "Server Network Ping (Java SLP & Bedrock)"),
            MenuEntry::new("8", "World Snapshots & Backup Manager"),
            MenuEntry::new("9", "Browse & Install Plugins (Modrinth / Hangar)"),
            MenuEntry::new("r", "Remote VPS Hosts (SSH Management)"),
            MenuEntry::new("d", "Service Daemon Control (Start / Stop / Restart)"),
            MenuEntry::new("c", "Cache & Storage Management"),
            MenuEntry::new("0", "Exit Craft"),
        ];

        let selection = run_menu(&header, &entries, &mut selected_main)?;

        match selection {
            Some(0) => {
                manage_servers_menu(paths).await?;
            }
            Some(1) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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
            Some(2) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_run("", None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(3) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_stop("", None, false, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(4) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_restart("", None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(5) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_view("", None, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(6) => {
                ping_menu().await?;
            }
            Some(7) => {
                backups_menu(paths).await?;
            }
            Some(8) => {
                plugins_menu(paths).await?;
            }
            Some(9) => {
                remotes_menu(paths).await?;
            }
            Some(10) => {
                daemon_menu(paths).await?;
            }
            Some(11) => {
                cache_menu(paths)?;
            }
            Some(12) | None => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                println!("{}", "Exiting Craft. Goodbye!".cyan().bold());
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

async fn manage_servers_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
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

        if registry.servers.is_empty() {
            let header = format!(
                "{}\r\n{}\r\n{}\r\n No servers currently registered on this machine.\r\n{}",
                "================================================================================".cyan().bold(),
                "                               REGISTERED SERVERS                               ".cyan().bold(),
                "================================================================================".cyan().bold(),
                "--------------------------------------------------------------------------------".dimmed()
            );

            let entries = vec![
                MenuEntry::new("2", "Create Your First Server (Setup Wizard)"),
                MenuEntry::new("0", "Back to Main Menu"),
            ];

            match run_menu(&header, &entries, &mut selected)? {
                Some(0) => {
                    let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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
                _ => return Ok(()),
            }
            continue;
        }

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a server to inspect details, control lifecycle, or attach console.\r\n{}",
            "================================================================================".cyan().bold(),
            "                               REGISTERED SERVERS                               ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let mut entries = Vec::new();
        for (idx, s) in registry.servers.iter().enumerate() {
            let is_running = running_paths.contains(&s.path)
                || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);
            let status_str = if is_running {
                "[RUNNING]".green().bold().to_string()
            } else {
                "[STOPPED]".dimmed().to_string()
            };
            let hotkey = if idx < 9 {
                (idx + 1).to_string()
            } else {
                ((b'a' + (idx - 9) as u8) as char).to_string()
            };
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} {:<10} {:<10} {}", s.name, s.software, s.version, status_str),
            ));
        }

        entries.push(MenuEntry::new("n", "Create New Server (Setup Wizard)"));
        entries.push(MenuEntry::new("0", "Back to Main Menu"));

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(idx) if idx < registry.servers.len() => {
                let chosen = &registry.servers[idx];
                server_control_panel(&chosen.name, paths).await?;
            }
            Some(idx) if idx == registry.servers.len() => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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
            _ => return Ok(()),
        }
    }
}

async fn server_control_panel(name: &str, paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

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

        let title = format!("SERVER CONTROL: {} {}", server.name, status_badge);
        let header = format!(
            "{}\r\n {:^78} \r\n{}\r\n  Platform: {} {}\r\n  Path:     {}\r\n  Memory:   {}\r\n{}",
            "================================================================================".cyan().bold(),
            title,
            "================================================================================".cyan().bold(),
            server.software.white().bold(),
            server.version.cyan(),
            server.path.display(),
            server.memory.as_deref().unwrap_or("Default (2G)"),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Start Server (Background Daemon)"),
            MenuEntry::new("2", "Start Server (Foreground Terminal)"),
            MenuEntry::new("3", "Stop Server"),
            MenuEntry::new("4", "Restart Server"),
            MenuEntry::new("5", "Attach Live Console (craft view)"),
            MenuEntry::new("6", "Create World Snapshot Backup"),
            MenuEntry::new("7", "List Existing Backups"),
            MenuEntry::new("8", "Delete / Unregister Server"),
            MenuEntry::new("0", "Back to Server List"),
        ];

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(0) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_run(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_run(&server.name, None, true, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(2) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_stop(&server.name, None, false, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(3) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_restart(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(4) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_view(&server.name, None, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
            }
            Some(5) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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
            Some(6) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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
            Some(7) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_rm(&server.name, None, false, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                press_enter();
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

async fn ping_menu() -> Result<()> {
    let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
    println!("{}", "================================================================================".cyan().bold());
    println!("{}", "                           SERVER NETWORK PING                                  ".cyan().bold());
    println!("{}", "================================================================================".cyan().bold());
    println!(" Test network reachability, latency, and online player counts.\r\n");

    let target = prompt_text("Server address (IP:Port or Domain)", Some("127.0.0.1:25565"))?;

    let mut proto_sel = 0;
    let proto_header = " Select Protocol:";
    let proto_entries = vec![
        MenuEntry::new("1", "Java Edition (SLP Protocol)"),
        MenuEntry::new("2", "Bedrock Edition (RakNet Protocol)"),
        MenuEntry::new("0", "Cancel"),
    ];

    let choice = run_menu(proto_header, &proto_entries, &mut proto_sel)?;
    let is_bedrock = match choice {
        Some(0) => false,
        Some(1) => true,
        _ => return Ok(()),
    };

    let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
    let res = handle_ping(&target, is_bedrock).await;
    if let Err(e) = res {
        eprintln!("{}: {}", "Ping Error".red().bold(), e);
    }
    press_enter();
    Ok(())
}

async fn backups_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Create compressed backups, inspect archive history, or restore worlds.\r\n{}",
            "================================================================================".cyan().bold(),
            "                       WORLD SNAPSHOTS & BACKUP MANAGER                         ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Create World Snapshot"),
            MenuEntry::new("2", "List Existing Backups"),
            MenuEntry::new("3", "Restore Server from Backup"),
            MenuEntry::new("0", "Back to Main Menu"),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let server = match prompt_select_server(paths)? {
                    Some(s) => s,
                    None => continue,
                };
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let world_only = prompt_confirm("Snapshot world directories only (faster, skips logs)?", false)?;
                let res = handle_backup(
                    BackupCommands::Create { server, world_only },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Backup Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let server = match prompt_select_server(paths)? {
                    Some(s) => s,
                    None => continue,
                };
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_backup(
                    BackupCommands::List { server },
                    paths,
                ).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Backup Error".red().bold(), e);
                }
                press_enter();
            }
            Some(2) => {
                let server = match prompt_select_server(paths)? {
                    Some(s) => s,
                    None => continue,
                };
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let backup_file_str = prompt_text("Path to backup archive (.tar.gz / .zip)", None)?;
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

async fn plugins_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Discover and install plugins from Modrinth, Hangar, and Poggit.\r\n{}",
            "================================================================================".cyan().bold(),
            "                         PLUGINS & EXTENSIONS MANAGER                           ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Search Plugins Online"),
            MenuEntry::new("2", "Install Plugin to Server"),
            MenuEntry::new("0", "Back to Main Menu"),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let query = prompt_text("Search keyword (e.g. essentials, viaversion, worldedit)", None)?;
                let res = handle_plugin(PluginCommands::Search { query }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Plugin Search Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let server = match prompt_select_server(paths)? {
                    Some(s) => s,
                    None => continue,
                };
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let project_id = prompt_text("Plugin ID or slug from Modrinth", None)?;
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

async fn remotes_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Manage remote game server hosts and orchestrated deployments over SSH.\r\n{}",
            "================================================================================".cyan().bold(),
            "                            REMOTE VPS HOSTS (SSH)                              ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "List Configured Remote Hosts"),
            MenuEntry::new("2", "Test Remote Host Connection"),
            MenuEntry::new("3", "Add New Remote Host"),
            MenuEntry::new("4", "Remove Remote Host"),
            MenuEntry::new("0", "Back to Main Menu"),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_remote(RemoteCommands::Ls, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Remote Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let alias = prompt_text("Remote host alias", None)?;
                let res = handle_remote(RemoteCommands::Test { alias }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Test Error".red().bold(), e);
                }
                press_enter();
            }
            Some(2) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let alias = prompt_text("New alias (e.g. prod-vps)", None)?;
                let connection = prompt_text("Connection string (e.g. user@192.168.1.100 or user@host:22)", None)?;

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
            Some(3) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let alias = prompt_text("Remote alias to remove", None)?;
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

async fn daemon_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Control the background 24/7 supervisor daemon on this system.\r\n{}",
            "================================================================================".cyan().bold(),
            "                          CRAFT SERVICE DAEMON CONTROL                          ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Check Daemon Status"),
            MenuEntry::new("2", "Start Daemon"),
            MenuEntry::new("3", "Stop Daemon"),
            MenuEntry::new("4", "Restart Daemon"),
            MenuEntry::new("0", "Back to Main Menu"),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_service(ServiceCommands::Status, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_service(ServiceCommands::Start { foreground: false }, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            Some(2) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_service(ServiceCommands::Stop, paths).await;
                if let Err(e) = res {
                    eprintln!("{}: {}", "Daemon Error".red().bold(), e);
                }
                press_enter();
            }
            Some(3) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
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

fn cache_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Inspect and purge cached server jarfiles, runtimes, and archives.\r\n{}",
            "================================================================================".cyan().bold(),
            "                         CACHE & STORAGE MANAGEMENT                             ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Show Current Cache Size"),
            MenuEntry::new("2", "Purge All Download Caches"),
            MenuEntry::new("0", "Back to Main Menu"),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let res = handle_cache(None, paths);
                if let Err(e) = res {
                    eprintln!("{}: {}", "Cache Error".red().bold(), e);
                }
                press_enter();
            }
            Some(1) => {
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
                let confirmed = prompt_confirm("Are you sure you want to delete all cached downloads?", false)?;
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

fn prompt_select_server(paths: &CraftPaths) -> Result<Option<String>> {
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        println!("{}", "No registered servers found.".yellow());
        return Ok(None);
    }

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let hotkey = if i < 9 {
            (i + 1).to_string()
        } else {
            ((b'a' + (i - 9) as u8) as char).to_string()
        };
        entries.push(MenuEntry::new(
            hotkey,
            format!("{:<20} [{} {}]", s.name, s.software, s.version),
        ));
    }
    entries.push(MenuEntry::new("0", "Cancel"));

    let mut selected = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select target server:\r\n{}",
        "================================================================================".cyan().bold(),
        "                             SELECT TARGET SERVER                               ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    match run_menu(&header, &entries, &mut selected)? {
        Some(idx) if idx < registry.servers.len() => Ok(Some(registry.servers[idx].name.clone())),
        _ => Ok(None),
    }
}
