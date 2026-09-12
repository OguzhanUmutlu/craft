use colored::Colorize;
use craft_core::{CraftPaths, RemoteHostConfig, Result};
use craft_remote::{RemoteCraftClient, RemoteServerInfo};
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_paged_list_menu, show_modal_message,
    MenuEntry, PagedMenuAction,
};
use super::remote_control::remote_server_control_panel;
use super::remote_backups::manage_remote_backups;

pub async fn manage_host_servers(
    paths: &CraftPaths,
    host_config: &RemoteHostConfig,
) -> Result<()> {
    print_in_place_status(
        "CONNECTING TO REMOTE HOST",
        &[format!(
            "Establishing SSH connection to '{}' ({}@{}:{})...",
            host_config.alias, host_config.user, host_config.host, host_config.port
        )],
    )?;

    let client = match RemoteCraftClient::connect(host_config) {
        Ok(c) => c,
        Err(e) => {
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
            return Ok(());
        }
    };

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

        let action_entries = vec![
            MenuEntry::new("n", "Create Server").with_aliases(&["c"]),
            MenuEntry::new("p", "Ping Host / Servers"),
            MenuEntry::new("b", "Remote Backups"),
            MenuEntry::new("d", "Daemon Control"),
            MenuEntry::new("k", "Purge Cache"),
            MenuEntry::new("t", "Trash Bin"),
        ];

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
                _ => {}
            },
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}

async fn remote_create_server_wizard(client: &RemoteCraftClient) -> Result<()> {
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
