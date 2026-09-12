use std::io::{self, Write};
use colored::Colorize;

use craft_backup::BackupEngine;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

use crate::commands::run::run_foreground_server;
use crate::commands::view::handle_view;
use super::screen::{
    box_divider, box_title, box_top, exec_console_action, get_content_width, print_in_place_status,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry,
};
use super::wizard::gui_create_server_wizard;

pub async fn show_empty_servers_modal(paths: &CraftPaths) -> Result<bool> {
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  No servers are currently registered on this machine.\r\n  Create your first Minecraft server to get started.\r\n{}",
        box_top(width),
        box_title("NO SERVERS REGISTERED", width, false),
        box_divider(width),
        box_divider(width)
    );

    let entries = vec![
        MenuEntry::new("1", "Create Your First Server (Setup Wizard)").with_aliases(&["c", "n"]),
        MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]),
    ];

    let mut selected = 0;
    let choice = run_menu(&header, &entries, &mut selected)?;
    match choice {
        Some(0) => {
            gui_create_server_wizard(paths).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(crate) async fn start_server_daemon(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry
        .find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    DaemonClient::ensure_daemon_started(paths).await?;
    let mut client = DaemonClient::connect(paths).await?;
    client.start_server(&server.path).await?;
    Ok(())
}

pub(crate) async fn stop_server_daemon(
    server_name: &str,
    force: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry
        .find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    if !DaemonClient::is_daemon_running(paths) {
        if let Some(pid) = craft_core::get_server_running_pid(&server.path) {
            craft_core::kill_process(pid, force)?;
            return Ok(());
        }
        return Err(CraftError::Other(
            "Daemon is not running; no background servers active.".to_string(),
        ));
    }
    let mut client = DaemonClient::connect(paths).await?;
    client.stop_server(&server.path, force).await?;
    Ok(())
}

pub(crate) async fn restart_server_daemon(
    server_name: &str,
    force: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry
        .find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other(
            "Daemon is not running; no background servers active.".to_string(),
        ));
    }
    let mut client = DaemonClient::connect(paths).await?;
    let _ = client.stop_server(&server.path, force).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    client.start_server(&server.path).await?;
    Ok(())
}

pub async fn quick_start_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let running_paths = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut c) = DaemonClient::connect(paths).await {
            c.get_running().await.unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let is_running = running_paths.contains(&s.path)
            || s.path
                .canonicalize()
                .map(|p| running_paths.contains(&p))
                .unwrap_or(false)
            || craft_core::is_server_locked(&s.path);
        let status_badge = if is_running {
            "[ALREADY RUNNING]".green().to_string()
        } else {
            "[STOPPED]".dimmed().to_string()
        };
        let hotkey = if i < 9 {
            (i + 1).to_string()
        } else {
            ((b'a' + (i - 9) as u8) as char).to_string()
        };
        entries.push(MenuEntry::new(
            hotkey,
            format!(
                "{:<20} {:<10} {:<10} {}",
                s.name, s.software, s.version, status_badge
            ),
        ));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a server to start in the background:\r\n{}",
        box_top(width),
        box_title("QUICK START SERVER", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            let is_running = running_paths.contains(&server.path)
                || server
                    .path
                    .canonicalize()
                    .map(|p| running_paths.contains(&p))
                    .unwrap_or(false)
                || craft_core::is_server_locked(&server.path);
            if is_running {
                show_modal_message(
                    "SERVER ALREADY RUNNING",
                    &[
                        format!("Server '{}' is already running!", server.name),
                        "Use 'Attach Live Console' or 'Stop Server' from the dashboard."
                            .to_string(),
                    ],
                    false,
                )?;
            } else {
                print_in_place_status(
                    "STARTING SERVER",
                    &[format!(
                        "Starting server '{}' in background daemon...",
                        server.name
                    )],
                )?;
                match start_server_daemon(&server.name, paths).await {
                    Ok(_) => {
                        show_modal_message(
                            "SERVER STARTED",
                            &[
                                format!("[OK] Server '{}' started successfully!", server.name)
                                    .green()
                                    .bold()
                                    .to_string(),
                                format!("Platform: {} {}", server.software, server.version),
                                format!("Path:     {}", server.path.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "START FAILED",
                            &[format!(
                                "[ERROR] Failed to start server '{}': {}",
                                server.name, e
                            )],
                            true,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn stop_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let running_paths = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut c) = DaemonClient::connect(paths).await {
            c.get_running().await.unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let running_servers: Vec<_> = registry
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
        .collect();

    if running_servers.is_empty() {
        show_modal_message(
            "NO RUNNING SERVERS",
            &[
                "No servers are currently running on this host."
                    .yellow()
                    .to_string(),
                "Use 'Quick Start Server' to launch one.".to_string(),
            ],
            false,
        )?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in running_servers.iter().enumerate() {
        let hotkey = if i < 9 {
            (i + 1).to_string()
        } else {
            ((b'a' + (i - 9) as u8) as char).to_string()
        };
        let status_str = if let Some(pid) = craft_core::get_server_running_pid(&s.path) {
            format!("[RUNNING (PID: {})]", pid).green().bold().to_string()
        } else {
            "[RUNNING]".green().bold().to_string()
        };
        entries.push(MenuEntry::new(
            hotkey,
            format!(
                "{:<20} {:<10} {:<10} {}",
                s.name,
                s.software,
                s.version,
                status_str
            ),
        ));
    }
    entries.push(MenuEntry::new("a", "Stop ALL Running Servers"));
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a running server to stop gracefully:\r\n{}",
        box_top(width),
        box_title("STOP RUNNING SERVER", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < running_servers.len() {
            let server = running_servers[idx];
            print_in_place_status(
                "STOPPING SERVER",
                &[format!(
                    "Sending stop command to server '{}'...",
                    server.name
                )],
            )?;
            match stop_server_daemon(&server.name, false, paths).await {
                Ok(_) => {
                    show_modal_message(
                        "SERVER STOPPED",
                        &[format!(
                            "[OK] Server '{}' stopped successfully.",
                            server.name
                        )
                        .green()
                        .bold()
                        .to_string()],
                        false,
                    )?;
                }
                Err(e) => {
                    show_modal_message(
                        "STOP FAILED",
                        &[format!(
                            "[ERROR] Failed to stop server '{}': {}",
                            server.name, e
                        )],
                        true,
                    )?;
                }
            }
        } else if idx == running_servers.len() {
            // Stop ALL
            print_in_place_status(
                "STOPPING ALL SERVERS",
                &["Stopping all active background servers...".to_string()],
            )?;
            let mut stopped = 0;
            for s in &running_servers {
                let _ = stop_server_daemon(&s.name, false, paths).await;
                stopped += 1;
            }
            show_modal_message(
                "SERVERS STOPPED",
                &[format!("[OK] Stopped {} servers.", stopped)
                    .green()
                    .bold()
                    .to_string()],
                false,
            )?;
        }
    }
    Ok(())
}

pub async fn restart_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
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
            format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version),
        ));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a server to restart:\r\n{}",
        box_top(width),
        box_title("RESTART SERVER", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            print_in_place_status(
                "RESTARTING SERVER",
                &[format!("Restarting server '{}'...", server.name)],
            )?;
            match restart_server_daemon(&server.name, false, paths).await {
                Ok(_) => {
                    show_modal_message(
                        "SERVER RESTARTED",
                        &[format!(
                            "[OK] Server '{}' restarted successfully.",
                            server.name
                        )
                        .green()
                        .bold()
                        .to_string()],
                        false,
                    )?;
                }
                Err(e) => {
                    show_modal_message(
                        "RESTART FAILED",
                        &[format!(
                            "[ERROR] Failed to restart server '{}': {}",
                            server.name, e
                        )],
                        true,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub async fn view_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
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
            format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version),
        ));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a server to attach live terminal console:\r\n{}",
        box_top(width),
        box_title("ATTACH LIVE CONSOLE (VIEW)", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            let _ = exec_console_action(|| async {
                handle_view(&server.name, None, paths).await
            })
            .await;
        }
    }
    Ok(())
}

pub async fn rm_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
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
            format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version),
        ));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a server to unregister or delete:\r\n{}",
        box_top(width),
        box_title("DELETE SERVER", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];

            let confirm_header = format!(
                "{}\r\n{}\r\n{}\r\n  Server: {}\r\n  Path:   {}\r\n  Choose removal option:\r\n{}",
                box_top(width),
                box_title(&format!("REMOVE SERVER: {}", server.name), width, true),
                box_divider(width),
                server.name.white().bold(),
                server.path.display(),
                box_divider(width)
            );

            let confirm_entries = vec![
                MenuEntry::new(
                    "1",
                    "Unregister from Craft (Preserve world & server files on disk)",
                ),
                MenuEntry::new(
                    "2",
                    "Permanently Delete Server Directory & World Files (-rf)",
                ),
                MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
            ];

            let mut c_sel = 0;
            match run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                Some(0) => {
                    let mut reg = ServersRegistry::load(paths)?;
                    reg.remove(&server.path);
                    reg.save(paths)?;
                    show_modal_message(
                        "SERVER UNREGISTERED",
                        &[format!(
                            "[OK] Server '{}' unregistered from Craft registry. Files preserved.",
                            server.name
                        )
                        .green()
                        .bold()
                        .to_string()],
                        false,
                    )?;
                }
                Some(1) => {
                    let mut reg = ServersRegistry::load(paths)?;
                    reg.remove(&server.path);
                    reg.save(paths)?;
                    if server.path.exists() {
                        let _ = std::fs::remove_dir_all(&server.path);
                    }
                    show_modal_message(
                        "SERVER DELETED",
                        &[format!(
                            "[OK] Server '{}' and its directory permanently removed.",
                            server.name
                        )
                        .green()
                        .bold()
                        .to_string()],
                        false,
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub async fn manage_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
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
            let width = get_content_width(80);
            let header = format!(
                "{}\r\n{}\r\n{}\r\n No servers currently registered on this host.\r\n{}",
                box_top(width).cyan().bold(),
                box_title("REGISTERED SERVERS", width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
                box_divider(width).dimmed(),
            );

            // Defensive: Only option 1 and 0, aliases "c" / "n", strictly NO "2"
            let entries = vec![
                MenuEntry::new("1", "Create Your First Server (Setup Wizard)").with_aliases(&["c", "n"]),
                MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b"]),
            ];

            match run_menu(&header, &entries, &mut selected)? {
                Some(0) => {
                    gui_create_server_wizard(paths).await?;
                }
                _ => return Ok(()),
            }
            continue;
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a server to inspect details, control lifecycle, or attach console.\r\n{}",
            box_top(width).cyan().bold(),
            box_title("REGISTERED SERVERS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (idx, s) in registry.servers.iter().enumerate() {
            let is_running = running_paths.contains(&s.path)
                || s.path
                    .canonicalize()
                    .map(|p| running_paths.contains(&p))
                    .unwrap_or(false)
                || craft_core::is_server_locked(&s.path);
            let status_str = if is_running {
                if let Some(pid) = craft_core::get_server_running_pid(&s.path) {
                    format!("[RUNNING (PID: {})]", pid).green().bold().to_string()
                } else {
                    "[RUNNING]".green().bold().to_string()
                }
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
                format!(
                    "{:<20} {:<10} {:<10} {}",
                    s.name, s.software, s.version, status_str
                ),
            ));
        }

        entries.push(MenuEntry::new("n", "Create New Server (Setup Wizard)").with_aliases(&["c"]));
        entries.push(MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b"]));

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(idx) if idx < registry.servers.len() => {
                let chosen = &registry.servers[idx];
                server_control_panel(&chosen.name, paths).await?;
            }
            Some(idx) if idx == registry.servers.len() => {
                gui_create_server_wizard(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn server_control_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;
    let mut flash_status: Option<String> = None;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.servers.iter().find(|s| s.name == server_name) {
            Some(s) => s.clone(),
            None => {
                show_modal_message(
                    "SERVER NOT FOUND",
                    &[format!("Server '{}' is no longer registered.", server_name)],
                    true,
                )?;
                return Ok(());
            }
        };

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let is_running = if daemon_running {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                let running = c.get_running().await.unwrap_or_default();
                running.contains(&server.path)
                    || server
                        .path
                        .canonicalize()
                        .map(|p| running.contains(&p))
                        .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        } || craft_core::is_server_locked(&server.path);

        let running_pid = if is_running {
            craft_core::get_server_running_pid(&server.path)
        } else {
            None
        };

        let status_badge = if is_running {
            if let Some(pid) = running_pid {
                format!("[RUNNING (PID: {})]", pid).green().bold().to_string()
            } else {
                "[RUNNING]".green().bold().to_string()
            }
        } else {
            "[STOPPED]".dimmed().to_string()
        };

        let width = get_content_width(80);
        let title = format!("SERVER: {} {}", server.name, status_badge);
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n Platform: {:<12} | Version: {:<10} | Memory: {}\r\n Path: {}\r\n",
            box_top(width).cyan().bold(),
            box_title(&title, width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.software.white().bold(),
            server.version.cyan(),
            server.memory.as_deref().unwrap_or("Default (2G)"),
            server.path.display(),
        );

        if let Some(msg) = flash_status.take() {
            header.push_str(&format!(" {}\r\n", msg));
        }

        header.push_str(&box_divider(width).dimmed().to_string());

        let entries = vec![
            MenuEntry::new("1", "Start Server (Background Daemon)"),
            MenuEntry::new("2", "Start Server (Foreground Terminal)"),
            MenuEntry::new("3", "Stop Server"),
            MenuEntry::new("4", "Restart Server"),
            MenuEntry::new("5", "Attach Live Console (craft view)"),
            MenuEntry::new("6", "Create World Snapshot Backup"),
            MenuEntry::new("7", "List Existing Backups"),
            MenuEntry::new("8", "Delete / Unregister Server"),
            MenuEntry::new("0", "Back to Server List").with_aliases(&["b"]),
        ];

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(0) => {
                // Start background
                if is_running {
                    flash_status = Some(format!("[INFO] Server '{}' is already running.", server.name).yellow().bold().to_string());
                } else {
                    print_in_place_status(
                        "STARTING SERVER",
                        &[format!(
                            "Starting server '{}' in background daemon...",
                            server.name
                        )],
                    )?;
                    match start_server_daemon(&server.name, paths).await {
                        Ok(_) => {
                            flash_status = Some(
                                format!("[OK] Server '{}' started in daemon.", server.name)
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            flash_status = Some(
                                format!("[ERROR] Failed to start server: {}", e)
                                    .red()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Some(1) => {
                // Start foreground
                if is_running {
                    flash_status = Some(
                        format!("[INFO] Server '{}' is already running. Stop it before starting in foreground.", server.name)
                            .yellow()
                            .bold()
                            .to_string(),
                    );
                } else {
                    let server_path = server.path.clone();
                    let server_name = server.name.clone();
                    let res = exec_console_action(|| async {
                        let width = get_content_width(80);
                        let title = format!("SERVER CONSOLE (FOREGROUND): {}", server_name);
                        println!("{}", box_top(width).cyan().bold());
                        println!("{}", box_title(&title, width, false).cyan().bold());
                        println!("{}", box_divider(width).cyan().bold());
                        println!(" {}", "Type commands below  |  Press Ctrl+C or type 'stop' to safely stop the server".dimmed());
                        println!("{}\r\n", box_divider(width).dimmed());
                        let _ = io::stdout().flush();
                        run_foreground_server(&server_path).await
                    })
                    .await;

                    match res {
                        Ok(_) => {
                            flash_status = Some(
                                format!("[OK] Foreground server '{}' has stopped.", server_name)
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            flash_status = Some(
                                format!("[ERROR] Server process ended: {}", e)
                                    .red()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Some(2) => {
                // Stop
                if !is_running {
                    flash_status = Some(
                        format!("[INFO] Server '{}' is not currently running.", server.name)
                            .yellow()
                            .bold()
                            .to_string(),
                    );
                } else {
                    print_in_place_status(
                        "STOPPING SERVER",
                        &[format!("Stopping server '{}'...", server.name)],
                    )?;
                    match stop_server_daemon(&server.name, false, paths).await {
                        Ok(_) => {
                            flash_status = Some(
                                format!("[OK] Server '{}' stopped.", server.name)
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            flash_status = Some(
                                format!("[ERROR] Failed to stop server: {}", e)
                                    .red()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Some(3) => {
                // Restart
                print_in_place_status(
                    "RESTARTING SERVER",
                    &[format!("Restarting server '{}'...", server.name)],
                )?;
                match restart_server_daemon(&server.name, false, paths).await {
                    Ok(_) => {
                        flash_status = Some(
                            format!("[OK] Server '{}' restarted in daemon.", server.name)
                                .green()
                                .bold()
                                .to_string(),
                        );
                    }
                    Err(e) => {
                        flash_status = Some(
                            format!("[ERROR] Failed to restart server: {}", e)
                                .red()
                                .bold()
                                .to_string(),
                        );
                    }
                }
            }
            Some(4) => {
                // View live console
                if !is_running {
                    flash_status = Some(
                        format!("[INFO] Server '{}' is not currently running.", server.name)
                            .yellow()
                            .bold()
                            .to_string(),
                    );
                } else {
                    let server_name = server.name.clone();
                    let _ = exec_console_action(|| async {
                        let width = get_content_width(80);
                        let title = format!("ATTACHED CONSOLE: {}", server_name);
                        println!("{}", box_top(width).cyan().bold());
                        println!("{}", box_title(&title, width, false).cyan().bold());
                        println!("{}", box_divider(width).cyan().bold());
                        println!(" {}", "Type commands to send to server  |  Press Ctrl+C to detach and return to TUI".dimmed());
                        println!("{}\r\n", box_divider(width).dimmed());
                        let _ = io::stdout().flush();
                        handle_view(&server_name, None, paths).await
                    })
                    .await;
                    flash_status = Some(
                        format!("[OK] Detached from '{}' console.", server.name)
                            .green()
                            .bold()
                            .to_string(),
                    );
                }
            }
            Some(5) => {
                // Create backup
                print_in_place_status(
                    "CREATING BACKUP",
                    &[format!("Creating snapshot for server '{}'...", server.name)],
                )?;
                let engine = BackupEngine::new(paths);
                match engine
                    .create_backup(&server.name, &server.path, None, false)
                    .await
                {
                    Ok(file) => {
                        flash_status = Some(
                            format!(
                                "[OK] Backup archive created: {}",
                                file.file_name().unwrap_or_default().to_string_lossy()
                            )
                            .green()
                            .bold()
                            .to_string(),
                        );
                    }
                    Err(e) => {
                        flash_status = Some(
                            format!("[ERROR] Backup failed: {}", e)
                                .red()
                                .bold()
                                .to_string(),
                        );
                    }
                }
            }
            Some(6) => {
                // List backups
                let engine = BackupEngine::new(paths);
                let list = engine.list_backups(&server.name);
                if list.is_empty() {
                    show_modal_message(
                        "NO BACKUPS",
                        &[format!(
                            "No existing backups found for server '{}'.",
                            server.name
                        )],
                        false,
                    )?;
                } else {
                    let lines: Vec<String> = list
                        .iter()
                        .map(|b| {
                            let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                            format!("{:<40} {:>8.2} MB  {}", b.filename, mb, b.created_at)
                        })
                        .collect();
                    show_modal_message("EXISTING BACKUPS", &lines, false)?;
                }
            }
            Some(7) => {
                // Delete server
                let width = get_content_width(80);
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Are you sure you want to remove server '{}'?\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("DELETE SERVER: {}", server.name), width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server.name,
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new(
                        "1",
                        "Unregister from Craft (Preserve world & server files)",
                    ),
                    MenuEntry::new("2", "Permanently Delete Server Directory & Files (-rf)"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut c_sel = 0;
                match run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    Some(0) => {
                        let mut reg = ServersRegistry::load(paths)?;
                        reg.remove(&server.path);
                        reg.save(paths)?;
                        show_modal_message(
                            "SERVER UNREGISTERED",
                            &[format!(
                                "[OK] Server '{}' unregistered. Files kept on disk.",
                                server.name
                            )],
                            false,
                        )?;
                        return Ok(());
                    }
                    Some(1) => {
                        let mut reg = ServersRegistry::load(paths)?;
                        reg.remove(&server.path);
                        reg.save(paths)?;
                        if server.path.exists() {
                            let _ = std::fs::remove_dir_all(&server.path);
                        }
                        show_modal_message(
                            "SERVER DELETED",
                            &[format!(
                                "[OK] Server '{}' and directory removed.",
                                server.name
                            )],
                            false,
                        )?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            _ => return Ok(()),
        }
    }
}
