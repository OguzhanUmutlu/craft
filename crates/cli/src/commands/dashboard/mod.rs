pub mod screen;
pub mod server_control;
pub mod tools;
pub mod wizard;

use std::io::{self, IsTerminal};
use colored::Colorize;
use sysinfo::System;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub use screen::*;
pub use server_control::*;
pub use tools::*;
pub use wizard::*;

pub(crate) fn get_system_summary() -> (String, f64, f64, f64) {
    let mut sys = System::new();
    sys.refresh_memory();
    let total_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let pct = if total_gb > 0.0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };
    let os_name = System::name().unwrap_or_else(|| "Linux".to_string());
    (os_name, total_gb, used_gb, pct)
}

pub(crate) fn build_dashboard_header(
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
        "================================================================================"
            .cyan()
            .bold(),
        "                         CRAFT SERVER MANAGER DASHBOARD                         "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
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
        println!(
            "{}",
            "Craft dashboard requires an interactive terminal (TTY).".yellow()
        );
        return Ok(());
    }

    let _guard = AltScreenGuard::enter();
    let mut selected_main = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let total_servers = registry.servers.len();

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let running_paths = if daemon_running {
            if let Ok(mut client) = DaemonClient::connect(paths).await {
                client.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let running_count = registry
            .servers
            .iter()
            .filter(|s| {
                running_paths.contains(&s.path)
                    || s.path
                        .canonicalize()
                        .map(|p| running_paths.contains(&p))
                        .unwrap_or(false)
            })
            .count();

        let (os_name, total_ram, used_ram, ram_pct) = get_system_summary();
        let header = build_dashboard_header(
            &os_name,
            total_ram,
            used_ram,
            ram_pct,
            daemon_running,
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
                gui_create_server_wizard(paths).await?;
            }
            Some(2) => {
                quick_start_menu(paths).await?;
            }
            Some(3) => {
                stop_servers_menu(paths).await?;
            }
            Some(4) => {
                restart_servers_menu(paths).await?;
            }
            Some(5) => {
                view_servers_menu(paths).await?;
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
                break;
            }
            _ => break,
        }
    }

    Ok(())
}
