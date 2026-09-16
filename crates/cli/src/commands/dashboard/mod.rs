pub mod screen;
pub mod server_control;
pub mod tools;
pub mod wizard;
pub mod remote_tui;
pub mod cloud_backups;
pub mod trash_tui;
pub mod properties_tui;
pub mod worlds_tui;
pub mod developer_tui;

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
pub use trash_tui::*;

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

    let title = if let Some(alias) = get_remote_node() {
        format!("REMOTE HOST: {}", alias)
    } else {
        "CRAFT SERVER MANAGER DASHBOARD".to_string()
    };

    let host_label = if let Some(alias) = get_remote_node() {
        alias
    } else {
        os.to_string()
    };

    let width = get_content_width(80);
    format!(
        "{}\r\n{}\r\n{}\r\n Host: {:<16} | RAM: {:.1} / {:.1} GB ({:.1}%) | Daemon: {}\r\n Registered Servers: {:<4} | Active Running: {:<4}\r\n{}",
        box_top(width).cyan().bold(),
        box_title(&title, width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        host_label.white().bold(),
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
    let _nav = if let Some(alias) = get_remote_node() {
        NavGuard::enter(alias)
    } else {
        NavGuard::enter("Dashboard")
    };
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

        let cache_bytes = craft_providers::CacheManager::new(paths).get_cache_size();
        let cache_mb = (cache_bytes as f64) / (1024.0 * 1024.0);
        let purge_label = format!("Purge Cache ({:.1} MB)", cache_mb);

        let trash_count = craft_core::TrashManager::new(paths)
            .list_items()
            .map(|i| i.len())
            .unwrap_or(0);
        let trash_label = if trash_count > 0 {
            format!("Trash Bin ({} items)", trash_count)
        } else {
            "Trash Bin".to_string()
        };

        let entries = if is_remote_node() {
            let count_sub = match total_servers {
                0 => "0 registered".dimmed().to_string(),
                1 => "1 server registered".to_string(),
                n => format!("{} servers registered", n),
            };
            vec![
                MenuEntry::new("1", format!("{:<24} ({})", "Manage Servers", count_sub))
                    .with_aliases(&["s", "servers", "m"]),
                MenuEntry::new("2", "New Server").with_aliases(&["n", "c", "create", "new"]),
                MenuEntry::new("3", "Backup Systems").with_aliases(&["b"]),
                MenuEntry::new("4", "Diagnostic & System Tools").with_aliases(&["p", "t", "tools"]),
                MenuEntry::new("5", "Daemon Control").with_aliases(&["d"]),
                MenuEntry::new("6", purge_label).with_aliases(&["k", "c"]),
                MenuEntry::new("7", trash_label).with_aliases(&["t"]),
                MenuEntry::new("0", "Back").with_aliases(&["q", "b"]),
            ]
        } else {
            vec![
                MenuEntry::new("1", "Local Servers"),
                MenuEntry::new("2", "Remote Servers"),
                MenuEntry::new("3", "Backup Systems").with_aliases(&["b"]),
                MenuEntry::new("4", "Diagnostic & System Tools").with_aliases(&["p", "t", "tools"]),
                MenuEntry::new("5", "Daemon Control").with_aliases(&["d"]),
                MenuEntry::new("6", purge_label).with_aliases(&["c"]),
                MenuEntry::new("7", trash_label).with_aliases(&["t"]),
                MenuEntry::new("0", "Exit").with_aliases(&["q"]),
            ]
        };

        let selection = run_main_menu(&header, &entries, &mut selected_main)?;

        if is_remote_node() {
            match selection {
                Some(0) => {
                    manage_servers_menu(paths).await?;
                }
                Some(1) => {
                    gui_create_server_wizard(paths).await?;
                }
                Some(2) => {
                    cloud_backups::setup_backup_systems_menu(paths).await?;
                }
                Some(3) => {
                    tools_menu(paths).await?;
                }
                Some(4) => {
                    daemon_menu(paths).await?;
                }
                Some(5) => {
                    let width = get_content_width(80);
                    let confirm_header = format!(
                        "{}\r\n{}\r\n{}\r\n Are you sure you want to purge the download cache?\r\n Total Cache Size: {:.2} MB\r\n Location: {}\r\n{}",
                        box_top(width).yellow().bold(),
                        box_title("PURGE CACHE", width, false).yellow().bold(),
                        box_divider(width).yellow().bold(),
                        cache_mb,
                        paths.cache_dir.display(),
                        box_divider(width).dimmed(),
                    );
                    let confirm_entries = vec![
                        MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                        MenuEntry::new("2", "Confirm Purge Cache"),
                    ];
                    let mut c_sel = 0;
                    if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                        match craft_providers::CacheManager::new(paths).clean_cache() {
                            Ok(freed) => {
                                let freed_mb = (freed as f64) / (1024.0 * 1024.0);
                                show_modal_message(
                                    "CACHE PURGED",
                                    &[format!("[OK] Cleared {:.2} MB of downloaded caches.", freed_mb).green().bold().to_string()],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message("ERROR", &[format!("[ERROR] Failed to purge cache: {}", e)], true)?;
                            }
                        }
                    }
                }
                Some(6) => {
                    trash_bin_menu(paths).await?;
                }
                _ => break,
            }
        } else {
            match selection {
                Some(0) => {
                    manage_servers_menu(paths).await?;
                }
                Some(1) => {
                    remote_servers_menu(paths).await?;
                }
                Some(2) => {
                    cloud_backups::setup_backup_systems_menu(paths).await?;
                }
                Some(3) => {
                    tools_menu(paths).await?;
                }
                Some(4) => {
                    daemon_menu(paths).await?;
                }
                Some(5) => {
                    let width = get_content_width(80);
                    let confirm_header = format!(
                        "{}\r\n{}\r\n{}\r\n Are you sure you want to purge the download cache?\r\n Total Cache Size: {:.2} MB\r\n Location: {}\r\n{}",
                        box_top(width).yellow().bold(),
                        box_title("PURGE CACHE", width, false).yellow().bold(),
                        box_divider(width).yellow().bold(),
                        cache_mb,
                        paths.cache_dir.display(),
                        box_divider(width).dimmed(),
                    );
                    let confirm_entries = vec![
                        MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                        MenuEntry::new("2", "Confirm Purge Cache"),
                    ];
                    let mut c_sel = 0;
                    if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                        match craft_providers::CacheManager::new(paths).clean_cache() {
                            Ok(freed) => {
                                let freed_mb = (freed as f64) / (1024.0 * 1024.0);
                                show_modal_message(
                                    "CACHE PURGED",
                                    &[format!("[OK] Cleared {:.2} MB of downloaded caches.", freed_mb).green().bold().to_string()],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message("ERROR", &[format!("[ERROR] Failed to purge cache: {}", e)], true)?;
                            }
                        }
                    }
                }
                Some(6) => {
                    trash_bin_menu(paths).await?;
                }
                _ => break,
            }
        }
    }

    Ok(())
}

