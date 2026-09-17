#![allow(dead_code)]

use super::remote_backups::manage_remote_backups;
use super::remote_control::remote_server_control_panel;
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, clean_exit, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_paged_list_menu, show_modal_message, AltScreenGuard, BoxFrame,
    MenuEntry, NavGuard, PagedMenuAction,
};
use colored::Colorize;
use craft_core::{CraftPaths, RemoteHostConfig, Result};
use craft_remote::{RemoteCraftClient, RemoteServerInfo};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

struct BootstrapProgressState {
    current: String,
    steps: Vec<(String, bool)>,
    title: String,
    spinner_idx: usize,
}

fn run_boxed_bootstrap(
    session: &craft_remote::RemoteSession,
    host_alias: &str,
    title: &str,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let initial_msg = format!("Connecting and preparing environment on '{}'...", host_alias);
    let state = Arc::new(Mutex::new(BootstrapProgressState {
        current: initial_msg,
        steps: Vec::new(),
        title: title.to_string(),
        spinner_idx: 0,
    }));

    let stop_flag = Arc::new(AtomicBool::new(false));
    let state_clone = Arc::clone(&state);
    let stop_clone = Arc::clone(&stop_flag);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Hide);

    let render_handle = thread::spawn(move || {
        let mut out = io::stdout();
        while !stop_clone.load(Ordering::Relaxed) {
            {
                if let Ok(mut s) = state_clone.lock() {
                    s.spinner_idx = s.spinner_idx.wrapping_add(1);
                    let mut modal = modalx::modals::WaitingModal::new(&s.title, &s.current)
                        .with_max_width(84);
                    for (step_label, completed) in &s.steps {
                        modal = modal.with_step(step_label.clone(), *completed);
                    }
                    let _ = modal.render_spinner(s.spinner_idx, &mut out);
                }
            }
            thread::sleep(Duration::from_millis(80));
        }
    });

    let res = craft_remote::run_bootstrap_with_progress(session, |msg| {
        if let Ok(mut s) = state.lock() {
            if let Some(last) = s.steps.last_mut() {
                last.1 = true;
            }
            s.steps.push((msg.to_string(), false));
            s.current = msg.to_string();
        }
    });

    stop_flag.store(true, Ordering::Relaxed);
    let _ = render_handle.join();

    if res.is_ok() {
        if let Ok(mut s) = state.lock() {
            if let Some(last) = s.steps.last_mut() {
                last.1 = true;
            }
            s.spinner_idx = s.spinner_idx.wrapping_add(1);
            let mut modal = modalx::modals::WaitingModal::new(&s.title, "Host bootstrap completed successfully!")
                .with_max_width(84);
            for (step_label, completed) in &s.steps {
                modal = modal.with_step(step_label.clone(), *completed);
            }
            let _ = modal.render_spinner(s.spinner_idx, &mut stdout);
        }
        thread::sleep(Duration::from_millis(300));
    }

    if !modalx::terminal::is_alt_screen_active() {
        let _ = disable_raw_mode();
    }
    let _ = execute!(stdout, Show);

    res
}

pub async fn connect_with_cancellation(
    host_config: &RemoteHostConfig,
) -> Result<Option<RemoteCraftClient>> {
    let _alt = AltScreenGuard::enter();
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
                    show_modal_message(
                        "SSH CONNECTION FAILED",
                        &[format!(
                            "Failed to connect to host '{}': connection dropped.",
                            host_config.alias
                        )],
                        true,
                    )?;
                    return Ok(None);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }

            // Render connecting status
            let width = get_content_width(80);
            let mut frame = BoxFrame::new(width);
            frame.title = Some(("CONNECTING TO REMOTE HOST".to_string(), false));
            frame.empty_row();
            frame.row(format!(
                "Establishing SSH connection to '{}' ({}@{}:{}) {}",
                host_config.alias,
                host_config.user,
                host_config.host,
                host_config.port,
                frames[frame_idx % frames.len()]
            ));
            frame.empty_row();
            frame.footer("Press Esc to cancel.");
            frame.render(&mut stdout)?;

            frame_idx = (frame_idx + 1) % frames.len();

            // Poll for cancellation keys and drain all available events
            if event::poll(Duration::from_millis(120))? {
                while event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            if (key.modifiers.contains(KeyModifiers::CONTROL)
                                && (key.code == KeyCode::Char('c')
                                    || key.code == KeyCode::Char('C')))
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
        }
    })();

    // Drain any remaining events before returning
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }

    result
}

pub async fn manage_host_servers(
    _paths: &CraftPaths,
    host_config: &RemoteHostConfig,
) -> Result<()> {
    let _alt = AltScreenGuard::enter();
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
                match run_boxed_bootstrap(
                    &client.session,
                    &host_config.alias,
                    "BOOTSTRAPPING REMOTE HOST",
                ) {
                    Ok(_) => {
                        // Immediately transition into the remote host menu without blocking modal
                    }
                    Err(e) => {
                        show_modal_message("BOOTSTRAP FAILED", &[format!("[ERROR] {}", e)], true)?;
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

    if !client.is_craft_installed() {
        show_modal_message(
            "CRAFT NOT INSTALLED",
            &[
                format!(
                    "Craft CLI is required to manage servers on '{}'.",
                    host_config.alias
                ),
                "Please run bootstrap to install Craft.".to_string(),
            ],
            true,
        )?;
        return Ok(());
    }

    // Bidirectional version check: remote vs local
    let remote_version = client.get_craft_version();
    let local_version = craft_core::CRAFT_VERSION;

    if let Some(ref r_ver) = remote_version {
        let r_semver = craft_core::parse_semver(r_ver);
        let l_semver = craft_core::parse_semver(local_version);

        let is_remote_outdated = match (r_semver, l_semver) {
            (Some(r), Some(l)) => r < l,
            _ => r_ver != local_version,
        };

        let is_local_outdated = match (r_semver, l_semver) {
            (Some(r), Some(l)) => l < r,
            _ => false,
        };

        if is_remote_outdated {
            // Case 1: Remote is outdated
            let width = get_content_width(80);
            let update_header = format!(
                "{}\r\n{}\r\n{}\r\n Remote Host:          {}\r\n Remote Craft Version: {} [OUTDATED]\r\n Local Craft Version:  {} [NEWER]\r\n\r\n An updated version of Craft is available on this local machine.\r\n Would you like to update the remote binary now?\r\n{}\r\n Choose an action:\r\n{}",
                box_top(width).cyan().bold(),
                box_title("REMOTE CRAFT UPDATE AVAILABLE", width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
                host_config.alias.cyan().bold(),
                r_ver.yellow().bold(),
                local_version.green().bold(),
                box_divider(width).dimmed(),
                box_divider(width).dimmed(),
            );

            let update_entries = vec![
                MenuEntry::new("1", "Update Remote Craft Now")
                    .with_aliases(&["u", "update", "y", "yes"]),
                MenuEntry::new("2", "Continue Without Updating")
                    .with_aliases(&["c", "continue", "n", "no"]),
                MenuEntry::new("0", "Cancel").with_aliases(&["q"]),
            ];

            let mut u_sel = 0;
            match run_menu(&update_header, &update_entries, &mut u_sel)? {
                Some(0) => {
                    match run_boxed_bootstrap(
                        &client.session,
                        &host_config.alias,
                        "UPDATING REMOTE CRAFT",
                    ) {
                        Ok(_) => {
                            let _ = client.ensure_daemon_started();
                            show_modal_message(
                                "UPDATE COMPLETE",
                                &[format!(
                                    "[OK] Craft successfully updated to v{} on '{}'!",
                                    local_version, host_config.alias
                                )
                                .green()
                                .bold()
                                .to_string()],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "UPDATE FAILED",
                                &[format!("[ERROR] Failed to update remote Craft: {}", e)],
                                true,
                            )?;
                            return Ok(());
                        }
                    }
                }
                Some(1) => {
                    // Continue without updating
                }
                _ => return Ok(()),
            }
        } else if is_local_outdated {
            // Case 2: Local is outdated
            let width = get_content_width(80);
            let adv_header = format!(
                "{}\r\n{}\r\n{}\r\n Remote Host:          {}\r\n Remote Craft Version: {} [NEWER]\r\n Local Craft Version:  {} [OUTDATED]\r\n\r\n Warning: Remote host '{}' is running a newer Craft version.\r\n Updating remote is disabled to prevent downgrading remote services.\r\n We recommend updating Craft on your local machine.\r\n{}\r\n Choose an action:\r\n{}",
                box_top(width).yellow().bold(),
                box_title("LOCAL CRAFT OUTDATED", width, false).yellow().bold(),
                box_divider(width).yellow().bold(),
                host_config.alias.cyan().bold(),
                r_ver.green().bold(),
                local_version.yellow().bold(),
                host_config.alias.cyan().bold(),
                box_divider(width).dimmed(),
                box_divider(width).dimmed(),
            );

            let adv_entries = vec![
                MenuEntry::new("1", "Continue Connecting to Remote Host")
                    .with_aliases(&["c", "continue", "y"]),
                MenuEntry::new("0", "Cancel").with_aliases(&["q"]),
            ];

            let mut a_sel = 0;
            match run_menu(&adv_header, &adv_entries, &mut a_sel)? {
                Some(0) => {
                    // Continue connecting to remote host
                }
                _ => return Ok(()),
            }
        }
    }

    // Ensure remote daemon is running by default if craft is installed
    let _ = client.ensure_daemon_started();

    // Launch remote TUI session directly over interactive PTY
    let remote_cmd = format!(
        "export PATH=\"$HOME/.local/bin:$PATH\"; ~/.local/bin/craft ui --remote-node \"{}\" 2>/dev/null || craft ui --remote-node \"{}\"",
        host_config.alias, host_config.alias
    );
    if let Err(e) = craft_remote::run_remote_pty_session(&client.session, &remote_cmd) {
        show_modal_message(
            "REMOTE TUI ERROR",
            &[
                format!(
                    "Failed to run remote TUI session on '{}':",
                    host_config.alias
                ),
                format!("[ERROR] {}", e),
            ],
            true,
        )?;
    }

    Ok(())
}

async fn remote_manage_servers_menu(
    paths: &CraftPaths,
    client: &RemoteCraftClient,
    host_config: &RemoteHostConfig,
) -> Result<()> {
    let _nav = NavGuard::enter("Servers");
    let mut current_page = 0;
    let page_size = 7;
    let width = get_content_width(80);

    loop {
        let servers = match client.list_servers() {
            Ok(s) => s,
            Err(e) => {
                show_modal_message(
                    "REMOTE SERVERS ERROR",
                    &[format!(
                        "Failed to retrieve servers from remote host: {}",
                        e
                    )],
                    true,
                )?;
                return Ok(());
            }
        };

        if servers.is_empty() {
            let header = format!(
                "{}\r\n{}\r\n{}\r\n  No servers are currently registered on remote host '{}'.\r\n  Create your first Minecraft server to get started.\r\n{}",
                box_top(width),
                box_title(&format!("REMOTE SERVERS: {}", host_config.alias), width, false),
                box_divider(width),
                host_config.alias,
                box_divider(width)
            );

            let entries = vec![
                MenuEntry::new("1", "New Server").with_aliases(&["c", "n", "create", "new"]),
                MenuEntry::new("0", "Back").with_aliases(&["b"]),
            ];

            let mut selected = 0;
            match run_menu(&header, &entries, &mut selected)? {
                Some(0) => {
                    remote_create_server_wizard(client).await?;
                }
                _ => return Ok(()),
            }
            continue;
        }

        let action_entries =
            vec![MenuEntry::new("n", "New Server").with_aliases(&["c", "create", "new"])];

        let action = run_paged_list_menu(
            &servers,
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
                format!(
                    "{}\r\n{}\r\n{}\r\n Manage game servers on remote host '{}' (Total: {}){}.\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("REMOTE SERVERS: {}", host_config.alias), width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    host_config.alias,
                    total_count,
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
            true,
        )?;

        match action {
            PagedMenuAction::Select(idx) => {
                if idx < servers.len() {
                    remote_server_control_panel(paths, client, &servers[idx]).await?;
                }
            }
            PagedMenuAction::Space(idx) => {
                if idx < servers.len() {
                    let s = &servers[idx];
                    let (is_running, _) = client.check_server_running(&s.name, &s.path);
                    if is_running {
                        let _ = print_in_place_status(
                            "STOPPING SERVER",
                            &[format!("Stopping remote server '{}'...", s.name)],
                        );
                        let _ = client.stop_server(&s.name);
                    } else {
                        let _ = print_in_place_status(
                            "STARTING SERVER",
                            &[format!("Starting remote server '{}'...", s.name)],
                        );
                        let _ = client.start_server(&s.name);
                    }
                }
            }
            PagedMenuAction::Action(act) if act == "n" => {
                remote_create_server_wizard(client).await?;
            }
            _ => return Ok(()),
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

    let name = match run_input_prompt(
        "NEW REMOTE SERVER",
        "Enter server name (e.g. survival):",
        None,
    )? {
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

    let version = match run_input_prompt(
        "VERSION",
        "Enter Minecraft version (e.g. 1.21.4):",
        Some("1.21.4"),
    )? {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => "1.21.4".to_string(),
    };

    let port: u16 = match run_input_prompt("SERVER PORT", "Enter server port:", Some("25565"))? {
        Some(p) => p.trim().parse().unwrap_or(25565),
        None => return Ok(()),
    };

    let _ = print_in_place_status(
        "CREATING REMOTE SERVER",
        &[format!(
            "Creating server '{}' ({}:{}) on remote host...",
            name, software, version
        )],
    );

    match client.create_server(&name, software, &version, port) {
        Ok(_) => {
            show_modal_message(
                "SERVER CREATED",
                &[
                    format!("[OK] Successfully created remote server '{}'!", name)
                        .green()
                        .bold()
                        .to_string(),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message("CREATION FAILED", &[format!("[ERROR] {}", e)], true)?;
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
        &[format!(
            "Checking network latency to '{}' ({}:{})...",
            host.alias, host.host, host.port
        )],
    );

    let start = std::time::Instant::now();
    let is_connected = client.is_craft_installed();
    let elapsed = start.elapsed().as_millis();

    let mut lines = vec![
        format!(
            "Host:         {} ({}:{})",
            host.alias.white().bold(),
            host.host,
            host.port
        ),
        format!("SSH Latency:  {} ms", elapsed)
            .green()
            .bold()
            .to_string(),
        format!(
            "Craft CLI:    {}",
            if is_connected {
                "[FOUND]".green()
            } else {
                "[NOT FOUND]".red()
            }
        ),
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
            box_title(&format!("REMOTE DAEMON: {}", host_alias), width, false)
                .cyan()
                .bold(),
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
                    &[format!(
                        "Remote service daemon is currently: {}",
                        if status {
                            "[ONLINE]".green()
                        } else {
                            "[OFFLINE]".yellow()
                        }
                    )],
                    false,
                )?;
            }
            Some(1) => {
                let _ = print_in_place_status(
                    "STARTING DAEMON",
                    &[format!("Starting daemon on '{}'...", host_alias)],
                );
                match client.daemon_start() {
                    Ok(_) => {
                        show_modal_message(
                            "DAEMON STARTED",
                            &[format!("[OK] Remote daemon started on '{}'.", host_alias)
                                .green()
                                .bold()
                                .to_string()],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(2) => {
                let _ = print_in_place_status(
                    "STOPPING DAEMON",
                    &[format!("Stopping daemon on '{}'...", host_alias)],
                );
                match client.daemon_stop() {
                    Ok(_) => {
                        show_modal_message(
                            "DAEMON STOPPED",
                            &[format!("[OK] Remote daemon stopped on '{}'.", host_alias)
                                .green()
                                .bold()
                                .to_string()],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(3) => {
                let _ = print_in_place_status(
                    "RESTARTING DAEMON",
                    &[format!("Restarting daemon on '{}'...", host_alias)],
                );
                match client.daemon_restart() {
                    Ok(_) => {
                        show_modal_message(
                            "DAEMON RESTARTED",
                            &[format!("[OK] Remote daemon restarted on '{}'.", host_alias)
                                .green()
                                .bold()
                                .to_string()],
                            false,
                        )?;
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
        let _ = print_in_place_status(
            "PURGING CACHE",
            &[format!("Cleaning cache on '{}'...", host_alias)],
        );
        match client.clean_cache() {
            Ok(_) => {
                show_modal_message(
                    "CACHE PURGED",
                    &[format!(
                        "[OK] Cleared downloaded cache on remote host '{}'.",
                        host_alias
                    )
                    .green()
                    .bold()
                    .to_string()],
                    false,
                )?;
            }
            Err(e) => {
                show_modal_message(
                    "ERROR",
                    &[format!("[ERROR] Failed to clean remote cache: {}", e)],
                    true,
                )?;
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
                    format!(" | Page {} of {}", page, total_pages)
                        .cyan()
                        .to_string()
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
                    format!("Total Trashed Archives: {}", total_count)
                        .white()
                        .bold()
                        .to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages)
                        .cyan()
                        .to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Host: {}\r\n Directory: ~/.craft/trash/\r\n {}{}\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("REMOTE TRASH BIN: {}", host_alias), width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    host_alias.white().bold(),
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, item| format!("{:<40}  {}", item.filename, item.created_at),
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(idx) if idx < trash_items.len() => {
                let item = &trash_items[idx];
                let detail_header = format!(
                    "{}\r\n{}\r\n{}\r\n Archive:  {}\r\n Location: {}\r\n Select action:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("REMOTE TRASHED ARCHIVE", width, false)
                        .cyan()
                        .bold(),
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
                            let default_server = servers
                                .first()
                                .map(|s| s.name.as_str())
                                .unwrap_or("my-server");
                            if let Some(target_srv) = run_input_prompt(
                                "RESTORE TARGET",
                                "Enter destination server name:",
                                Some(default_server),
                            )? {
                                if !target_srv.trim().is_empty() {
                                    let _ = print_in_place_status(
                                        "RESTORING ARCHIVE",
                                        &[format!(
                                            "Restoring '{}' on remote host...",
                                            item.filename
                                        )],
                                    );
                                    match client.restore_trash(&item.filename, target_srv.trim()) {
                                        Ok(_) => {
                                            show_modal_message(
                                                "RESTORE COMPLETE",
                                                &[format!("[OK] Successfully restored '{}' to server '{}' backups.", item.filename, target_srv.trim()).green().bold().to_string()],
                                                false,
                                            )?;
                                        }
                                        Err(e) => {
                                            show_modal_message(
                                                "ERROR",
                                                &[format!(
                                                    "[ERROR] Failed to restore remote archive: {}",
                                                    e
                                                )],
                                                true,
                                            )?;
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
                                MenuEntry::new(
                                    "2",
                                    format!("Confirm Delete of '{}'", item.filename),
                                ),
                            ];
                            let mut c_sel = 0;
                            if let Some(1) =
                                run_menu(&confirm_header, &confirm_entries, &mut c_sel)?
                            {
                                match client.delete_trash_item(&item.filename) {
                                    Ok(_) => {
                                        show_modal_message(
                                            "DELETED",
                                            &[format!(
                                                "[OK] Permanently deleted '{}'.",
                                                item.filename
                                            )
                                            .green()
                                            .bold()
                                            .to_string()],
                                            false,
                                        )?;
                                    }
                                    Err(e) => {
                                        show_modal_message(
                                            "ERROR",
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
                            show_modal_message(
                                "TRASH EMPTIED",
                                &["[OK] Remote trash bin emptied.".green().bold().to_string()],
                                false,
                            )?;
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

pub async fn remote_uninstall_craft_wizard(
    client: &RemoteCraftClient,
    host_alias: &str,
) -> Result<bool> {
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
        MenuEntry::new(
            "2",
            format!("Confirm and Uninstall Craft from '{}'", host_alias),
        ),
    ];

    let mut sel2 = 0;
    match run_menu(&prompt2_header, &prompt2_entries, &mut sel2)? {
        Some(1) => {
            let _ = print_in_place_status(
                "UNINSTALLING CRAFT",
                &[format!(
                    "Uninstalling Craft and daemon from '{}'...",
                    host_alias
                )],
            );
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
                    show_modal_message(
                        "ERROR",
                        &[format!("[ERROR] Failed to uninstall Craft: {}", e)],
                        true,
                    )?;
                    Ok(false)
                }
            }
        }
        _ => Ok(false),
    }
}
