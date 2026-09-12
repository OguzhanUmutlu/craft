use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use craft_core::{CraftPaths, RemoteHostConfig, Result};
use craft_remote::{RemoteCraftClient, RemoteServerInfo};
use crate::commands::dashboard::screen::{
    box_bottom, box_divider, box_title, box_title_simple, box_top, clean_exit,
    get_content_width, print_in_place_status, run_input_prompt, run_menu,
    run_paged_list_menu, show_modal_message, MenuEntry, NavGuard, PagedMenuAction,
};
use super::remote_control::remote_server_control_panel;
use super::remote_backups::manage_remote_backups;

async fn connect_with_cancellation(host_config: &RemoteHostConfig) -> Result<Option<RemoteCraftClient>> {
    let (tx, rx) = mpsc::channel();
    let cfg_clone = host_config.clone();
    thread::spawn(move || {
        let res = RemoteCraftClient::connect(&cfg_clone);
        let _ = tx.send(res);
    });

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    let frames = [".  ", ".. ", "...", " ..", "  .", "   "];
    let mut frame_idx = 0;

    let result = (|| -> Result<Option<RemoteCraftClient>> {
        loop {
            // Check if connection completed
            match rx.try_recv() {
                Ok(Ok(client)) => return Ok(Some(client)),
                Ok(Err(e)) => {
                    let _ = disable_raw_mode();
                    let _ = execute!(io::stdout(), Show);
                    show_modal_message(
                        "SSH CONNECTION FAILED",
                        &[
                            format!("Failed to connect to host '{}':", host_config.alias),
                            format!("[ERROR] {}", e),
                            "".to_string(),
                            "Check network, host address, SSH credentials, and port.".to_string(),
                        ],
                        true,
                    )?;
                    return Ok(None);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = disable_raw_mode();
                    let _ = execute!(io::stdout(), Show);
                    show_modal_message(
                        "SSH CONNECTION FAILED",
                        &[
                            format!("Failed to connect to host '{}': connection dropped.", host_config.alias),
                        ],
                        true,
                    )?;
                    return Ok(None);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }

            // Render connecting status
            let width = get_content_width(80);
            execute!(stdout, MoveTo(0, 0))?;
            print!("{}\x1B[K\r\n", box_top(width));
            print!("{}\x1B[K\r\n", box_title_simple("CONNECTING TO REMOTE HOST", width, false));
            print!("{}\x1B[K\r\n", box_divider(width));
            print!("\x1B[K\r\n");
            print!(
                "  Establishing SSH connection to '{}' ({}@{}:{}) {}\x1B[K\r\n",
                host_config.alias, host_config.user, host_config.host, host_config.port, frames[frame_idx % frames.len()]
            );
            print!("  Press Esc, q, Backspace, or Left arrow to cancel.\x1B[K\r\n");
            print!("\x1B[K\r\n{}\x1B[K\r\n", box_bottom(width));
            execute!(stdout, Clear(ClearType::FromCursorDown))?;
            stdout.flush()?;

            frame_idx = (frame_idx + 1) % frames.len();

            // Poll for cancellation keys
            if event::poll(Duration::from_millis(120))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if (key.modifiers.contains(KeyModifiers::CONTROL)
                            && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')))
                            || key.code == KeyCode::Char('\x03')
                        {
                            clean_exit();
                        }

                        match key.code {
                            KeyCode::Esc
                            | KeyCode::Char('q')
                            | KeyCode::Char('Q')
                            | KeyCode::Left
                            | KeyCode::Backspace => {
                                return Ok(None);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    })();

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show);
    result
}

pub async fn manage_host_servers(
    paths: &CraftPaths,
    host_config: &RemoteHostConfig,
) -> Result<()> {
    let client = match connect_with_cancellation(host_config).await? {
        Some(c) => c,
        None => return Ok(()),
    };

    let _nav = NavGuard::enter(&host_config.alias);

    // Check if craft is installed on remote host
    if !client.is_craft_installed() {
        let width = get_content_width(80);
        let warn_header = format!(
            "{}\r\n{}\r\n{}\r\n Warning: 'craft' CLI is not found on remote host '{}'.\r\n To manage game servers, Craft needs to be installed on the remote machine.\r\n{}\r\n Choose an action:\r\n{}",
            box_top(width).yellow().bold(),
            box_title("CRAFT NOT FOUND ON REMOTE", width, false).yellow().bold(),
            box_divider(width).yellow().bold(),
            host_config.alias.cyan().bold(),
            box_divider(width).dimmed(),
            box_divider(width).dimmed(),
        );

        let warn_entries = vec![
            MenuEntry::new("1", "Bootstrap & Install Craft").with_aliases(&["b", "i"]),
            MenuEntry::new("2", "Continue Anyway").with_aliases(&["c"]),
            MenuEntry::new("0", "Cancel").with_aliases(&["q"]),
        ];

        let mut w_sel = 0;
        match run_menu(&warn_header, &warn_entries, &mut w_sel)? {
            Some(0) => {
                print_in_place_status(
                    "BOOTSTRAPPING REMOTE HOST",
                    &[format!("Installing Craft daemon and CLI on '{}'...", host_config.alias)],
                )?;

                match craft_remote::run_bootstrap(&client.session) {
                    Ok(_) => {
                        // Immediately transition into the remote host menu without blocking modal
                    }
                    Err(e) => {
                        show_modal_message(
                            "BOOTSTRAP FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                        return Ok(());
                    }
                }
            }
            Some(1) => {
                // Continue anyway
            }
            _ => return Ok(()),
        }
    }

    // Ensure remote daemon is running by default if craft is installed
    if client.is_craft_installed() {
        let _ = client.ensure_daemon_started();
    }

    let mut current_page = 0;
    let page_size = 6;

    loop {
        let servers = match client.list_servers() {
            Ok(s) => s,
            Err(e) => {
                show_modal_message(
                    "REMOTE SERVERS ERROR",
                    &[format!("Failed to retrieve servers from remote host: {}", e)],
                    true,
                )?;
                return Ok(());
            }
        };

        let width = get_content_width(80);

        let mut action_entries = vec![
            MenuEntry::new("n", "New Server").with_aliases(&["c", "create", "new"]),
            MenuEntry::new("p", "Ping Host / Servers"),
            MenuEntry::new("b", "Remote Backups"),
            MenuEntry::new("d", "Daemon Control"),
            MenuEntry::new("k", "Purge Cache"),
            MenuEntry::new("t", "Trash Bin"),
        ];
        if !client.is_craft_installed() {
            action_entries.push(MenuEntry::new("i", "Install Craft").with_aliases(&["bootstrap"]));
        } else {
            action_entries.push(MenuEntry::new("u", "Uninstall Craft").with_aliases(&["uninstall"]));
        }

        let action = run_paged_list_menu(
            &servers,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "No Craft servers currently registered on this remote host.".dimmed().to_string()
                } else {
                    format!("Total Registered Servers: {}", total_count).white().bold().to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Host: {:<16} | {}@{}:{}\r\n {}{}\r\n Select a server or choose a remote host management tool:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("REMOTE HOST: {}", host_config.alias), width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    host_config.alias.white().bold(),
                    host_config.user,
                    host_config.host,
                    host_config.port,
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, s| {
                let status_badge = if s.is_running {
                    if let Some(p) = s.pid {
                        format!("[RUNNING (PID: {})]", p).green().bold().to_string()
                    } else {
                        "[RUNNING]".green().bold().to_string()
                    }
                } else {
                    "[STOPPED]".dimmed().to_string()
                };
                format!(
                    "{:<20} {:<18} | {:<8} {:<8} | Port: {:<5}",
                    s.name, status_badge, s.server_type, s.version, s.port
                )
            },
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(idx) => {
                if idx < servers.len() {
                    remote_server_control_panel(paths, &client, &servers[idx]).await?;
                }
            }
            PagedMenuAction::Action(act) => match act.as_str() {
                "n" => {
                    remote_create_server_wizard(&client).await?;
                }
                "p" => {
                    remote_ping_host(&client, host_config, &servers).await?;
                }
                "b" => {
                    remote_manage_all_backups_picker(paths, &client, &servers).await?;
                }
                "d" => {
                    remote_daemon_control_menu(&client, &host_config.alias).await?;
                }
                "k" => {
                    remote_purge_cache_action(&client, &host_config.alias).await?;
                }
                "t" => {
                    remote_trash_menu(&client, &host_config.alias, &servers).await?;
                }
                "u" => {
                    if remote_uninstall_craft_wizard(&client, &host_config.alias).await? {
                        return Ok(());
                    }
                }
                "i" => {
                    print_in_place_status(
                        "BOOTSTRAPPING REMOTE HOST",
                        &[format!("Installing Craft daemon and CLI on '{}'...", host_config.alias)],
                    )?;
                    match craft_remote::run_bootstrap(&client.session) {
                        Ok(_) => {
                            show_modal_message(
                                "BOOTSTRAP COMPLETE",
                                &[format!("[OK] Successfully installed Craft on '{}' at ~/.local/bin/craft!", host_config.alias).green().bold().to_string()],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "BOOTSTRAP FAILED",
                                &[format!("[ERROR] {}", e)],
                                true,
                            )?;
                        }
                    }
                }
                _ => {}
            },
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}

async fn remote_create_server_wizard(client: &RemoteCraftClient) -> Result<()> {
    let _nav = NavGuard::enter("New Server");

    if !client.is_craft_installed() {
        show_modal_message(
            "CRAFT NOT FOUND ON REMOTE",
            &[
                "The 'craft' CLI binary is not installed on this remote host.".to_string(),
                "Please run option [i] 'Install Craft' to bootstrap and deploy Craft to ~/.local/bin/craft.".to_string(),
            ],
            true,
        )?;
        return Ok(());
    }

    let name = match run_input_prompt("NEW REMOTE SERVER", "Enter server name (e.g. survival):", None)? {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => return Ok(()),
    };

    let sw_header = " Select server software:";
    let sw_entries = vec![
        MenuEntry::new("1", "Paper (Recommended)"),
        MenuEntry::new("2", "Purpur"),
        MenuEntry::new("3", "Fabric"),
        MenuEntry::new("4", "Vanilla"),
        MenuEntry::new("5", "Bedrock Dedicated Server"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];
    let mut sw_sel = 0;
    let software = match run_menu(sw_header, &sw_entries, &mut sw_sel)? {
        Some(0) => "paper",
        Some(1) => "purpur",
        Some(2) => "fabric",
        Some(3) => "vanilla",
        Some(4) => "bedrock",
        _ => return Ok(()),
    };

    let version = match run_input_prompt("VERSION", "Enter Minecraft version (e.g. 1.21.4):", Some("1.21.4"))? {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => "1.21.4".to_string(),
    };

    let port: u16 = match run_input_prompt("SERVER PORT", "Enter server port:", Some("25565"))? {
        Some(p) => p.trim().parse().unwrap_or(25565),
        None => return Ok(()),
    };

    let _ = print_in_place_status(
        "CREATING REMOTE SERVER",
        &[format!("Creating server '{}' ({}:{}) on remote host...", name, software, version)],
    );

    match client.create_server(&name, software, &version, port) {
        Ok(_) => {
            show_modal_message(
                "SERVER CREATED",
                &[format!("[OK] Successfully created remote server '{}'!", name).green().bold().to_string()],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "CREATION FAILED",
                &[format!("[ERROR] {}", e)],
                true,
            )?;
        }
    }

    Ok(())
}

async fn remote_ping_host(
    client: &RemoteCraftClient,
    host: &RemoteHostConfig,
    servers: &[RemoteServerInfo],
) -> Result<()> {
    let _nav = NavGuard::enter("Ping Host");
    let _ = print_in_place_status(
        "PINGING REMOTE HOST",
        &[format!("Checking network latency to '{}' ({}:{})...", host.alias, host.host, host.port)],
    );

    let start = std::time::Instant::now();
    let is_connected = client.is_craft_installed();
    let elapsed = start.elapsed().as_millis();

    let mut lines = vec![
        format!("Host:         {} ({}:{})", host.alias.white().bold(), host.host, host.port),
        format!("SSH Latency:  {} ms", elapsed).green().bold().to_string(),
        format!("Craft CLI:    {}", if is_connected { "[FOUND]".green() } else { "[NOT FOUND]".red() }),
        "".to_string(),
        "Registered Servers & Port Status:".to_string(),
    ];

    if servers.is_empty() {
        lines.push("  (No servers registered on host)".dimmed().to_string());
    } else {
        for s in servers {
            let status = if s.is_running {
                format!("[RUNNING on port {}]", s.port).green().to_string()
            } else {
                format!("[STOPPED port {}]", s.port).dimmed().to_string()
            };
            lines.push(format!("  • {:<20} {}", s.name.white().bold(), status));
        }
    }

    show_modal_message("REMOTE HOST NETWORK PING", &lines, false)?;
    Ok(())
}

async fn remote_daemon_control_menu(client: &RemoteCraftClient, host_alias: &str) -> Result<()> {
    let _nav = NavGuard::enter("Daemon Control");
    let width = get_content_width(80);
    let mut sel = 0;

    loop {
        let is_online = client.daemon_status().unwrap_or(false);
        let status_badge = if is_online {
            "[ONLINE]".green().bold()
        } else {
            "[OFFLINE]".yellow().bold()
        };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Remote Host:   {}\r\n Daemon Status: {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("REMOTE DAEMON: {}", host_alias), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            host_alias.white().bold(),
            status_badge,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Check Daemon Status"),
            MenuEntry::new("2", "Start Daemon"),
            MenuEntry::new("3", "Stop Daemon"),
            MenuEntry::new("4", "Restart Daemon"),
            MenuEntry::new("5", "Uninstall Craft").with_aliases(&["u", "uninstall"]),
            MenuEntry::new("0", "Back").with_aliases(&["b"]),
        ];

        match run_menu(&header, &entries, &mut sel)? {
            Some(0) => {
                let status = client.daemon_status().unwrap_or(false);
                show_modal_message(
                    "REMOTE DAEMON STATUS",
                    &[format!("Remote service daemon is currently: {}", if status { "[ONLINE]".green() } else { "[OFFLINE]".yellow() })],
                    false,
                )?;
            }
            Some(1) => {
                let _ = print_in_place_status("STARTING DAEMON", &[format!("Starting daemon on '{}'...", host_alias)]);
                match client.daemon_start() {
                    Ok(_) => {
                        show_modal_message("DAEMON STARTED", &[format!("[OK] Remote daemon started on '{}'.", host_alias).green().bold().to_string()], false)?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(2) => {
                let _ = print_in_place_status("STOPPING DAEMON", &[format!("Stopping daemon on '{}'...", host_alias)]);
                match client.daemon_stop() {
                    Ok(_) => {
                        show_modal_message("DAEMON STOPPED", &[format!("[OK] Remote daemon stopped on '{}'.", host_alias).green().bold().to_string()], false)?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(3) => {
                let _ = print_in_place_status("RESTARTING DAEMON", &[format!("Restarting daemon on '{}'...", host_alias)]);
                match client.daemon_restart() {
                    Ok(_) => {
                        show_modal_message("DAEMON RESTARTED", &[format!("[OK] Remote daemon restarted on '{}'.", host_alias).green().bold().to_string()], false)?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(4) => {
                if remote_uninstall_craft_wizard(client, host_alias).await? {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn remote_purge_cache_action(client: &RemoteCraftClient, host_alias: &str) -> Result<()> {
    let width = get_content_width(80);
    let confirm_header = format!(
        "{}\r\n{}\r\n{}\r\n Are you sure you want to purge the downloaded asset cache on remote host '{}'?\r\n{}",
        box_top(width).yellow().bold(),
        box_title("CONFIRM REMOTE PURGE CACHE", width, false).yellow().bold(),
        box_divider(width).yellow().bold(),
        host_alias.white().bold(),
        box_divider(width).dimmed(),
    );

    let confirm_entries = vec![
        MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
        MenuEntry::new("2", "Confirm Purge Remote Cache"),
    ];

    let mut sel = 0;
    if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut sel)? {
        let _ = print_in_place_status("PURGING CACHE", &[format!("Cleaning cache on '{}'...", host_alias)]);
        match client.clean_cache() {
            Ok(_) => {
                show_modal_message(
                    "CACHE PURGED",
                    &[format!("[OK] Cleared downloaded cache on remote host '{}'.", host_alias).green().bold().to_string()],
                    false,
                )?;
            }
            Err(e) => {
                show_modal_message("ERROR", &[format!("[ERROR] Failed to clean remote cache: {}", e)], true)?;
            }
        }
    }
    Ok(())
}

async fn remote_manage_all_backups_picker(
    paths: &CraftPaths,
    client: &RemoteCraftClient,
    servers: &[RemoteServerInfo],
) -> Result<()> {
    let _nav = NavGuard::enter("Backups");
    if servers.is_empty() {
        show_modal_message(
            "NO SERVERS",
            &["No registered servers found on this remote host to view backups for.".to_string()],
            false,
        )?;
        return Ok(());
    }

    if servers.len() == 1 {
        return manage_remote_backups(paths, client, &servers[0]).await;
    }

    let mut current_page = 0;
    let page_size = 7;
    let width = get_content_width(80);

    loop {
        let action = run_paged_list_menu(
            servers,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };
                format!(
                    "{}\r\n{}\r\n{}\r\n Select a server to manage its remote backups (Total: {}){}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("REMOTE SERVER BACKUPS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    total_count,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, s| format!("{:<24} ({})", s.name, s.server_type),
            &[],
            false,
        )?;

        match action {
            PagedMenuAction::Select(idx) if idx < servers.len() => {
                manage_remote_backups(paths, client, &servers[idx]).await?;
            }
            _ => return Ok(()),
        }
    }
}

async fn remote_trash_menu(
    client: &RemoteCraftClient,
    host_alias: &str,
    servers: &[RemoteServerInfo],
) -> Result<()> {
    let _nav = NavGuard::enter("Trash Bin");
    let mut current_page = 0;
    let page_size = 7;
    let width = get_content_width(80);

    loop {
        let trash_items = client.list_trash().unwrap_or_default();
        let action_entries = if trash_items.is_empty() {
            vec![]
        } else {
            vec![MenuEntry::new("e", "Empty Remote Trash")]
        };

        let action = run_paged_list_menu(
            &trash_items,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "Remote trash bin is empty.".dimmed().to_string()
                } else {
                    format!("Total Trashed Archives: {}", total_count).white().bold().to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Host: {}\r\n Directory: ~/.craft/trash/\r\n {}{}\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("REMOTE TRASH BIN: {}", host_alias), width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    host_alias.white().bold(),
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, item| {
                format!("{:<40}  {}", item.filename, item.created_at)
            },
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(idx) if idx < trash_items.len() => {
                let item = &trash_items[idx];
                let detail_header = format!(
                    "{}\r\n{}\r\n{}\r\n Archive:  {}\r\n Location: {}\r\n Select action:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("REMOTE TRASHED ARCHIVE", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    item.filename.white().bold(),
                    item.remote_path,
                    box_divider(width).dimmed(),
                );

                let detail_entries = vec![
                    MenuEntry::new("1", "Restore to Server"),
                    MenuEntry::new("2", "Delete Permanently"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];

                let mut sel = 0;
                if let Some(act) = run_menu(&detail_header, &detail_entries, &mut sel)? {
                    match act {
                        0 => {
                            // Restore
                            let default_server = servers.first().map(|s| s.name.as_str()).unwrap_or("my-server");
                            if let Some(target_srv) = run_input_prompt("RESTORE TARGET", "Enter destination server name:", Some(default_server))? {
                                if !target_srv.trim().is_empty() {
                                    let _ = print_in_place_status("RESTORING ARCHIVE", &[format!("Restoring '{}' on remote host...", item.filename)]);
                                    match client.restore_trash(&item.filename, target_srv.trim()) {
                                        Ok(_) => {
                                            show_modal_message(
                                                "RESTORE COMPLETE",
                                                &[format!("[OK] Successfully restored '{}' to server '{}' backups.", item.filename, target_srv.trim()).green().bold().to_string()],
                                                false,
                                            )?;
                                        }
                                        Err(e) => {
                                            show_modal_message("ERROR", &[format!("[ERROR] Failed to restore remote archive: {}", e)], true)?;
                                        }
                                    }
                                }
                            }
                        }
                        1 => {
                            // Delete permanently
                            let confirm_header = format!(
                                "{}\r\n{}\r\n{}\r\n Are you sure you want to PERMANENTLY delete '{}' from remote host?\r\n{}",
                                box_top(width).red().bold(),
                                box_title("CONFIRM DELETE", width, false).red().bold(),
                                box_divider(width).red().bold(),
                                item.filename.white().bold(),
                                box_divider(width).dimmed(),
                            );
                            let confirm_entries = vec![
                                MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                                MenuEntry::new("2", format!("Confirm Delete of '{}'", item.filename)),
                            ];
                            let mut c_sel = 0;
                            if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                                match client.delete_trash_item(&item.filename) {
                                    Ok(_) => {
                                        show_modal_message("DELETED", &[format!("[OK] Permanently deleted '{}'.", item.filename).green().bold().to_string()], false)?;
                                    }
                                    Err(e) => {
                                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            PagedMenuAction::Action(act) if act == "e" => {
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Are you sure you want to permanently delete ALL items in the remote trash bin?\r\n This action cannot be undone.\r\n{}",
                    box_top(width).red().bold(),
                    box_title("CONFIRM EMPTY REMOTE TRASH", width, false).red().bold(),
                    box_divider(width).red().bold(),
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                    MenuEntry::new("2", "Confirm Empty Remote Trash"),
                ];
                let mut c_sel = 0;
                if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    match client.empty_trash() {
                        Ok(_) => {
                            show_modal_message("TRASH EMPTIED", &["[OK] Remote trash bin emptied.".green().bold().to_string()], false)?;
                        }
                        Err(e) => {
                            show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn remote_uninstall_craft_wizard(client: &RemoteCraftClient, host_alias: &str) -> Result<bool> {
    let _nav = NavGuard::enter("Uninstall Craft");
    let width = get_content_width(80);

    // Confirmation Prompt 1 of 2: Warning & intent check
    let prompt1_header = format!(
        "{}\r\n{}\r\n{}\r\n Warning: You are about to UNINSTALL Craft from remote host '{}'.\r\n\r\n This will:\r\n  - Stop all running Craft background daemons and services\r\n  - Disable and remove the systemd user service unit\r\n  - Delete the 'craft' CLI binary from ~/.local/bin/craft\r\n\r\n Do you wish to proceed to confirmation? (Prompt 1 of 2)\r\n{}",
        box_top(width).yellow().bold(),
        box_title("UNINSTALL CRAFT - PROMPT 1 OF 2", width, false).yellow().bold(),
        box_divider(width).yellow().bold(),
        host_alias.white().bold(),
        box_divider(width).dimmed(),
    );

    let prompt1_entries = vec![
        MenuEntry::new("1", "Cancel").with_aliases(&["0", "b", "q"]),
        MenuEntry::new("2", "Proceed to Final Confirmation"),
    ];

    let mut sel1 = 0;
    match run_menu(&prompt1_header, &prompt1_entries, &mut sel1)? {
        Some(1) => {}
        _ => return Ok(false),
    }

    // Confirmation Prompt 2 of 2: Destructive action confirmation
    let prompt2_header = format!(
        "{}\r\n{}\r\n{}\r\n FINAL CONFIRMATION: Are you ABSOLUTELY sure?\r\n\r\n Remote host '{}' will no longer have Craft installed.\r\n You will need to re-install / bootstrap Craft before managing servers again.\r\n\r\n (Prompt 2 of 2)\r\n{}",
        box_top(width).red().bold(),
        box_title("FINAL CONFIRMATION - PROMPT 2 OF 2", width, false).red().bold(),
        box_divider(width).red().bold(),
        host_alias.white().bold(),
        box_divider(width).dimmed(),
    );

    let prompt2_entries = vec![
        MenuEntry::new("1", "Cancel (Keep Craft Installed)").with_aliases(&["0", "b", "q"]),
        MenuEntry::new("2", format!("Confirm and Uninstall Craft from '{}'", host_alias)),
    ];

    let mut sel2 = 0;
    match run_menu(&prompt2_header, &prompt2_entries, &mut sel2)? {
        Some(1) => {
            let _ = print_in_place_status("UNINSTALLING CRAFT", &[format!("Uninstalling Craft and daemon from '{}'...", host_alias)]);
            match client.uninstall_craft() {
                Ok(_) => {
                    show_modal_message(
                        "CRAFT UNINSTALLED",
                        &[
                            format!("[OK] Craft has been successfully uninstalled from remote host '{}'.", host_alias).green().bold().to_string(),
                            "Daemon services stopped and removed from ~/.local/bin/craft.".to_string(),
                        ],
                        false,
                    )?;
                    Ok(true)
                }
                Err(e) => {
                    show_modal_message("ERROR", &[format!("[ERROR] Failed to uninstall Craft: {}", e)], true)?;
                    Ok(false)
                }
            }
        }
        _ => Ok(false),
    }
}

