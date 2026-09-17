use colored::Colorize;

use craft_backup::{BackupEngine, GDriveStorageProvider, S3StorageProvider, StorageProvider};
use craft_core::{
    CraftError, CraftPaths, GlobalBackupRegistry, Result, ServersRegistry, TrashManager,
};
use craft_daemon::DaemonClient;

use super::screen::{
    box_divider, box_title, box_top, exec_console_action, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_paged_list_menu, show_modal_message, AltScreenGuard, MenuEntry,
    NavGuard, PagedMenuAction,
};
use super::wizard::gui_create_server_wizard;
use crate::commands::view::handle_view;

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
        MenuEntry::new("1", "Create Server").with_aliases(&["c", "n"]),
        MenuEntry::new("0", "Back").with_aliases(&["b"]),
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
    let mut sel = 0;
    let mut flash_status: Option<String> = None;

    loop {
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
            let game_badge = format!("[{}]", s.game_definition().name.to_uppercase())
                .magenta()
                .to_string();
            entries.push(MenuEntry::new(
                hotkey,
                format!(
                    "{:<18} {:<12} {:<10} {:<10} {}",
                    s.name, game_badge, s.software, s.version, status_badge
                ),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

        let width = get_content_width(80);
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n  Select a server to start in the background:\r\n",
            box_top(width).cyan().bold(),
            box_title("QUICK START SERVER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
        );
        if let Some(msg) = flash_status.take() {
            header.push_str(&format!("  {}\r\n", msg));
        }
        header.push_str(&box_divider(width).dimmed().to_string());

        match run_menu(&header, &entries, &mut sel)? {
            Some(idx) if idx < registry.servers.len() => {
                let server = &registry.servers[idx];
                let is_running = running_paths.contains(&server.path)
                    || server
                        .path
                        .canonicalize()
                        .map(|p| running_paths.contains(&p))
                        .unwrap_or(false)
                    || craft_core::is_server_locked(&server.path);
                if is_running {
                    flash_status = Some(
                        format!("[INFO] Server '{}' is already running.", server.name)
                            .yellow()
                            .bold()
                            .to_string(),
                    );
                } else {
                    let _ = print_in_place_status(
                        "STARTING SERVER",
                        &[
                            format!("Initializing background daemon for '{}'...", server.name),
                            "Launching server process in supervisor...".to_string(),
                        ],
                    );
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
            _ => return Ok(()),
        }
    }
}

pub async fn stop_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut sel = 0;
    let mut flash_status: Option<String> = None;

    loop {
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
            let width = get_content_width(80);
            let mut header = format!(
                "{}\r\n{}\r\n{}\r\n",
                box_top(width).cyan().bold(),
                box_title("STOP RUNNING SERVER", width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
            );
            if let Some(msg) = flash_status {
                header.push_str(&format!("  {}\r\n", msg));
            }
            header.push_str("  No servers are currently running on this host.\r\n");
            header.push_str(&box_divider(width).dimmed().to_string());

            let entries = vec![MenuEntry::new("0", "Back").with_aliases(&["b"])];
            let mut exit_sel = 0;
            let _ = run_menu(&header, &entries, &mut exit_sel)?;
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
                format!("[RUNNING (PID: {})]", pid)
                    .green()
                    .bold()
                    .to_string()
            } else {
                "[RUNNING]".green().bold().to_string()
            };
            let game_badge = format!("[{}]", s.game_definition().name.to_uppercase())
                .magenta()
                .to_string();
            entries.push(MenuEntry::new(
                hotkey,
                format!(
                    "{:<18} {:<12} {:<10} {:<10} {}",
                    s.name, game_badge, s.software, s.version, status_str
                ),
            ));
        }
        if running_servers.len() > 1 {
            entries.push(MenuEntry::new("a", "Stop All Servers"));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

        let width = get_content_width(80);
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n  Select a running server to stop gracefully:\r\n",
            box_top(width).cyan().bold(),
            box_title("STOP RUNNING SERVER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
        );
        if let Some(msg) = flash_status.take() {
            header.push_str(&format!("  {}\r\n", msg));
        }
        header.push_str(&box_divider(width).dimmed().to_string());

        match run_menu(&header, &entries, &mut sel)? {
            Some(idx) if idx < running_servers.len() => {
                let server = running_servers[idx];
                let _ = print_in_place_status(
                    "STOPPING SERVER",
                    &[
                        format!("Stopping server '{}' gracefully...", server.name),
                        "Saving worlds and player states...".to_string(),
                        "Waiting for process termination...".to_string(),
                    ],
                );
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
            Some(idx) if running_servers.len() > 1 && idx == running_servers.len() => {
                // Stop ALL
                let _ = print_in_place_status(
                    "STOPPING SERVERS",
                    &[
                        "Stopping all running servers gracefully...".to_string(),
                        "Saving worlds and player states...".to_string(),
                    ],
                );
                let mut stopped = 0;
                for s in &running_servers {
                    let _ = stop_server_daemon(&s.name, false, paths).await;
                    stopped += 1;
                }
                flash_status = Some(
                    format!("[OK] Stopped {} servers.", stopped)
                        .green()
                        .bold()
                        .to_string(),
                );
            }
            _ => return Ok(()),
        }
    }
}

pub async fn restart_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut sel = 0;
    let mut flash_status: Option<String> = None;

    loop {
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
            let game_badge = format!("[{}]", s.game_definition().name.to_uppercase())
                .magenta()
                .to_string();
            entries.push(MenuEntry::new(
                hotkey,
                format!(
                    "{:<18} {:<12} {:<10} {:<10}",
                    s.name, game_badge, s.software, s.version
                ),
            ));
        }
        if registry.servers.len() > 1 {
            entries.push(MenuEntry::new("a", "Restart All Servers"));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

        let width = get_content_width(80);
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n  Select a server to restart:\r\n",
            box_top(width).cyan().bold(),
            box_title("RESTART SERVER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
        );
        if let Some(msg) = flash_status.take() {
            header.push_str(&format!("  {}\r\n", msg));
        }
        header.push_str(&box_divider(width).dimmed().to_string());

        match run_menu(&header, &entries, &mut sel)? {
            Some(idx) if idx < registry.servers.len() => {
                let server = &registry.servers[idx];
                let _ = print_in_place_status(
                    "RESTARTING SERVER",
                    &[
                        format!("Restarting server '{}' gracefully...", server.name),
                        "Saving worlds and re-launching daemon...".to_string(),
                    ],
                );
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
            Some(idx) if registry.servers.len() > 1 && idx == registry.servers.len() => {
                // Restart ALL
                let _ = print_in_place_status(
                    "RESTARTING SERVERS",
                    &[
                        "Restarting all servers gracefully...".to_string(),
                        "Saving worlds and player states...".to_string(),
                    ],
                );
                let mut restarted = 0;
                for s in &registry.servers {
                    let _ = restart_server_daemon(&s.name, false, paths).await;
                    restarted += 1;
                }
                flash_status = Some(
                    format!("[OK] Restarted {} servers.", restarted)
                        .green()
                        .bold()
                        .to_string(),
                );
            }
            _ => return Ok(()),
        }
    }
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
    entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

    let mut sel = 0;
    let width = get_content_width(80);
    let header = format!(
        "{}\r\n{}\r\n{}\r\n  Select a server to attach live terminal console:\r\n{}",
        box_top(width),
        box_title("LIVE CONSOLE", width, false),
        box_divider(width),
        box_divider(width)
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            let _ = exec_console_action(|| async { handle_view(&server.name, None, paths).await })
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
    entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

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
                MenuEntry::new("1", "Unregister (Keep Files)"),
                MenuEntry::new("2", "Delete Server & Files"),
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
                    let second_header = format!(
                        "{}\r\n{}\r\n{}\r\n WARNING: This will permanently erase server '{}' and ALL world data!\r\n Directory: {}\r\n This action is IRREVERSIBLE and CANNOT be undone.\r\n{}\r\n Are you ABSOLUTELY sure you want to proceed?\r\n{}",
                        box_top(width).red().bold(),
                        box_title("FINAL CONFIRMATION: PERMANENT REMOVAL", width, false).red().bold(),
                        box_divider(width).red().bold(),
                        server.name.red().bold(),
                        server.path.display(),
                        box_divider(width).dimmed(),
                        box_divider(width).dimmed(),
                    );
                    let second_entries = vec![
                        MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                        MenuEntry::new("2", format!("Confirm Delete '{}'", server.name)),
                    ];
                    let mut second_sel = 0;
                    if let Some(1) = run_menu(&second_header, &second_entries, &mut second_sel)? {
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
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub async fn manage_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let nav_label = if super::screen::is_remote_node() {
        "Servers"
    } else {
        "Local Servers"
    };
    let _nav = NavGuard::enter(nav_label);
    let mut selected = 0;
    let mut current_page = 0;
    let page_size = 7;

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
            let title = if let Some(alias) = super::screen::get_remote_node() {
                format!("REMOTE SERVERS: {}", alias)
            } else {
                "LOCAL SERVERS (HOST)".to_string()
            };
            let desc = if let Some(alias) = super::screen::get_remote_node() {
                format!(
                    " No servers currently registered on remote host '{}'.\r\n",
                    alias
                )
            } else {
                " No servers currently registered on this local host.\r\n".to_string()
            };
            let header = format!(
                "{}\r\n{}\r\n{}\r\n{}{}",
                box_top(width).cyan().bold(),
                box_title(&title, width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
                desc,
                box_divider(width).dimmed(),
            );

            // Defensive: Only option 1 and 0, aliases "c" / "n", strictly NO "2"
            let entries = vec![
                MenuEntry::new("1", "New Server").with_aliases(&["c", "n", "create", "new"]),
                MenuEntry::new("0", "Back").with_aliases(&["b"]),
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
        let action_entries =
            vec![MenuEntry::new("n", "New Server").with_aliases(&["c", "create", "new"])];

        let action = super::screen::run_paged_list_menu(
            &registry.servers,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages)
                        .cyan()
                        .to_string()
                } else {
                    "".to_string()
                };
                let title = if let Some(alias) = super::screen::get_remote_node() {
                    format!("REMOTE SERVERS: {}", alias)
                } else {
                    "LOCAL SERVERS".to_string()
                };
                let desc = if let Some(alias) = super::screen::get_remote_node() {
                    format!(
                        " Manage servers on remote host '{}' (Total: {}){}.\r\n",
                        alias, total_count, page_info
                    )
                } else {
                    format!(
                        " Manage local servers on this host (Total: {}){}.\r\n",
                        total_count, page_info
                    )
                };
                format!(
                    "{}\r\n{}\r\n{}\r\n{}{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    desc,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, s| {
                let is_running = running_paths.contains(&s.path)
                    || s.path
                        .canonicalize()
                        .map(|p| running_paths.contains(&p))
                        .unwrap_or(false)
                    || craft_core::is_server_locked(&s.path);
                let status_str = if is_running {
                    if let Some(pid) = craft_core::get_server_running_pid(&s.path) {
                        format!("[RUNNING (PID: {})]", pid)
                            .green()
                            .bold()
                            .to_string()
                    } else {
                        "[RUNNING]".green().bold().to_string()
                    }
                } else {
                    "[STOPPED]".dimmed().to_string()
                };
                let game_badge = format!("[{}]", s.game_definition().name.to_uppercase())
                    .magenta()
                    .to_string();
                format!(
                    "{:<18} {:<12} {:<10} {:<10} {}",
                    s.name, game_badge, s.software, s.version, status_str
                )
            },
            &action_entries,
            true,
        )?;

        match action {
            super::screen::PagedMenuAction::Select(global_idx)
                if global_idx < registry.servers.len() =>
            {
                let chosen = &registry.servers[global_idx];
                server_control_panel(&chosen.name, paths).await?;
            }
            super::screen::PagedMenuAction::Space(global_idx)
                if global_idx < registry.servers.len() =>
            {
                let chosen = &registry.servers[global_idx];
                let is_running = running_paths.contains(&chosen.path)
                    || chosen
                        .path
                        .canonicalize()
                        .map(|p| running_paths.contains(&p))
                        .unwrap_or(false)
                    || craft_core::is_server_locked(&chosen.path);
                if is_running {
                    let _ = print_in_place_status(
                        "STOPPING SERVER",
                        &[format!("Stopping '{}' gracefully...", chosen.name)],
                    );
                    let _ = stop_server_daemon(&chosen.name, false, paths).await;
                } else {
                    let _ = print_in_place_status(
                        "STARTING SERVER",
                        &[format!("Starting '{}' in background...", chosen.name)],
                    );
                    let _ = start_server_daemon(&chosen.name, paths).await;
                }
            }
            super::screen::PagedMenuAction::Action(act) if act == "n" => {
                gui_create_server_wizard(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn server_control_panel(
    initial_server_name: &str,
    paths: &CraftPaths,
) -> Result<()> {
    let mut selected = 0;
    let mut current_server_name = initial_server_name.to_string();
    let mut flash_status: Option<String> = None;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry
            .servers
            .iter()
            .find(|s| s.name == current_server_name)
        {
            Some(s) => s.clone(),
            None => {
                show_modal_message(
                    "SERVER NOT FOUND",
                    &[format!(
                        "Server '{}' is no longer registered.",
                        current_server_name
                    )],
                    true,
                )?;
                return Ok(());
            }
        };
        let _nav = NavGuard::enter(&server.name);

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
                format!("[RUNNING (PID: {})]", pid)
                    .green()
                    .bold()
                    .to_string()
            } else {
                "[RUNNING]".green().bold().to_string()
            }
        } else {
            "[STOPPED]".dimmed().to_string()
        };

        let width = get_content_width(80);
        let title_prefix = if super::screen::is_remote_node() {
            "REMOTE SERVER"
        } else {
            "SERVER"
        };
        let title = format!("{}: {} {}", title_prefix, server.name, status_badge);
        let remote_line = if let Some(alias) = super::screen::get_remote_node() {
            format!(" Remote Host: {}\r\n", alias.cyan().bold())
        } else {
            String::new()
        };
        let path_label = if super::screen::is_remote_node() {
            "Remote Path"
        } else {
            "Path"
        };
        let game_def = server.game_definition();
        let port_display = server
            .port
            .map(|p| p.to_string())
            .unwrap_or_else(|| "Default".to_string());
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n Game: {:<14} | Software: {:<12} | Version: {:<10} | Port: {}\r\n{}{}: {}\r\n",
            box_top(width).cyan().bold(),
            box_title(&title, width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            game_def.name.magenta().bold(),
            server.software.white().bold(),
            server.version.cyan(),
            port_display.yellow(),
            remote_line,
            path_label,
            server.path.display(),
        );

        if let Some(msg) = flash_status.take() {
            header.push_str(&format!(" {}\r\n", msg));
        }

        header.push_str(&box_divider(width).dimmed().to_string());

        enum ControlAction {
            ToggleStartStop,
            Restart,
            AttachConsole,
            ServerProperties,
            ManageWorlds,
            ContentManagement,
            DeveloperTools,
            Backups,
            Maintenance,
        }

        let mut entries = Vec::new();
        let mut actions = Vec::new();

        if is_running {
            entries.push(MenuEntry::new("1", "Stop Server"));
            actions.push(ControlAction::ToggleStartStop);

            entries.push(MenuEntry::new("2", "Restart Server"));
            actions.push(ControlAction::Restart);

            entries.push(MenuEntry::new("3", "Live Console"));
            actions.push(ControlAction::AttachConsole);
        } else {
            entries.push(MenuEntry::new("1", "Start Server"));
            actions.push(ControlAction::ToggleStartStop);
        }

        let prop_hotkey = (actions.len() + 1).to_string();
        entries.push(
            MenuEntry::new(prop_hotkey, "Server Properties (Config Editor)")
                .with_aliases(&["prop", "props", "cfg"]),
        );
        actions.push(ControlAction::ServerProperties);

        let wrd_hotkey = (actions.len() + 1).to_string();
        entries.push(
            MenuEntry::new(wrd_hotkey, "Manage Worlds & Level Data")
                .with_aliases(&["w", "worlds", "world"]),
        );
        actions.push(ControlAction::ManageWorlds);

        let caps = craft_providers::get_content_capabilities(&server.software);
        if caps.has_any() {
            let cnt_label = if caps.plugins && caps.mods && caps.datapacks {
                "Content (Plugins, Mods, Datapacks)"
            } else if caps.plugins && caps.datapacks {
                "Content (Plugins, Datapacks)"
            } else if caps.mods && caps.datapacks {
                "Content (Mods, Datapacks)"
            } else if caps.plugins && !caps.datapacks {
                "Content (Plugins)"
            } else if caps.datapacks && !caps.plugins && !caps.mods {
                "Content (Datapacks)"
            } else if caps.mods && !caps.datapacks {
                "Content (Mods)"
            } else {
                "Content Management"
            };
            let cnt_hotkey = (actions.len() + 1).to_string();
            entries.push(MenuEntry::new(cnt_hotkey, cnt_label).with_aliases(&["c", "content"]));
            actions.push(ControlAction::ContentManagement);
        }

        let dev_hotkey = (actions.len() + 1).to_string();
        entries
            .push(MenuEntry::new(dev_hotkey, "Developer Tools").with_aliases(&["dev", "develop"]));
        actions.push(ControlAction::DeveloperTools);

        let bkp_hotkey = (actions.len() + 1).to_string();
        entries.push(MenuEntry::new(bkp_hotkey, "Backup Systems").with_aliases(&["bkp", "backup"]));
        actions.push(ControlAction::Backups);

        let mnt_hotkey = (actions.len() + 1).to_string();
        entries.push(
            MenuEntry::new(mnt_hotkey, "Server Maintenance (Fix, Rename, Delete)")
                .with_aliases(&["m", "maintenance"]),
        );
        actions.push(ControlAction::Maintenance);

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        let sel = run_menu(&header, &entries, &mut selected)?;

        let action = match sel {
            Some(idx) if idx < actions.len() => &actions[idx],
            _ => return Ok(()),
        };

        match action {
            ControlAction::ToggleStartStop => {
                if is_running {
                    let _ = print_in_place_status(
                        "STOPPING SERVER",
                        &[
                            format!("Stopping server '{}' gracefully...", server.name),
                            "Saving world and player data...".to_string(),
                            "Waiting for process termination...".to_string(),
                        ],
                    );
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
                } else {
                    let _ = print_in_place_status(
                        "STARTING SERVER",
                        &[
                            format!("Starting server '{}' in background daemon...", server.name),
                            "Initializing supervisor process...".to_string(),
                        ],
                    );
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
            ControlAction::Restart => {
                let _ = print_in_place_status(
                    "RESTARTING SERVER",
                    &[
                        format!("Stopping server '{}' gracefully...", server.name),
                        "Re-launching server via background daemon...".to_string(),
                    ],
                );
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
            ControlAction::AttachConsole => {
                let server_name = server.name.clone();
                let server_path = server.path.clone();
                let res =
                    super::screen::run_virtual_console(&server_name, &server_path, paths).await;
                match res {
                    Ok(_) => {
                        flash_status = Some(
                            format!("[OK] Detached from '{}' console.", server_name)
                                .green()
                                .bold()
                                .to_string(),
                        );
                    }
                    Err(e) => {
                        flash_status = Some(
                            format!("[ERROR] Console session: {}", e)
                                .red()
                                .bold()
                                .to_string(),
                        );
                    }
                }
            }
            ControlAction::ServerProperties => {
                super::properties_tui::server_properties_editor(&server.path, &server.name).await?;
            }
            ControlAction::ManageWorlds => {
                super::worlds_tui::manage_installed_worlds_menu(&server).await?;
            }
            ControlAction::ContentManagement => {
                server_content_menu(&server, paths).await?;
            }
            ControlAction::DeveloperTools => {
                super::developer_tui::developer_tools_menu(&server, paths).await?;
            }
            ControlAction::Backups => {
                server_backups_panel(&server.name, paths).await?;
            }
            ControlAction::Maintenance => {
                match server_maintenance_menu(&server, paths, is_running).await? {
                    MaintenanceOutcome::Renamed(new_name) => {
                        flash_status = Some(
                            format!(
                                "[OK] Server renamed from '{}' to '{}'.",
                                current_server_name, new_name
                            )
                            .green()
                            .bold()
                            .to_string(),
                        );
                        current_server_name = new_name;
                    }
                    MaintenanceOutcome::Deleted => {
                        return Ok(());
                    }
                    MaintenanceOutcome::None => {}
                }
            }
        }
    }
}

pub(crate) async fn server_content_menu(
    server: &craft_core::ServerConfig,
    paths: &CraftPaths,
) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Content");
    let mut selected = 0;

    let caps = craft_providers::get_content_capabilities(&server.software);
    if !caps.has_any() {
        show_modal_message(
            "CONTENT MANAGEMENT NOT SUPPORTED",
            &[
                format!("Server '{}' (software: {}) does not support plugins, mods, or datapacks.", server.name, server.software),
                "".to_string(),
                "Dedicated server content packages are only available for compatible software engines.".yellow().to_string(),
            ],
            false,
        )?;
        return Ok(());
    }

    #[derive(Clone, Copy)]
    enum ContentPanelKind {
        Plugins,
        Mods,
        Datapacks,
    }

    loop {
        let width = get_content_width(80);
        let header =
            format!(
            "{}\r\n{}\r\n{}\r\n Server:   {}\r\n Software: {} {}\r\n Select content manager:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("SERVER CONTENT: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            server.software.cyan(),
            server.version,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        let mut kinds = Vec::new();

        if caps.plugins {
            let plugins_dir = server.path.join("plugins");
            let count = list_server_plugins(&plugins_dir).len();
            let key = (entries.len() + 1).to_string();
            entries.push(
                MenuEntry::new(key, format!("Plugins (Installed: {})", count))
                    .with_aliases(&["p", "plugin", "plugins"]),
            );
            kinds.push(ContentPanelKind::Plugins);
        }

        if caps.mods {
            let mods_dir = server.path.join("mods");
            let count = list_server_mods(&mods_dir).len();
            let key = (entries.len() + 1).to_string();
            entries.push(
                MenuEntry::new(key, format!("Mods (Installed: {})", count))
                    .with_aliases(&["m", "mod", "mods"]),
            );
            kinds.push(ContentPanelKind::Mods);
        }

        if caps.datapacks {
            let default_world = craft_core::get_default_world(&server.path);
            let datapacks_dir = server.path.join(&default_world).join("datapacks");
            let count = list_server_datapacks(&datapacks_dir).len();
            let key = (entries.len() + 1).to_string();
            entries.push(
                MenuEntry::new(key, format!("Datapacks (Installed: {})", count)).with_aliases(&[
                    "d",
                    "datapack",
                    "datapacks",
                ]),
            );
            kinds.push(ContentPanelKind::Datapacks);
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < kinds.len() => match kinds[idx] {
                ContentPanelKind::Plugins => {
                    server_plugins_panel(&server.name, paths).await?;
                }
                ContentPanelKind::Mods => {
                    server_mods_panel(&server.name, paths).await?;
                }
                ContentPanelKind::Datapacks => {
                    server_datapacks_panel(&server.name, paths).await?;
                }
            },
            _ => return Ok(()),
        }
    }
}

pub(crate) enum MaintenanceOutcome {
    None,
    Renamed(String),
    Deleted,
}

pub(crate) async fn server_maintenance_menu(
    server: &craft_core::ServerConfig,
    paths: &CraftPaths,
    is_running: bool,
) -> Result<MaintenanceOutcome> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Maintenance");
    let mut selected = 0;

    loop {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:   {}\r\n Path:     {}\r\n Choose maintenance operation:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("SERVER MAINTENANCE: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            server.path.display(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new(
                "1",
                "Auto-Heal / Fix Server (Repair Jars, Java Version, EULA)",
            )
            .with_aliases(&["f", "fix"]),
            MenuEntry::new("2", "Rename Server").with_aliases(&["r", "rename"]),
            MenuEntry::new("3", "Delete or Unregister Server (Safe Trash / Permanent)")
                .with_aliases(&["del", "rm", "delete"]),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = print_in_place_status(
                    "RUNNING AUTO-HEAL",
                    &[format!("Analyzing server '{}'...", server.name)],
                );
                match crate::commands::fix::handle_fix(&server.name, None, paths).await {
                    Ok(_) => {
                        show_modal_message(
                            "FIX COMPLETE",
                            &[format!(
                                "[OK] Server '{}' checked and repaired successfully.",
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
                            "FIX ERROR",
                            &[format!("[ERROR] Auto-heal encountered an error: {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(1) => {
                if is_running {
                    show_modal_message(
                        "RENAME BLOCKED",
                        &[
                            format!(
                                "Cannot rename server '{}': The server is currently RUNNING.",
                                server.name
                            ),
                            "Please STOP the server first before renaming it.".to_string(),
                        ],
                        true,
                    )?;
                    continue;
                }

                let prompt = format!("Enter new name for server '{}':", server.name);
                let new_name = match run_input_prompt("RENAME SERVER", &prompt, Some(&server.name))?
                {
                    Some(n) => n.trim().to_string(),
                    None => continue,
                };

                if new_name.is_empty() || new_name == server.name {
                    continue;
                }

                if !new_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    show_modal_message(
                        "INVALID NAME",
                        &[
                            "Server name may only contain alphanumeric characters, hyphens, and underscores.".to_string(),
                        ],
                        true,
                    )?;
                    continue;
                }

                let mut reg = ServersRegistry::load(paths)?;
                if reg
                    .servers
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(&new_name))
                {
                    show_modal_message(
                        "NAME TAKEN",
                        &[format!(
                            "A server named '{}' already exists in registry.",
                            new_name
                        )],
                        true,
                    )?;
                    continue;
                }

                let old_name = server.name.clone();
                let old_path = server.path.clone();

                let new_path = if old_path.starts_with(&paths.servers_dir)
                    && old_path.file_name().and_then(|f| f.to_str()) == Some(&old_name)
                {
                    let target_dir = paths.servers_dir.join(&new_name);
                    if target_dir.exists() {
                        show_modal_message(
                            "DIRECTORY EXISTS",
                            &[format!(
                                "Directory '{}' already exists on disk.",
                                target_dir.display()
                            )],
                            true,
                        )?;
                        continue;
                    }
                    if let Err(e) = std::fs::rename(&old_path, &target_dir) {
                        show_modal_message(
                            "RENAME FAILED",
                            &[format!("Failed to rename server directory: {}", e)],
                            true,
                        )?;
                        continue;
                    }
                    target_dir
                } else {
                    old_path
                };

                if let Some(s) = reg.servers.iter_mut().find(|s| s.name == old_name) {
                    s.name = new_name.clone();
                    s.path = new_path;
                    let _ = reg.save(paths);
                }

                if let Ok(mut bkp_reg) = GlobalBackupRegistry::load(paths) {
                    if let Some(policy) = bkp_reg.server_policies.remove(&old_name) {
                        bkp_reg.server_policies.insert(new_name.clone(), policy);
                        let _ = bkp_reg.save(paths);
                    }
                }

                let old_bkp_dir = paths.backups_dir.join(&old_name);
                let new_bkp_dir = paths.backups_dir.join(&new_name);
                if old_bkp_dir.exists() && !new_bkp_dir.exists() {
                    let _ = std::fs::rename(old_bkp_dir, new_bkp_dir);
                }

                return Ok(MaintenanceOutcome::Renamed(new_name));
            }
            Some(2) => {
                let width = get_content_width(80);
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose removal method for server '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("DELETE SERVER: {}", server.name), width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    server.name,
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Move to Trash Bin (Safe, Restorable)")
                        .with_aliases(&["t", "trash"]),
                    MenuEntry::new("2", "Unregister Only (Keep Files on Disk)")
                        .with_aliases(&["u"]),
                    MenuEntry::new("3", "Permanently Delete Files (Irreversible)")
                        .with_aliases(&["p"]),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut c_sel = 0;
                match run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    Some(0) => {
                        let mut reg = ServersRegistry::load(paths)?;
                        reg.remove(&server.path);
                        reg.save(paths)?;

                        let trash = TrashManager::new(paths);
                        if server.path.exists() {
                            let _ = trash.trash_file(&server.path, Some(&server.name));
                        }
                        show_modal_message(
                            "MOVED TO TRASH",
                            &[
                                format!(
                                    "[OK] Server '{}' was safely moved to the Trash Bin.",
                                    server.name
                                )
                                .green()
                                .bold()
                                .to_string(),
                                "You can restore it anytime from the main dashboard Trash Bin."
                                    .to_string(),
                            ],
                            false,
                        )?;
                        return Ok(MaintenanceOutcome::Deleted);
                    }
                    Some(1) => {
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
                        return Ok(MaintenanceOutcome::Deleted);
                    }
                    Some(2) => {
                        let second_header = format!(
                            "{}\r\n{}\r\n{}\r\n WARNING: This will permanently erase server '{}' and ALL world data!\r\n Directory: {}\r\n This action is IRREVERSIBLE and CANNOT be undone.\r\n{}\r\n Are you ABSOLUTELY sure you want to proceed?\r\n{}",
                            box_top(width).red().bold(),
                            box_title("FINAL CONFIRMATION: PERMANENT REMOVAL", width, false).red().bold(),
                            box_divider(width).red().bold(),
                            server.name.red().bold(),
                            server.path.display(),
                            box_divider(width).dimmed(),
                            box_divider(width).dimmed(),
                        );
                        let second_entries = vec![
                            MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                            MenuEntry::new(
                                "2",
                                format!("Confirm Permanent Delete '{}'", server.name),
                            ),
                        ];
                        let mut second_sel = 0;
                        if let Some(1) = run_menu(&second_header, &second_entries, &mut second_sel)?
                        {
                            let mut reg = ServersRegistry::load(paths)?;
                            reg.remove(&server.path);
                            reg.save(paths)?;
                            if server.path.exists() {
                                let _ = std::fs::remove_dir_all(&server.path);
                            }
                            show_modal_message(
                                "SERVER DELETED",
                                &[format!(
                                    "[OK] Server '{}' and directory permanently removed.",
                                    server.name
                                )],
                                false,
                            )?;
                            return Ok(MaintenanceOutcome::Deleted);
                        }
                    }
                    _ => {}
                }
            }
            _ => return Ok(MaintenanceOutcome::None),
        }
    }
}

pub(crate) async fn server_backups_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Backups");
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.find_by_name(server_name) {
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

        let backup_reg = GlobalBackupRegistry::load(paths)?;
        let method_display = backup_reg.format_method_display(server.backup_method.as_deref());

        let policy = backup_reg.server_policies.get(&server.name);
        let policy_str = if let Some(p) = policy {
            if p.enabled {
                format!(
                    "[AUTO: Every {}h | Keep {}]",
                    p.interval_hours, p.retention_count
                )
                .green()
                .to_string()
            } else {
                "[AUTO: Disabled]".dimmed().to_string()
            }
        } else {
            "[AUTO: Disabled]".dimmed().to_string()
        };

        let local_dir = if let Some(ref m) = server.backup_method {
            if let Some(id) = m.strip_prefix("local:") {
                backup_reg
                    .find_local(id)
                    .map(|t| t.path.clone())
                    .unwrap_or_else(|| backup_reg.default_local_path(paths))
            } else {
                backup_reg.default_local_path(paths)
            }
        } else {
            backup_reg.default_local_path(paths)
        };
        let engine = BackupEngine::with_dir(local_dir);
        let existing_backups = engine.list_backups(&server.name);
        let total_size_mb: f64 = existing_backups
            .iter()
            .map(|b| b.size_bytes as f64)
            .sum::<f64>()
            / (1024.0 * 1024.0);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:         {}\r\n Active Method:  {}\r\n Auto-Backup:    {}\r\n Local Archives: {} ({:.2} MB total)\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("BACKUPS: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            method_display,
            policy_str,
            existing_backups.len().to_string().cyan().bold(),
            total_size_mb,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "New Backup").with_aliases(&["c", "create", "n", "new"]),
            MenuEntry::new("2", "Browse & Manage Backups"),
            MenuEntry::new("3", "Restore Backup"),
            MenuEntry::new("4", "Auto-Backup Policy"),
            MenuEntry::new("5", "Backup Method"),
            MenuEntry::new("6", "Trash Bin").with_aliases(&["t"]),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Create backup now using selected method
                let scope_header = " Choose backup scope:";
                let scope_entries = vec![
                    MenuEntry::new("1", "Full Backup"),
                    MenuEntry::new("2", "World Only"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut sc_sel = 0;
                let world_only = match run_menu(scope_header, &scope_entries, &mut sc_sel)? {
                    Some(0) => false,
                    Some(1) => true,
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "CREATING BACKUP",
                    &[
                        format!("Creating snapshot for '{}'...", server.name),
                        "Compressing archive data...".to_string(),
                    ],
                );

                let archive_path = match engine
                    .create_backup(&server.name, &server.path, None, world_only)
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        show_modal_message("BACKUP FAILED", &[format!("[ERROR] {}", e)], true)?;
                        continue;
                    }
                };

                let fname = archive_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("backup.tar.gz");
                let mut summary_lines =
                    vec![
                        format!("[OK] Local archive saved: {}", archive_path.display())
                            .green()
                            .bold()
                            .to_string(),
                    ];

                let method = server.backup_method.as_deref().unwrap_or("local");

                // Upload to S3 if method matches
                let s3_targets_to_upload: Vec<craft_core::S3BackupConfig> =
                    if let Some(id) = method.strip_prefix("s3:") {
                        backup_reg
                            .find_s3(id)
                            .cloned()
                            .map(|t| t.into())
                            .into_iter()
                            .collect()
                    } else if method == "s3" {
                        if let Some(first) = backup_reg.s3_targets.first() {
                            vec![first.clone().into()]
                        } else if let Some(ref legacy) = backup_reg.s3 {
                            vec![legacy.clone()]
                        } else {
                            vec![]
                        }
                    } else if method == "multi" {
                        if !backup_reg.s3_targets.is_empty() {
                            backup_reg
                                .s3_targets
                                .iter()
                                .map(|t| t.clone().into())
                                .collect()
                        } else if let Some(ref legacy) = backup_reg.s3 {
                            vec![legacy.clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                if method.starts_with("s3") && s3_targets_to_upload.is_empty() {
                    summary_lines.push(
                        "[WARN] Selected S3 provider is not configured; kept locally."
                            .yellow()
                            .to_string(),
                    );
                }

                for s3_config in s3_targets_to_upload {
                    let _ = print_in_place_status(
                        "UPLOADING TO S3",
                        &[format!(
                            "Uploading '{}' to S3 bucket '{}'...",
                            fname, s3_config.bucket
                        )],
                    );
                    let provider = S3StorageProvider::new(s3_config.clone());
                    let remote_key = if let Some(ref pfx) = s3_config.prefix {
                        format!("{}/{}/{}", pfx.trim_end_matches('/'), server.name, fname)
                    } else {
                        format!("{}/{}", server.name, fname)
                    };
                    match provider.upload_file(&archive_path, &remote_key).await {
                        Ok(_) => summary_lines.push(
                            format!(
                                "[OK] S3 Upload complete: s3://{}/{}",
                                s3_config.bucket, remote_key
                            )
                            .green()
                            .to_string(),
                        ),
                        Err(e) => summary_lines.push(
                            format!("[WARN] S3 Upload failed ({}): {}", s3_config.bucket, e)
                                .yellow()
                                .to_string(),
                        ),
                    }
                }

                // Upload to Google Drive if method matches
                let gd_targets_to_upload: Vec<craft_core::GDriveBackupConfig> =
                    if let Some(id) = method.strip_prefix("gdrive:") {
                        backup_reg
                            .find_gdrive(id)
                            .cloned()
                            .map(|t| t.into())
                            .into_iter()
                            .collect()
                    } else if method == "gdrive" {
                        if let Some(first) = backup_reg.gdrive_targets.first() {
                            vec![first.clone().into()]
                        } else if let Some(ref legacy) = backup_reg.gdrive {
                            vec![legacy.clone()]
                        } else {
                            vec![]
                        }
                    } else if method == "multi" {
                        if !backup_reg.gdrive_targets.is_empty() {
                            backup_reg
                                .gdrive_targets
                                .iter()
                                .map(|t| t.clone().into())
                                .collect()
                        } else if let Some(ref legacy) = backup_reg.gdrive {
                            vec![legacy.clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                if method.starts_with("gdrive") && gd_targets_to_upload.is_empty() {
                    summary_lines.push(
                        "[WARN] Selected Google Drive provider is not configured; kept locally."
                            .yellow()
                            .to_string(),
                    );
                }

                for gd_config in gd_targets_to_upload {
                    let _ = print_in_place_status(
                        "UPLOADING TO GDRIVE",
                        &[format!(
                            "Uploading '{}' to Google Drive folder '{}'...",
                            fname, gd_config.folder_id
                        )],
                    );
                    let provider = GDriveStorageProvider::new(gd_config.clone());
                    match provider.upload_file(&archive_path, fname).await {
                        Ok(_) => summary_lines.push(
                            format!(
                                "[OK] Google Drive Upload complete: folder {}",
                                gd_config.folder_id
                            )
                            .green()
                            .to_string(),
                        ),
                        Err(e) => summary_lines.push(
                            format!("[WARN] Google Drive Upload failed: {}", e)
                                .yellow()
                                .to_string(),
                        ),
                    }
                }

                show_modal_message("SNAPSHOT CREATED", &summary_lines, false)?;
            }
            Some(1) => {
                // Browse & Manage backups
                let mut b_page = 0;
                let page_size = 7;
                loop {
                    let list = engine.list_backups(&server.name);
                    if list.is_empty() {
                        show_modal_message(
                            "NO BACKUPS FOUND",
                            &[format!(
                                "No local backups found for server '{}'.",
                                server.name
                            )],
                            false,
                        )?;
                        break;
                    }

                    let width = get_content_width(80);
                    let action = run_paged_list_menu(
                        &list,
                        &mut b_page,
                        page_size,
                        |page, total_pages, total_count| {
                            let page_info = if total_pages > 1 {
                                format!(" | Page {} of {}", page, total_pages)
                                    .cyan()
                                    .to_string()
                            } else {
                                "".to_string()
                            };
                            format!(
                                "{}\r\n{}\r\n{}\r\n Server: {}\r\n Total Backups: {}{}\r\n Select a backup archive to restore or move to trash.\r\n{}",
                                box_top(width).cyan().bold(),
                                box_title(&format!("BACKUPS: {}", server.name), width, false).cyan().bold(),
                                box_divider(width).cyan().bold(),
                                server.name.white().bold(),
                                total_count.to_string().cyan().bold(),
                                page_info,
                                box_divider(width).dimmed(),
                            )
                        },
                        |_local_idx, _global_idx, b| {
                            let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                            format!("{:<38} ({:.2} MB, {})", b.filename, mb, b.created_at)
                        },
                        &[MenuEntry::new("n", "New Backup").with_aliases(&["c", "create", "new"])],
                        false,
                    )?;

                    match action {
                        PagedMenuAction::Action(act) if act == "n" || act == "c" => {
                            let scope_header = " Choose backup scope:";
                            let scope_entries = vec![
                                MenuEntry::new("1", "Full Backup"),
                                MenuEntry::new("2", "World Only"),
                                MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                            ];
                            let mut scope_sel = 0;
                            if let Some(s_idx) =
                                run_menu(scope_header, &scope_entries, &mut scope_sel)?
                            {
                                let world_only = match s_idx {
                                    0 => false,
                                    1 => true,
                                    _ => continue,
                                };
                                let _ = print_in_place_status(
                                    "CREATING BACKUP",
                                    &[format!("Creating snapshot for '{}'...", server.name)],
                                );
                                match engine
                                    .create_backup(&server.name, &server.path, None, world_only)
                                    .await
                                {
                                    Ok(archive_path) => {
                                        show_modal_message(
                                            "BACKUP GENERATED",
                                            &[format!(
                                                "[OK] Generated: {}",
                                                archive_path.display()
                                            )
                                            .green()
                                            .bold()
                                            .to_string()],
                                            false,
                                        )?;
                                    }
                                    Err(e) => {
                                        show_modal_message(
                                            "BACKUP FAILED",
                                            &[format!("[ERROR] {}", e)],
                                            true,
                                        )?;
                                    }
                                }
                            }
                            continue;
                        }
                        PagedMenuAction::Select(global_idx) if global_idx < list.len() => {
                            let backup = &list[global_idx];
                            let mb = (backup.size_bytes as f64) / (1024.0 * 1024.0);
                            let action_header = format!(
                                " Backup: {}\r\n Size:   {:.2} MB | Created: {}\r\n Path:   {}\r\n Select action:",
                                backup.filename.white().bold(),
                                mb,
                                backup.created_at,
                                backup.path.display(),
                            );
                            let action_entries = vec![
                                MenuEntry::new("1", "Restore Backup"),
                                MenuEntry::new("2", "Move to Trash"),
                                MenuEntry::new("0", "Back").with_aliases(&["b"]),
                            ];
                            let mut act_sel = 0;
                            if let Some(act) =
                                run_menu(&action_header, &action_entries, &mut act_sel)?
                            {
                                match act {
                                    0 => {
                                        // Restore
                                        if craft_core::is_server_locked(&server.path)
                                            || craft_core::get_server_running_pid(&server.path)
                                                .is_some()
                                        {
                                            let pid_info =
                                                craft_core::get_server_running_pid(&server.path)
                                                    .map(|p| format!(" (PID: {})", p))
                                                    .unwrap_or_default();
                                            show_modal_message(
                                                "RESTORE BLOCKED: SERVER IS RUNNING",
                                                &[
                                                    format!("Cannot restore backup to server '{}': The server is currently RUNNING{}.", server.name, pid_info),
                                                    "You MUST stop the server before restoring a backup to prevent world corruption.".to_string(),
                                                    "".to_string(),
                                                    "Please stop the server first, then try restoring again.".to_string(),
                                                ],
                                                true,
                                            )?;
                                            continue;
                                        }

                                        let confirm_header = format!(
                                            "{}\r\n{}\r\n{}\r\n WARNING: Restoring will overwrite server files with archive '{}'!\r\n Server: {}\r\n Path:   {}\r\n{}\r\n Are you sure you want to proceed with restore?\r\n{}",
                                            box_top(width).yellow().bold(),
                                            box_title("CONFIRM BACKUP RESTORE", width, false).yellow().bold(),
                                            box_divider(width).yellow().bold(),
                                            backup.filename.white().bold(),
                                            server.name.white().bold(),
                                            server.path.display(),
                                            box_divider(width).dimmed(),
                                            box_divider(width).dimmed(),
                                        );
                                        let confirm_entries = vec![
                                            MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                                            MenuEntry::new(
                                                "2",
                                                format!("Confirm Restore of '{}'", backup.filename),
                                            ),
                                        ];
                                        let mut c_sel = 0;
                                        if let Some(1) =
                                            run_menu(&confirm_header, &confirm_entries, &mut c_sel)?
                                        {
                                            let _ = print_in_place_status(
                                                "RESTORING SERVER",
                                                &[format!(
                                                    "Unpacking backup '{}' into '{}'...",
                                                    backup.filename,
                                                    server.path.display()
                                                )],
                                            );
                                            match engine.restore_backup(&backup.path, &server.path)
                                            {
                                                Ok(_) => {
                                                    show_modal_message(
                                                        "RESTORE COMPLETE",
                                                        &[format!("[OK] Successfully restored server '{}' from backup!", server.name)
                                                            .green()
                                                            .bold()
                                                            .to_string()],
                                                        false,
                                                    )?;
                                                }
                                                Err(e) => {
                                                    show_modal_message(
                                                        "RESTORE FAILED",
                                                        &[format!("[ERROR] {}", e)],
                                                        true,
                                                    )?;
                                                }
                                            }
                                        }
                                    }
                                    1 => {
                                        // Move to Trash
                                        let confirm_header = format!(
                                            "{}\r\n{}\r\n{}\r\n Move backup '{}' to the Trash Bin?\r\n Server: {}\r\n Size:   {:.2} MB\r\n\r\n Backups in the trash are securely hashed with SHA-256 and can be restored or permanently deleted.\r\n{}",
                                            box_top(width).yellow().bold(),
                                            box_title("CONFIRM MOVE TO TRASH", width, false).yellow().bold(),
                                            box_divider(width).yellow().bold(),
                                            backup.filename.white().bold(),
                                            server.name.white().bold(),
                                            mb,
                                            box_divider(width).dimmed(),
                                        );
                                        let confirm_entries = vec![
                                            MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                                            MenuEntry::new(
                                                "2",
                                                format!("Move '{}' to Trash", backup.filename),
                                            ),
                                        ];
                                        let mut c_sel = 0;
                                        if let Some(1) =
                                            run_menu(&confirm_header, &confirm_entries, &mut c_sel)?
                                        {
                                            let _ = print_in_place_status(
                                                "MOVING TO TRASH",
                                                &[format!("Calculating SHA-256 hash and moving '{}' to trash...", backup.filename)],
                                            );
                                            let manager = TrashManager::new(paths);
                                            match manager
                                                .trash_file(&backup.path, Some(&server.name))
                                            {
                                                Ok(item) => {
                                                    show_modal_message(
                                                        "MOVED TO TRASH",
                                                        &[
                                                            "[OK] Backup successfully moved to Trash Bin.".green().bold().to_string(),
                                                            format!("Archive: {}", item.original_name),
                                                            format!("SHA-256: {}", item.content_hash).dimmed().to_string(),
                                                        ],
                                                        false,
                                                    )?;
                                                }
                                                Err(e) => {
                                                    show_modal_message(
                                                        "TRASH ERROR",
                                                        &[format!("[ERROR] {}", e)],
                                                        true,
                                                    )?;
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        PagedMenuAction::Back => break,
                        _ => {}
                    }
                }
            }
            Some(2) => {
                // Restore server from backup
                if craft_core::is_server_locked(&server.path)
                    || craft_core::get_server_running_pid(&server.path).is_some()
                {
                    let pid_info = craft_core::get_server_running_pid(&server.path)
                        .map(|p| format!(" (PID: {})", p))
                        .unwrap_or_default();
                    show_modal_message(
                        "RESTORE BLOCKED: SERVER IS RUNNING",
                        &[
                            format!("Cannot restore backup to server '{}': The server is currently RUNNING{}.", server.name, pid_info),
                            "You MUST stop the server before restoring a backup to prevent world corruption.".to_string(),
                            "".to_string(),
                            "Please stop the server first, then try restoring again.".to_string(),
                        ],
                        true,
                    )?;
                    continue;
                }

                let list = engine.list_backups(&server.name);
                let mut b_page = 0;
                let page_size = 7;
                let action_entries = vec![MenuEntry::new("c", "Custom Archive")];
                let width = get_content_width(80);

                let target_archive = loop {
                    let action = run_paged_list_menu(
                        &list,
                        &mut b_page,
                        page_size,
                        |page, total_pages, total_count| {
                            let page_info = if total_pages > 1 {
                                format!(" | Page {} of {}", page, total_pages)
                                    .cyan()
                                    .to_string()
                            } else {
                                "".to_string()
                            };
                            format!(
                                "{}\r\n{}\r\n{}\r\n Server: {}\r\n Select backup archive to restore (Total: {}){}:\r\n{}",
                                box_top(width).cyan().bold(),
                                box_title(&format!("RESTORE BACKUP: {}", server.name), width, false).cyan().bold(),
                                box_divider(width).cyan().bold(),
                                server.name.white().bold(),
                                total_count,
                                page_info,
                                box_divider(width).dimmed(),
                            )
                        },
                        |_local_idx, _global_idx, b| {
                            let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                            format!("{:<38} ({:.2} MB, {})", b.filename, mb, b.created_at)
                        },
                        &action_entries,
                        false,
                    )?;

                    match action {
                        PagedMenuAction::Select(global_idx) if global_idx < list.len() => {
                            break Some(list[global_idx].path.clone());
                        }
                        PagedMenuAction::Action(act) if act == "c" => {
                            match run_input_prompt(
                                "CUSTOM ARCHIVE",
                                "Enter path to archive (.tar.gz / .zip):",
                                None,
                            )? {
                                Some(p) if !p.trim().is_empty() => {
                                    break Some(std::path::PathBuf::from(p.trim()))
                                }
                                _ => continue,
                            }
                        }
                        PagedMenuAction::Back => break None,
                        _ => {}
                    }
                };

                let target_archive = match target_archive {
                    Some(p) => p,
                    None => continue,
                };

                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n WARNING: Restoring will overwrite server files with archive '{}'!\r\n Server: {}\r\n Path:   {}\r\n{}\r\n Are you sure you want to proceed with restore?\r\n{}",
                    box_top(width).yellow().bold(),
                    box_title("CONFIRM BACKUP RESTORE", width, false).yellow().bold(),
                    box_divider(width).yellow().bold(),
                    target_archive.file_name().and_then(|f| f.to_str()).unwrap_or("backup"),
                    server.name.white().bold(),
                    server.path.display(),
                    box_divider(width).dimmed(),
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                    MenuEntry::new("2", "Confirm Restore"),
                ];
                let mut c_sel = 0;
                if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    let _ = print_in_place_status(
                        "RESTORING SERVER",
                        &[format!(
                            "Unpacking backup '{}' into '{}'...",
                            target_archive.display(),
                            server.path.display()
                        )],
                    );
                    match engine.restore_backup(&target_archive, &server.path) {
                        Ok(_) => {
                            show_modal_message(
                                "RESTORE COMPLETE",
                                &[format!(
                                    "[OK] Successfully restored server '{}' from backup!",
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
                                "RESTORE FAILED",
                                &[format!("[ERROR] {}", e)],
                                true,
                            )?;
                        }
                    }
                }
            }
            Some(3) => {
                // Configure Auto-Backup Schedule
                super::cloud_backups::configure_single_policy(paths, &server.name).await?;
            }
            Some(4) => {
                // Select active backup method
                if let Some(new_method) =
                    super::cloud_backups::pick_server_backup_method(paths, &server.name).await?
                {
                    let mut reg = ServersRegistry::load(paths)?;
                    if let Some(s) = reg.servers.iter_mut().find(|s| s.name == server.name) {
                        s.backup_method = Some(new_method.clone());
                        reg.save(paths)?;
                        let display = GlobalBackupRegistry::load(paths)?
                            .format_method_display(Some(&new_method));
                        show_modal_message(
                            "BACKUP METHOD UPDATED",
                            &[format!(
                                "[OK] Server '{}' backup system updated: {}",
                                server.name, display
                            )
                            .green()
                            .bold()
                            .to_string()],
                            false,
                        )?;
                    }
                }
            }
            Some(5) => {
                // Trash Bin
                super::trash_tui::trash_bin_menu(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn server_plugins_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Plugins");
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.find_by_name(server_name) {
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

        let caps = craft_providers::get_content_capabilities(&server.software);
        if !caps.plugins {
            show_modal_message(
                "PLUGINS NOT SUPPORTED",
                &[
                    format!(
                        "Server '{}' (software: {}) does not support plugins.",
                        server.name, server.software
                    ),
                    "".to_string(),
                    if caps.mods {
                        "This server is a modded server; use Mods instead."
                            .yellow()
                            .to_string()
                    } else {
                        "Plugins only exist in non-vanilla server softwares (e.g. Paper, Purpur, Spigot).".yellow().to_string()
                    },
                ],
                true,
            )?;
            return Ok(());
        }

        let plugins_dir = server.path.join("plugins");
        let installed_plugins = list_server_plugins(&plugins_dir);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:    {} ({:<10} {})\r\n Directory: {}\r\n Installed: {} plugin jar(s)\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("PLUGINS: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            server.software.cyan(),
            server.version,
            plugins_dir.display(),
            installed_plugins.len().to_string().cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Search Online"),
            MenuEntry::new("2", "Install by Slug / ID"),
            MenuEntry::new("3", "Manage Installed"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Search online
                let query = match run_input_prompt(
                    "SEARCH PLUGINS",
                    "Enter search keyword (e.g. essentials, viaversion, luckperms, worldedit):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "SEARCHING PLUGINS",
                    &[format!(
                        "Searching Modrinth, Hangar, and Poggit for '{}'...",
                        query
                    )],
                );
                let pm = craft_plugins::PluginManager::new();
                let results = pm.search(&query).await;

                if results.is_empty() {
                    show_modal_message(
                        "NO PLUGINS FOUND",
                        &[format!("No plugins found matching query '{}'.", query)],
                        false,
                    )?;
                } else {
                    let mut p_entries = Vec::new();
                    for (i, hit) in results.iter().enumerate() {
                        let hotkey = if i < 9 {
                            (i + 1).to_string()
                        } else if i < 35 {
                            ((b'a' + (i - 9) as u8) as char).to_string()
                        } else {
                            format!("{}", i + 1)
                        };
                        let desc = craft_core::truncate_ellipsis(&hit.description, 40);
                        p_entries.push(MenuEntry::new(
                            hotkey,
                            format!("{:<18} [{}] - {}", hit.name, hit.source, desc),
                        ));
                    }
                    p_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                    let p_header = format!(
                        " Search results for '{}' - select to install directly into '{}':",
                        query, server.name
                    );
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];
                            let _ = print_in_place_status(
                                "DOWNLOADING PLUGIN",
                                &[format!(
                                    "Downloading '{}' into '{}'...",
                                    chosen.name,
                                    plugins_dir.display()
                                )],
                            );
                            match pm
                                .install_from_modrinth(&server.path, &chosen.id_or_slug)
                                .await
                            {
                                Ok(dest) => {
                                    show_modal_message(
                                        "PLUGIN INSTALLED",
                                        &[
                                            format!(
                                                "[OK] Successfully installed '{}'!",
                                                chosen.name
                                            )
                                            .green()
                                            .bold()
                                            .to_string(),
                                            format!("File: {}", dest.display()),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "INSTALLATION FAILED",
                                        &[format!("[ERROR] {}", e)],
                                        true,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                // Install by slug
                let slug = match run_input_prompt(
                    "PLUGIN SLUG / ID",
                    "Enter Modrinth plugin slug or ID (e.g. luckperms, spark, floodgate):",
                    None,
                )? {
                    Some(s) if !s.trim().is_empty() => s.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "DOWNLOADING PLUGIN",
                    &[format!(
                        "Downloading '{}' into '{}'...",
                        slug,
                        plugins_dir.display()
                    )],
                );
                let pm = craft_plugins::PluginManager::new();
                match pm.install_from_modrinth(&server.path, &slug).await {
                    Ok(dest) => {
                        show_modal_message(
                            "PLUGIN INSTALLED",
                            &[
                                format!("[OK] Successfully installed plugin '{}'!", slug)
                                    .green()
                                    .bold()
                                    .to_string(),
                                format!("File: {}", dest.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "INSTALLATION FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(2) => {
                // Manage installed plugins
                manage_installed_plugins_menu(&server.name, &plugins_dir).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) struct InstalledPluginItem {
    pub filename: String,
    pub path: std::path::PathBuf,
    pub is_enabled: bool,
    pub size_bytes: u64,
}

pub(crate) fn list_server_plugins(plugins_dir: &std::path::Path) -> Vec<InstalledPluginItem> {
    let mut list = Vec::new();
    if plugins_dir.exists() && plugins_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.ends_with(".jar") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledPluginItem {
                            filename: fname,
                            path,
                            is_enabled: true,
                            size_bytes: size,
                        });
                    } else if fname.ends_with(".jar.disabled") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledPluginItem {
                            filename: fname,
                            path,
                            is_enabled: false,
                            size_bytes: size,
                        });
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| a.filename.cmp(&b.filename));
    list
}

async fn manage_installed_plugins_menu(
    server_name: &str,
    plugins_dir: &std::path::Path,
) -> Result<()> {
    let mut selected = 0;

    loop {
        let plugins = list_server_plugins(plugins_dir);
        if plugins.is_empty() {
            show_modal_message(
                "NO PLUGINS INSTALLED",
                &[format!(
                    "No plugin jars found in '{}'.",
                    plugins_dir.display()
                )],
                false,
            )?;
            return Ok(());
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server: '{}'\r\n Select a plugin jar to toggle status or delete:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("INSTALLED PLUGINS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, p) in plugins.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else if i < 35 {
                ((b'a' + (i - 9) as u8) as char).to_string()
            } else {
                format!("{}", i + 1)
            };
            let status = if p.is_enabled {
                "[ENABLED]".green().bold().to_string()
            } else {
                "[DISABLED]".dimmed().to_string()
            };
            let kb = (p.size_bytes as f64) / 1024.0;
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<35} {:>8.1} KB  {}", p.filename, kb, status),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match super::screen::run_menu_with_space(&header, &entries, &mut selected)? {
            super::screen::MenuAction::Space(idx) if idx < plugins.len() => {
                let chosen = &plugins[idx];
                if chosen.is_enabled {
                    let new_path = chosen.path.with_extension("jar.disabled");
                    let _ = std::fs::rename(&chosen.path, new_path);
                } else {
                    let stem = chosen.path.to_string_lossy();
                    if let Some(orig) = stem.strip_suffix(".disabled") {
                        let _ = std::fs::rename(&chosen.path, orig);
                    }
                }
            }
            super::screen::MenuAction::Select(idx) if idx < plugins.len() => {
                let chosen = &plugins[idx];
                let item_header = format!(" Plugin: {}\r\n Choose action:", chosen.filename);
                let toggle_label = if chosen.is_enabled {
                    "Disable Plugin"
                } else {
                    "Enable Plugin"
                };
                let item_entries = vec![
                    MenuEntry::new("1", toggle_label),
                    MenuEntry::new("2", "Delete Plugin"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut item_sel = 0;
                match run_menu(&item_header, &item_entries, &mut item_sel)? {
                    Some(0) => {
                        if chosen.is_enabled {
                            let new_path = chosen.path.with_extension("jar.disabled");
                            let _ = std::fs::rename(&chosen.path, new_path);
                        } else {
                            let stem = chosen.path.to_string_lossy();
                            if let Some(orig) = stem.strip_suffix(".disabled") {
                                let _ = std::fs::rename(&chosen.path, orig);
                            }
                        }
                    }
                    Some(1) => {
                        let _ = std::fs::remove_file(&chosen.path);
                        show_modal_message(
                            "PLUGIN DELETED",
                            &[format!(
                                "[OK] Removed '{}' from plugins directory.",
                                chosen.filename
                            )],
                            false,
                        )?;
                    }
                    _ => continue,
                }
            }
            _ => return Ok(()),
        }
    }
}

// ==========================================
// MODS MANAGEMENT
// ==========================================

pub(crate) async fn server_mods_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Mods");
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.find_by_name(server_name) {
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

        let caps = craft_providers::get_content_capabilities(&server.software);
        if !caps.mods {
            show_modal_message(
                "MODS NOT SUPPORTED",
                &[
                    format!(
                        "Server '{}' (software: {}) does not support mods.",
                        server.name, server.software
                    ),
                    "".to_string(),
                    "Mods only exist in modded server softwares (e.g. Fabric, Quilt, NeoForge)."
                        .yellow()
                        .to_string(),
                ],
                true,
            )?;
            return Ok(());
        }

        let mods_dir = server.path.join("mods");
        let _ = std::fs::create_dir_all(&mods_dir);
        let installed_mods = list_server_mods(&mods_dir);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:    {} ({:<10} {})\r\n Directory: {}\r\n Installed: {} mod jar(s)\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("MODS: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            server.software.cyan(),
            server.version,
            mods_dir.display(),
            installed_mods.len().to_string().cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Search Online"),
            MenuEntry::new("2", "Install by Slug / ID"),
            MenuEntry::new("3", "Manage Installed"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Search online
                let query = match run_input_prompt(
                    "SEARCH MODS",
                    "Enter mod keyword (e.g. fabric-api, sodium, lithium, appleskin, jei):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "SEARCHING MODS",
                    &[format!("Searching Modrinth for '{}'...", query)],
                );
                let pm = craft_plugins::PluginManager::new();
                let results = pm.search_mods(&query).await;

                if results.is_empty() {
                    show_modal_message(
                        "NO MODS FOUND",
                        &[format!("No mods found matching query '{}'.", query)],
                        false,
                    )?;
                } else {
                    let mut p_entries = Vec::new();
                    for (i, hit) in results.iter().enumerate() {
                        let hotkey = if i < 9 {
                            (i + 1).to_string()
                        } else if i < 35 {
                            ((b'a' + (i - 9) as u8) as char).to_string()
                        } else {
                            format!("{}", i + 1)
                        };
                        let desc = craft_core::truncate_ellipsis(&hit.description, 40);
                        p_entries.push(MenuEntry::new(
                            hotkey,
                            format!("{:<18} [{}] - {}", hit.name, hit.source, desc),
                        ));
                    }
                    p_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                    let p_header = format!(
                        " Search results for '{}' - select to install directly into '{}':",
                        query, server.name
                    );
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];
                            let _ = print_in_place_status(
                                "DOWNLOADING MOD",
                                &[format!(
                                    "Downloading '{}' into '{}'...",
                                    chosen.name,
                                    mods_dir.display()
                                )],
                            );
                            match pm
                                .install_mod_from_modrinth(&server.path, &chosen.id_or_slug)
                                .await
                            {
                                Ok(dest) => {
                                    show_modal_message(
                                        "MOD INSTALLED",
                                        &[
                                            format!(
                                                "[OK] Successfully installed '{}'!",
                                                chosen.name
                                            )
                                            .green()
                                            .bold()
                                            .to_string(),
                                            format!("File: {}", dest.display()),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "INSTALLATION FAILED",
                                        &[format!("[ERROR] {}", e)],
                                        true,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                // Install by slug
                let slug = match run_input_prompt(
                    "MOD SLUG / ID",
                    "Enter Modrinth mod slug or ID (e.g. fabric-api, sodium, lithium):",
                    None,
                )? {
                    Some(s) if !s.trim().is_empty() => s.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "DOWNLOADING MOD",
                    &[format!(
                        "Downloading '{}' into '{}'...",
                        slug,
                        mods_dir.display()
                    )],
                );
                let pm = craft_plugins::PluginManager::new();
                match pm.install_mod_from_modrinth(&server.path, &slug).await {
                    Ok(dest) => {
                        show_modal_message(
                            "MOD INSTALLED",
                            &[
                                format!("[OK] Successfully installed mod '{}'!", slug)
                                    .green()
                                    .bold()
                                    .to_string(),
                                format!("File: {}", dest.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "INSTALLATION FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(2) => {
                manage_installed_mods_menu(&server.name, &mods_dir).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) fn list_server_mods(mods_dir: &std::path::Path) -> Vec<InstalledPluginItem> {
    let mut list = Vec::new();
    if mods_dir.exists() && mods_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.ends_with(".jar") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledPluginItem {
                            filename: fname,
                            path,
                            is_enabled: true,
                            size_bytes: size,
                        });
                    } else if fname.ends_with(".jar.disabled") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledPluginItem {
                            filename: fname,
                            path,
                            is_enabled: false,
                            size_bytes: size,
                        });
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| a.filename.cmp(&b.filename));
    list
}

async fn manage_installed_mods_menu(server_name: &str, mods_dir: &std::path::Path) -> Result<()> {
    let mut selected = 0;

    loop {
        let mods = list_server_mods(mods_dir);
        if mods.is_empty() {
            show_modal_message(
                "NO MODS INSTALLED",
                &[format!("No mod jars found in '{}'.", mods_dir.display())],
                false,
            )?;
            return Ok(());
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server: '{}'\r\n Select a mod jar to toggle status or delete:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("INSTALLED MODS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, p) in mods.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else if i < 35 {
                ((b'a' + (i - 9) as u8) as char).to_string()
            } else {
                format!("{}", i + 1)
            };
            let status = if p.is_enabled {
                "[ENABLED]".green().bold().to_string()
            } else {
                "[DISABLED]".dimmed().to_string()
            };
            let kb = (p.size_bytes as f64) / 1024.0;
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<35} {:>8.1} KB  {}", p.filename, kb, status),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match super::screen::run_menu_with_space(&header, &entries, &mut selected)? {
            super::screen::MenuAction::Space(idx) if idx < mods.len() => {
                let chosen = &mods[idx];
                if chosen.is_enabled {
                    let new_path = chosen.path.with_extension("jar.disabled");
                    let _ = std::fs::rename(&chosen.path, new_path);
                } else {
                    let stem = chosen.path.to_string_lossy();
                    if let Some(orig) = stem.strip_suffix(".disabled") {
                        let _ = std::fs::rename(&chosen.path, orig);
                    }
                }
            }
            super::screen::MenuAction::Select(idx) if idx < mods.len() => {
                let chosen = &mods[idx];
                let item_header = format!(" Mod: {}\r\n Choose action:", chosen.filename);
                let toggle_label = if chosen.is_enabled {
                    "Disable Mod"
                } else {
                    "Enable Mod"
                };
                let item_entries = vec![
                    MenuEntry::new("1", toggle_label),
                    MenuEntry::new("2", "Delete Mod"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut item_sel = 0;
                match run_menu(&item_header, &item_entries, &mut item_sel)? {
                    Some(0) => {
                        if chosen.is_enabled {
                            let new_path = chosen.path.with_extension("jar.disabled");
                            let _ = std::fs::rename(&chosen.path, new_path);
                        } else {
                            let stem = chosen.path.to_string_lossy();
                            if let Some(orig) = stem.strip_suffix(".disabled") {
                                let _ = std::fs::rename(&chosen.path, orig);
                            }
                        }
                    }
                    Some(1) => {
                        let _ = std::fs::remove_file(&chosen.path);
                        show_modal_message(
                            "MOD DELETED",
                            &[format!(
                                "[OK] Removed '{}' from mods directory.",
                                chosen.filename
                            )],
                            false,
                        )?;
                    }
                    _ => continue,
                }
            }
            _ => return Ok(()),
        }
    }
}

// ==========================================
// DATAPACKS MANAGEMENT
// ==========================================

pub(crate) struct InstalledDatapackItem {
    pub filename: String,
    pub path: std::path::PathBuf,
    pub is_enabled: bool,
    pub size_bytes: u64,
}

pub(crate) fn list_server_datapacks(datapacks_dir: &std::path::Path) -> Vec<InstalledDatapackItem> {
    let mut list = Vec::new();
    if datapacks_dir.exists() && datapacks_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(datapacks_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                if path.is_file() {
                    if fname.ends_with(".zip") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledDatapackItem {
                            filename: fname,
                            path,
                            is_enabled: true,
                            size_bytes: size,
                        });
                    } else if fname.ends_with(".zip.disabled") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(InstalledDatapackItem {
                            filename: fname,
                            path,
                            is_enabled: false,
                            size_bytes: size,
                        });
                    }
                } else if path.is_dir() {
                    let is_disabled = fname.ends_with(".disabled");
                    let size = craft_plugins::world::dir_size(&path).unwrap_or(0);
                    list.push(InstalledDatapackItem {
                        filename: fname,
                        path,
                        is_enabled: !is_disabled,
                        size_bytes: size,
                    });
                }
            }
        }
    }
    list.sort_by(|a, b| a.filename.cmp(&b.filename));
    list
}

pub(crate) async fn server_datapacks_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Datapacks");
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.find_by_name(server_name) {
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

        let caps = craft_providers::get_content_capabilities(&server.software);
        if !caps.datapacks {
            show_modal_message(
                "DATAPACKS NOT SUPPORTED",
                &[
                    format!(
                        "Server '{}' (software: {}) does not support datapacks.",
                        server.name, server.software
                    ),
                    "".to_string(),
                    "Datapacks are only supported on Minecraft Java world servers."
                        .yellow()
                        .to_string(),
                ],
                true,
            )?;
            return Ok(());
        }

        let default_world = craft_core::get_default_world(&server.path);
        let datapacks_dir = server.path.join(&default_world).join("datapacks");
        let _ = std::fs::create_dir_all(&datapacks_dir);
        let installed_datapacks = list_server_datapacks(&datapacks_dir);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:       {} ({:<10} {})\r\n Active World: {}\r\n Directory:    {}\r\n Installed:    {} datapack(s)\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("DATAPACKS: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            server.software.cyan(),
            server.version,
            default_world.cyan().bold(),
            datapacks_dir.display(),
            installed_datapacks.len().to_string().cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Search Online"),
            MenuEntry::new("2", "Install by Slug / ID"),
            MenuEntry::new("3", "Manage Installed"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Search online
                let query = match run_input_prompt(
                    "SEARCH DATAPACKS",
                    "Enter datapack keyword (e.g. terralith, incendium, nullscape, timber):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "SEARCHING DATAPACKS",
                    &[format!("Searching Modrinth for '{}'...", query)],
                );
                let pm = craft_plugins::PluginManager::new();
                let results = pm.search_datapacks(&query).await;

                if results.is_empty() {
                    show_modal_message(
                        "NO DATAPACKS FOUND",
                        &[format!("No datapacks found matching query '{}'.", query)],
                        false,
                    )?;
                } else {
                    let mut p_entries = Vec::new();
                    for (i, hit) in results.iter().enumerate() {
                        let hotkey = if i < 9 {
                            (i + 1).to_string()
                        } else if i < 35 {
                            ((b'a' + (i - 9) as u8) as char).to_string()
                        } else {
                            format!("{}", i + 1)
                        };
                        let desc = craft_core::truncate_ellipsis(&hit.description, 40);
                        p_entries.push(MenuEntry::new(
                            hotkey,
                            format!("{:<18} [{}] - {}", hit.name, hit.source, desc),
                        ));
                    }
                    p_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                    let p_header = format!(" Search results for '{}' - select to install directly into '{}/datapacks':", query, default_world);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];
                            let _ = print_in_place_status(
                                "DOWNLOADING DATAPACK",
                                &[format!(
                                    "Downloading '{}' into '{}'...",
                                    chosen.name,
                                    datapacks_dir.display()
                                )],
                            );
                            match pm
                                .install_datapack_from_modrinth(
                                    &server.path,
                                    &chosen.id_or_slug,
                                    &default_world,
                                )
                                .await
                            {
                                Ok(dest) => {
                                    show_modal_message(
                                        "DATAPACK INSTALLED",
                                        &[
                                            format!(
                                                "[OK] Successfully installed '{}'!",
                                                chosen.name
                                            )
                                            .green()
                                            .bold()
                                            .to_string(),
                                            format!("File: {}", dest.display()),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "INSTALLATION FAILED",
                                        &[format!("[ERROR] {}", e)],
                                        true,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                // Install by slug
                let slug = match run_input_prompt(
                    "DATAPACK SLUG / ID",
                    "Enter Modrinth datapack slug or ID (e.g. terralith, incendium, nullscape):",
                    None,
                )? {
                    Some(s) if !s.trim().is_empty() => s.trim().to_string(),
                    _ => continue,
                };

                let _ = print_in_place_status(
                    "DOWNLOADING DATAPACK",
                    &[format!(
                        "Downloading '{}' into '{}'...",
                        slug,
                        datapacks_dir.display()
                    )],
                );
                let pm = craft_plugins::PluginManager::new();
                match pm
                    .install_datapack_from_modrinth(&server.path, &slug, &default_world)
                    .await
                {
                    Ok(dest) => {
                        show_modal_message(
                            "DATAPACK INSTALLED",
                            &[
                                format!("[OK] Successfully installed datapack '{}'!", slug)
                                    .green()
                                    .bold()
                                    .to_string(),
                                format!("File: {}", dest.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "INSTALLATION FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(2) => {
                manage_installed_datapacks_menu(&server.name, &datapacks_dir).await?;
            }
            _ => return Ok(()),
        }
    }
}

async fn manage_installed_datapacks_menu(
    server_name: &str,
    datapacks_dir: &std::path::Path,
) -> Result<()> {
    let mut selected = 0;

    loop {
        let datapacks = list_server_datapacks(datapacks_dir);
        if datapacks.is_empty() {
            show_modal_message(
                "NO DATAPACKS INSTALLED",
                &[format!(
                    "No datapacks found in '{}'.",
                    datapacks_dir.display()
                )],
                false,
            )?;
            return Ok(());
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server: '{}'\r\n Select a datapack to toggle status or delete:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("INSTALLED DATAPACKS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, p) in datapacks.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else if i < 35 {
                ((b'a' + (i - 9) as u8) as char).to_string()
            } else {
                format!("{}", i + 1)
            };
            let status = if p.is_enabled {
                "[ENABLED]".green().bold().to_string()
            } else {
                "[DISABLED]".dimmed().to_string()
            };
            let kb = (p.size_bytes as f64) / 1024.0;
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<35} {:>8.1} KB  {}", p.filename, kb, status),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match super::screen::run_menu_with_space(&header, &entries, &mut selected)? {
            super::screen::MenuAction::Space(idx) if idx < datapacks.len() => {
                let chosen = &datapacks[idx];
                if chosen.is_enabled {
                    let new_name = format!(
                        "{}.disabled",
                        chosen.path.file_name().unwrap().to_string_lossy()
                    );
                    let new_path = chosen.path.with_file_name(new_name);
                    let _ = std::fs::rename(&chosen.path, new_path);
                } else {
                    let stem = chosen.path.file_name().unwrap().to_string_lossy();
                    if let Some(orig) = stem.strip_suffix(".disabled") {
                        let new_path = chosen.path.with_file_name(orig);
                        let _ = std::fs::rename(&chosen.path, new_path);
                    }
                }
            }
            super::screen::MenuAction::Select(idx) if idx < datapacks.len() => {
                let chosen = &datapacks[idx];
                let item_header = format!(" Datapack: {}\r\n Choose action:", chosen.filename);
                let toggle_label = if chosen.is_enabled {
                    "Disable Datapack"
                } else {
                    "Enable Datapack"
                };
                let item_entries = vec![
                    MenuEntry::new("1", toggle_label),
                    MenuEntry::new("2", "Delete Datapack"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut item_sel = 0;
                match run_menu(&item_header, &item_entries, &mut item_sel)? {
                    Some(0) => {
                        if chosen.is_enabled {
                            let new_name = format!(
                                "{}.disabled",
                                chosen.path.file_name().unwrap().to_string_lossy()
                            );
                            let new_path = chosen.path.with_file_name(new_name);
                            let _ = std::fs::rename(&chosen.path, new_path);
                        } else {
                            let stem = chosen.path.file_name().unwrap().to_string_lossy();
                            if let Some(orig) = stem.strip_suffix(".disabled") {
                                let new_path = chosen.path.with_file_name(orig);
                                let _ = std::fs::rename(&chosen.path, new_path);
                            }
                        }
                    }
                    Some(1) => {
                        if chosen.path.is_dir() {
                            let _ = std::fs::remove_dir_all(&chosen.path);
                        } else {
                            let _ = std::fs::remove_file(&chosen.path);
                        }
                        show_modal_message(
                            "DATAPACK DELETED",
                            &[format!(
                                "[OK] Removed '{}' from datapacks directory.",
                                chosen.filename
                            )],
                            false,
                        )?;
                    }
                    _ => continue,
                }
            }
            _ => return Ok(()),
        }
    }
}

// ==========================================
// WORLDS / MAPS MANAGEMENT (DELEGATED TO WORLDS_TUI)
// ==========================================

#[allow(dead_code)]
pub(crate) async fn server_worlds_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = match registry.find_by_name(server_name) {
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
    super::worlds_tui::manage_installed_worlds_menu(&server).await
}
