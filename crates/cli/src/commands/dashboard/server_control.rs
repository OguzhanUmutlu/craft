use colored::Colorize;

use craft_backup::{BackupEngine, GDriveStorageProvider, S3StorageProvider, StorageProvider};
use craft_core::{CraftError, CraftPaths, GlobalBackupRegistry, Result, ServersRegistry};
use craft_daemon::DaemonClient;

use crate::commands::view::handle_view;
use super::screen::{
    box_divider, box_title, box_top, exec_console_action, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, show_modal_message, AltScreenGuard, MenuEntry,
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
        MenuEntry::new("1", "Create Your First Server").with_aliases(&["c", "n"]),
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
            entries.push(MenuEntry::new(
                hotkey,
                format!(
                    "{:<20} {:<10} {:<10} {}",
                    s.name, s.software, s.version, status_badge
                ),
            ));
        }
        entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

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

            let entries = vec![MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"])];
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
        if running_servers.len() > 1 {
            entries.push(MenuEntry::new("a", "Stop ALL Running Servers"));
        }
        entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

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
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version),
            ));
        }
        if registry.servers.len() > 1 {
            entries.push(MenuEntry::new("a", "Restart ALL Registered Servers"));
        }
        entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b"]));

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
                        MenuEntry::new("1", "Cancel (Keep server and data safe)").with_aliases(&["0", "b"]),
                        MenuEntry::new("2", format!("Confirm Permanent Deletion of '{}'", server.name)),
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
                "{}\r\n{}\r\n{}\r\n No servers currently registered on this local host.\r\n{}",
                box_top(width).cyan().bold(),
                box_title("LOCAL SERVERS (HOST)", width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
                box_divider(width).dimmed(),
            );

            // Defensive: Only option 1 and 0, aliases "c" / "n", strictly NO "2"
            let entries = vec![
                MenuEntry::new("1", "Create Your First Server").with_aliases(&["c", "n"]),
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
            box_title("LOCAL SERVERS (HOST)", width, false).cyan().bold(),
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

        entries.push(MenuEntry::new("n", "Create New Server").with_aliases(&["c"]));
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

        enum ControlAction {
            ToggleStartStop,
            Restart,
            AttachConsole,
            Backups,
            Plugins,
            DeleteServer,
        }

        let mut entries = Vec::new();
        let mut actions = Vec::new();

        if is_running {
            entries.push(MenuEntry::new("1", "Stop Server"));
            actions.push(ControlAction::ToggleStartStop);

            entries.push(MenuEntry::new("2", "Restart Server"));
            actions.push(ControlAction::Restart);

            entries.push(MenuEntry::new("3", "Attach Live Console"));
            actions.push(ControlAction::AttachConsole);
        } else {
            entries.push(MenuEntry::new("1", "Start Server"));
            actions.push(ControlAction::ToggleStartStop);
        }

        let bkp_hotkey = (actions.len() + 1).to_string();
        entries.push(MenuEntry::new(bkp_hotkey, "World Snapshots & Backups"));
        actions.push(ControlAction::Backups);

        let plg_hotkey = (actions.len() + 1).to_string();
        entries.push(MenuEntry::new(plg_hotkey, "Browse & Manage Plugins"));
        actions.push(ControlAction::Plugins);

        let del_hotkey = (actions.len() + 1).to_string();
        entries.push(MenuEntry::new(del_hotkey, "Delete Server"));
        actions.push(ControlAction::DeleteServer);

        entries.push(MenuEntry::new("0", "Back to Server List").with_aliases(&["b"]));

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
                let res = super::screen::run_virtual_console(&server_name, &server_path, paths).await;
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
            ControlAction::Backups => {
                server_backups_panel(&server.name, paths).await?;
            }
            ControlAction::Plugins => {
                server_plugins_panel(&server.name, paths).await?;
            }
            ControlAction::DeleteServer => {
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
                    MenuEntry::new("1", "Unregister Server (Keep files on disk)"),
                    MenuEntry::new("2", "Permanently Delete Server & Files"),
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
                        // SECOND CONFIRMATION
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
                            MenuEntry::new("1", "Cancel (Keep server and data safe)").with_aliases(&["0", "b"]),
                            MenuEntry::new("2", format!("Confirm Permanent Deletion of '{}'", server.name)),
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
                                    "[OK] Server '{}' and directory permanently removed.",
                                    server.name
                                )],
                                false,
                            )?;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub(crate) async fn server_backups_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
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
        let current_method_id = server.backup_method.as_deref().unwrap_or("local");
        let method_display = match current_method_id {
            "s3" => {
                if let Some(ref s3) = backup_reg.s3 {
                    format!("AWS S3 / R2 / MinIO [Bucket: {}]", s3.bucket).green().bold().to_string()
                } else {
                    "AWS S3 [NOT CONFIGURED IN CLOUD BACKUPS]".yellow().to_string()
                }
            }
            "gdrive" => {
                if let Some(ref gd) = backup_reg.gdrive {
                    format!("Google Drive [Folder: {}]", gd.folder_id).green().bold().to_string()
                } else {
                    "Google Drive [NOT CONFIGURED IN CLOUD BACKUPS]".yellow().to_string()
                }
            }
            "multi" => "Multi-Destination (Local + All Cloud)".cyan().bold().to_string(),
            _ => "Local Disk Storage (~/.craft/backups)".white().bold().to_string(),
        };

        let policy = backup_reg.server_policies.get(&server.name);
        let policy_str = if let Some(p) = policy {
            if p.enabled {
                format!("[AUTO: Every {}h | Keep {}]", p.interval_hours, p.retention_count).green().to_string()
            } else {
                "[AUTO: Disabled]".dimmed().to_string()
            }
        } else {
            "[AUTO: Disabled]".dimmed().to_string()
        };

        let engine = BackupEngine::new(paths);
        let existing_backups = engine.list_backups(&server.name);
        let total_size_mb: f64 = existing_backups.iter().map(|b| b.size_bytes as f64).sum::<f64>() / (1024.0 * 1024.0);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:         {}\r\n Active Method:  {}\r\n Auto-Backup:    {}\r\n Local Archives: {} ({:.2} MB total)\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("BACKUPS & SNAPSHOTS: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            method_display,
            policy_str,
            existing_backups.len().to_string().cyan().bold(),
            total_size_mb,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Select Active Backup Method"),
            MenuEntry::new("2", "Create Backup Now"),
            MenuEntry::new("3", "List Existing Backups"),
            MenuEntry::new("4", "Restore Server from Backup"),
            MenuEntry::new("5", "Configure Auto-Backup Schedule"),
            MenuEntry::new("0", "Back to Server Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Select active backup method
                let mut method_sel = 0;
                let s3_label = if let Some(ref s3) = backup_reg.s3 {
                    format!("AWS S3 / R2 / MinIO [Configured: {}]", s3.bucket)
                } else {
                    "AWS S3 / R2 / MinIO [Configure First]".to_string()
                };
                let gd_label = if let Some(ref gd) = backup_reg.gdrive {
                    format!("Google Drive [Configured: {}]", gd.folder_id)
                } else {
                    "Google Drive [Configure First]".to_string()
                };

                let method_entries = vec![
                    MenuEntry::new("1", "Local Disk Storage (~/.craft/backups)"),
                    MenuEntry::new("2", s3_label),
                    MenuEntry::new("3", gd_label),
                    MenuEntry::new("4", "Multi-Destination (Local + Cloud)"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];

                let method_header = format!(" Select backup system for server '{}':", server.name);
                if let Some(m_idx) = run_menu(&method_header, &method_entries, &mut method_sel)? {
                    let new_method = match m_idx {
                        0 => Some("local".to_string()),
                        1 => {
                            if backup_reg.s3.is_none() {
                                show_modal_message(
                                    "S3 NOT CONFIGURED",
                                    &[
                                        "No S3 / R2 / MinIO credentials are configured yet.",
                                        "Opening S3 setup wizard now...",
                                    ],
                                    false,
                                )?;
                                super::cloud_backups::configure_s3_menu(paths).await?;
                                let updated_reg = GlobalBackupRegistry::load(paths)?;
                                if updated_reg.s3.is_none() {
                                    continue;
                                }
                            }
                            Some("s3".to_string())
                        }
                        2 => {
                            if backup_reg.gdrive.is_none() {
                                show_modal_message(
                                    "GDRIVE NOT CONFIGURED",
                                    &[
                                        "No Google Drive credentials are configured yet.",
                                        "Opening Google Drive setup wizard now...",
                                    ],
                                    false,
                                )?;
                                super::cloud_backups::configure_gdrive_menu(paths).await?;
                                let updated_reg = GlobalBackupRegistry::load(paths)?;
                                if updated_reg.gdrive.is_none() {
                                    continue;
                                }
                            }
                            Some("gdrive".to_string())
                        }
                        3 => Some("multi".to_string()),
                        _ => continue,
                    };

                    let mut reg = ServersRegistry::load(paths)?;
                    if let Some(s) = reg.servers.iter_mut().find(|s| s.name == server.name) {
                        s.backup_method = new_method.clone();
                        reg.save(paths)?;
                        show_modal_message(
                            "BACKUP METHOD UPDATED",
                            &[
                                format!("[OK] Server '{}' now uses backup method: {}", server.name, new_method.as_deref().unwrap_or("local"))
                                    .green()
                                    .bold()
                                    .to_string(),
                            ],
                            false,
                        )?;
                    }
                }
            }
            Some(1) => {
                // Create backup now using selected method
                let scope_header = " Choose backup scope:";
                let scope_entries = vec![
                    MenuEntry::new("1", "Full Server Snapshot"),
                    MenuEntry::new("2", "World Only Snapshot"),
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

                let archive_path = match engine.create_backup(&server.name, &server.path, None, world_only).await {
                    Ok(p) => p,
                    Err(e) => {
                        show_modal_message("BACKUP FAILED", &[format!("[ERROR] {}", e)], true)?;
                        continue;
                    }
                };

                let fname = archive_path.file_name().and_then(|f| f.to_str()).unwrap_or("backup.tar.gz");
                let mut summary_lines = vec![
                    format!("[OK] Local archive saved: {}", archive_path.display()).green().bold().to_string(),
                ];

                let method = server.backup_method.as_deref().unwrap_or("local");
                if method == "s3" || method == "multi" {
                    if let Some(ref s3_config) = backup_reg.s3 {
                        let _ = print_in_place_status(
                            "UPLOADING TO S3",
                            &[format!("Uploading '{}' to S3 bucket '{}'...", fname, s3_config.bucket)],
                        );
                        let provider = S3StorageProvider::new(s3_config.clone());
                        let remote_key = if let Some(ref pfx) = s3_config.prefix {
                            format!("{}/{}/{}", pfx.trim_end_matches('/'), server.name, fname)
                        } else {
                            format!("{}/{}", server.name, fname)
                        };
                        match provider.upload_file(&archive_path, &remote_key).await {
                            Ok(_) => summary_lines.push(format!("[OK] S3 Upload complete: s3://{}/{}", s3_config.bucket, remote_key).green().to_string()),
                            Err(e) => summary_lines.push(format!("[WARN] S3 Upload failed: {}", e).yellow().to_string()),
                        }
                    } else if method == "s3" {
                        summary_lines.push("[WARN] S3 is not configured in Cloud Backups; kept locally.".yellow().to_string());
                    }
                }

                if method == "gdrive" || method == "multi" {
                    if let Some(ref gd_config) = backup_reg.gdrive {
                        let _ = print_in_place_status(
                            "UPLOADING TO GDRIVE",
                            &[format!("Uploading '{}' to Google Drive folder '{}'...", fname, gd_config.folder_id)],
                        );
                        let provider = GDriveStorageProvider::new(gd_config.clone());
                        match provider.upload_file(&archive_path, fname).await {
                            Ok(_) => summary_lines.push(format!("[OK] Google Drive Upload complete: folder {}", gd_config.folder_id).green().to_string()),
                            Err(e) => summary_lines.push(format!("[WARN] Google Drive Upload failed: {}", e).yellow().to_string()),
                        }
                    } else if method == "gdrive" {
                        summary_lines.push("[WARN] Google Drive is not configured; kept locally.".yellow().to_string());
                    }
                }

                show_modal_message("SNAPSHOT CREATED", &summary_lines, false)?;
            }
            Some(2) => {
                // List backups
                let list = engine.list_backups(&server.name);
                if list.is_empty() {
                    show_modal_message(
                        "NO BACKUPS FOUND",
                        &[format!("No local backups found for server '{}'.", server.name)],
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
                    show_modal_message(&format!("BACKUPS: {}", server.name), &lines, false)?;
                }
            }
            Some(3) => {
                // Restore server from backup
                if craft_core::is_server_locked(&server.path) || craft_core::get_server_running_pid(&server.path).is_some() {
                    let pid_info = craft_core::get_server_running_pid(&server.path).map(|p| format!(" (PID: {})", p)).unwrap_or_default();
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
                let mut b_entries = Vec::new();
                for (bi, b) in list.iter().enumerate() {
                    let hotkey = if bi < 9 {
                        (bi + 1).to_string()
                    } else {
                        ((b'a' + (bi - 9) as u8) as char).to_string()
                    };
                    let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                    b_entries.push(MenuEntry::new(
                        hotkey,
                        format!("{:<32} ({:.1} MB)", b.filename, mb),
                    ));
                }
                b_entries.push(MenuEntry::new("c", "Custom Archive Path"));
                b_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let b_header = format!(" Select backup archive to restore to '{}':", server.name);
                let mut b_sel = 0;
                if let Some(b_idx) = run_menu(&b_header, &b_entries, &mut b_sel)? {
                    let target_archive = if b_idx < list.len() {
                        paths.backups_dir.join(&server.name).join(&list[b_idx].filename)
                    } else if b_idx == list.len() {
                        match run_input_prompt("CUSTOM ARCHIVE", "Enter path to archive (.tar.gz / .zip):", None)? {
                            Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p.trim()),
                            _ => continue,
                        }
                    } else {
                        continue;
                    };

                    let _ = print_in_place_status(
                        "RESTORING SERVER",
                        &[format!("Unpacking backup '{}' into '{}'...", target_archive.display(), server.path.display())],
                    );
                    match engine.restore_backup(&target_archive, &server.path) {
                        Ok(_) => {
                            show_modal_message(
                                "RESTORE COMPLETE",
                                &[
                                    format!("[OK] Successfully restored server '{}' from backup!", server.name)
                                        .green()
                                        .bold()
                                        .to_string(),
                                ],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message("RESTORE FAILED", &[format!("[ERROR] {}", e)], true)?;
                        }
                    }
                }
            }
            Some(4) => {
                // Configure Auto-Backup Schedule
                super::cloud_backups::configure_single_policy(paths, &server.name).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn server_plugins_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
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
            MenuEntry::new("1", "Search & Install Plugins Online"),
            MenuEntry::new("2", "Install Plugin by Slug / ID"),
            MenuEntry::new("3", "Manage Installed Plugins"),
            MenuEntry::new("0", "Back to Server Menu").with_aliases(&["b", "q"]),
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
                    &[format!("Searching Modrinth, Hangar, and Poggit for '{}'...", query)],
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
                        let desc = if hit.description.len() > 40 {
                            format!("{}...", &hit.description[..37])
                        } else {
                            hit.description.clone()
                        };
                        p_entries.push(MenuEntry::new(
                            hotkey,
                            format!("{:<18} [{}] - {}", hit.name, hit.source, desc),
                        ));
                    }
                    p_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                    let p_header = format!(" Search results for '{}' - select to install directly into '{}':", query, server.name);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];
                            let _ = print_in_place_status(
                                "DOWNLOADING PLUGIN",
                                &[format!("Downloading '{}' into '{}'...", chosen.name, plugins_dir.display())],
                            );
                            match pm.install_from_modrinth(&server.path, &chosen.id_or_slug).await {
                                Ok(dest) => {
                                    show_modal_message(
                                        "PLUGIN INSTALLED",
                                        &[
                                            format!("[OK] Successfully installed '{}'!", chosen.name).green().bold().to_string(),
                                            format!("File: {}", dest.display()),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message("INSTALLATION FAILED", &[format!("[ERROR] {}", e)], true)?;
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
                    &[format!("Downloading '{}' into '{}'...", slug, plugins_dir.display())],
                );
                let pm = craft_plugins::PluginManager::new();
                match pm.install_from_modrinth(&server.path, &slug).await {
                    Ok(dest) => {
                        show_modal_message(
                            "PLUGIN INSTALLED",
                            &[
                                format!("[OK] Successfully installed plugin '{}'!", slug).green().bold().to_string(),
                                format!("File: {}", dest.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("INSTALLATION FAILED", &[format!("[ERROR] {}", e)], true)?;
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

async fn manage_installed_plugins_menu(server_name: &str, plugins_dir: &std::path::Path) -> Result<()> {
    let mut selected = 0;

    loop {
        let plugins = list_server_plugins(plugins_dir);
        if plugins.is_empty() {
            show_modal_message(
                "NO PLUGINS INSTALLED",
                &[format!("No plugin jars found in '{}'.", plugins_dir.display())],
                false,
            )?;
            return Ok(());
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server: '{}'\r\n Select a plugin jar to toggle status or delete:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("MANAGE INSTALLED PLUGINS", width, false).cyan().bold(),
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

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < plugins.len() => {
                let chosen = &plugins[idx];
                let item_header = format!(" Plugin: {}\r\n Choose action:", chosen.filename);
                let toggle_label = if chosen.is_enabled { "Disable Plugin (rename to .disabled)" } else { "Enable Plugin (rename to .jar)" };
                let item_entries = vec![
                    MenuEntry::new("1", toggle_label),
                    MenuEntry::new("2", "Delete Plugin File"),
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
                            &[format!("[OK] Removed '{}' from plugins directory.", chosen.filename)],
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
