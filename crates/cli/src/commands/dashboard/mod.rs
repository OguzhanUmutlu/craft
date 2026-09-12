pub mod screen;
pub mod server_control;
pub mod tools;
pub mod wizard;
pub mod remote_tui;
pub mod cloud_backups;

use std::io::{self, IsTerminal};
use colored::Colorize;
use sysinfo::System;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub use screen::*;
pub use server_control::*;
pub use tools::*;
pub use wizard::*;
pub use remote_tui::remote_servers_menu;






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

    let width = get_content_width(80);
    format!(
        "{}\r\n{}\r\n{}\r\n Host: {:<16} | RAM: {:.1} / {:.1} GB ({:.1}%) | Daemon: {}\r\n Registered Servers: {:<4} | Active Running: {:<4}\r\n{}",
        box_top(width).cyan().bold(),
        box_title("CRAFT SERVER MANAGER DASHBOARD", width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        os.white().bold(),
        used_ram,
        total_ram,
        ram_pct,
        daemon_badge,
        registered_count.to_string().cyan().bold(),
        running_count.to_string().green().bold(),
        box_divider(width).dimmed(),
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
                    || craft_core::is_server_locked(&s.path)
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
            MenuEntry::new("1", "Local Servers"),
            MenuEntry::new("2", "Remote Servers"),
            MenuEntry::new("3", "Create New Server").with_aliases(&["c", "n"]),
            MenuEntry::new("4", "Tools & Utilities").with_aliases(&["t", "u"]),
            MenuEntry::new("0", "Exit Craft").with_aliases(&["q"]),
        ];

        let selection = run_menu(&header, &entries, &mut selected_main)?;

        match selection {
            Some(0) => {
                manage_servers_menu(paths).await?;
            }
            Some(1) => {
                remote_servers_menu(paths).await?;
            }
            Some(2) => {
                gui_create_server_wizard(paths).await?;
            }
            Some(3) => {
                tools_menu(paths).await?;
            }
            Some(4) | None => {
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

