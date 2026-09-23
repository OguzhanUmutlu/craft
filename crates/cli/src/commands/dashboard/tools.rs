use colored::Colorize;
use std::path::PathBuf;

use craft_backup::BackupEngine;
use craft_core::{
    format_size, kill_process, read_pid_file, CraftPaths, ModpackRegistry, RemoteAuthType,
    RemoteHostConfig, RemotesRegistry, Result, ServersRegistry,
};
use craft_daemon::DaemonClient;
use craft_plugins::PluginManager;
use craft_providers::CacheManager;

use super::screen::{
    box_divider, box_title, box_top, exec_console_action, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};
use super::server_control::show_empty_servers_modal;
use crate::commands::remote::parse_connection_string;

pub async fn ping_menu() -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Server Ping");
    let target = match run_input_prompt(
        "SERVER NETWORK PING",
        "Enter server address (IP:Port or Domain):",
        Some("127.0.0.1:25565"),
    )? {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => return Ok(()),
    };

    let mut proto_sel = 0;
    let width = get_content_width(80);
    let proto_header = format!(
        "{}\r\n{}\r\n{}\r\n Target: {}\r\n Select protocol query mode:\r\n{}",
        box_top(width).cyan().bold(),
        box_title("SELECT PING PROTOCOL", width, false)
            .cyan()
            .bold(),
        box_divider(width).cyan().bold(),
        target,
        box_divider(width).dimmed(),
    );

    let proto_entries = vec![
        MenuEntry::new("1", "Auto-Detect / Universal Probe"),
        MenuEntry::new("2", "Minecraft Java Edition (SLP)"),
        MenuEntry::new("3", "Minecraft Bedrock Edition (RakNet)"),
        MenuEntry::new("4", "Valve / Steam Engine (A2S - Palworld, Valheim, etc.)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let choice = run_menu(&proto_header, &proto_entries, &mut proto_sel)?;
    let (proto_hint, default_port) = match choice {
        Some(0) => (None, 25565),
        Some(1) => (Some(craft_core::QueryProtocolKind::MinecraftJavaSlp), 25565),
        Some(2) => (
            Some(craft_core::QueryProtocolKind::MinecraftBedrockRakNet),
            19132,
        ),
        Some(3) => (Some(craft_core::QueryProtocolKind::ValveA2S), 27015),
        _ => return Ok(()),
    };

    let (host, port) = if let Some(idx) = target.find(':') {
        let (h, p) = target.split_at(idx);
        let port_num: u16 = p[1..].parse().unwrap_or(default_port);
        (h.to_string(), port_num)
    } else {
        (target.clone(), default_port)
    };

    print_in_place_status(
        "PINGING SERVER",
        &[format!("Connecting to {}:{}...", host, port)],
    )?;

    match craft_net::ping_server_auto(&host, port, proto_hint).await {
        Ok(craft_net::UniversalPingStatus::MinecraftJava(res)) => {
            show_modal_message(
                "JAVA SERVER ONLINE",
                &[
                    format!("MOTD:     {}", res.motd),
                    format!(
                        "Version:  {} (Protocol {})",
                        res.version_name, res.protocol_version
                    ),
                    format!("Players:  {}/{}", res.online_players, res.max_players),
                    format!("Latency:  {} ms", res.latency_ms),
                ],
                false,
            )?;
        }
        Ok(craft_net::UniversalPingStatus::MinecraftBedrock(res)) => {
            show_modal_message(
                "BEDROCK SERVER ONLINE",
                &[
                    format!("Server Name: {}", res.server_name),
                    format!(
                        "Version:     {} (Protocol {})",
                        res.version, res.protocol_version
                    ),
                    format!("Players:     {}/{}", res.online_players, res.max_players),
                    format!("World:       {}", res.world_name),
                    format!("Latency:     {} ms", res.latency_ms),
                ],
                false,
            )?;
        }
        Ok(craft_net::UniversalPingStatus::ValveA2S(res)) => {
            show_modal_message(
                "STEAM / VALVE A2S ONLINE",
                &[
                    format!("Server Name: {}", res.server_name),
                    format!(
                        "Game / Map:  {} ({}) / {}",
                        res.game_name, res.game_folder, res.map_name
                    ),
                    format!(
                        "Players:     {}/{} (Bots: {})",
                        res.online_players, res.max_players, res.bots
                    ),
                    format!("Server Type: {} [{}]", res.server_type, res.environment),
                    format!(
                        "VAC Secured: {}",
                        if res.vac_secured { "Yes" } else { "No" }
                    ),
                    format!("Latency:     {} ms", res.latency_ms),
                ],
                false,
            )?;
        }
        Ok(craft_net::UniversalPingStatus::PortProbe {
            latency_ms,
            transport,
            ..
        }) => {
            show_modal_message(
                "SERVER PORT OPEN",
                &[
                    format!("Target:    {}:{}", host, port),
                    format!("Transport: {}", transport),
                    "Status:    Port Open / Responsive".to_string(),
                    format!("Latency:   {} ms", latency_ms),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "PING FAILED",
                &[format!("[ERROR] Could not reach {}:{}: {}", host, port, e)],
                true,
            )?;
        }
    }

    Ok(())
}

pub async fn backups_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let bk_reg = craft_core::GlobalBackupRegistry::load(paths)?;
        let s3_status = if let Some(ref s3) = bk_reg.s3 {
            format!("[S3: {}]", s3.bucket).green().bold().to_string()
        } else {
            "[S3: OFF]".dimmed().to_string()
        };
        let gd_status = if let Some(ref gd) = bk_reg.gdrive {
            format!("[GDrive: {}]", gd.folder_id)
                .green()
                .bold()
                .to_string()
        } else {
            "[GDrive: OFF]".dimmed().to_string()
        };

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Centralized backup engine: configure cloud providers, policies, and snapshots.\r\n Active Systems: [Local: ~/.craft/backups] {} {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("WORLD SNAPSHOTS & BACKUP SYSTEMS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            s3_status,
            gd_status,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new(
                "1",
                "Setup Backup Systems (Local, S3 / R2 / MinIO, Google Drive)",
            )
            .with_aliases(&["s", "c"]),
            MenuEntry::new("2", "Automated Backup Policies & Retention").with_aliases(&["p", "a"]),
            MenuEntry::new("3", "Create Server Snapshot").with_aliases(&["n"]),
            MenuEntry::new("4", "List Existing Backups").with_aliases(&["l"]),
            MenuEntry::new("5", "Restore Server from Backup").with_aliases(&["r"]),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                super::cloud_backups::setup_backup_systems_menu(paths).await?;
            }
            Some(1) => {
                super::cloud_backups::configure_policies_menu(paths).await?;
            }
            Some(2) => {
                if registry.servers.is_empty() {
                    show_empty_servers_modal(paths).await?;
                    continue;
                }
                // Create
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 {
                        (i + 1).to_string()
                    } else {
                        ((b'a' + (i - 9) as u8) as char).to_string()
                    };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let s_header = " Select server to snapshot:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];
                        let mode_header = " Choose backup scope:";
                        let mode_entries = vec![
                            MenuEntry::new("1", "Full Server Snapshot"),
                            MenuEntry::new("2", "World Only Snapshot"),
                            MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                        ];
                        let mut m_sel = 0;
                        if let Some(m_idx) = run_menu(mode_header, &mode_entries, &mut m_sel)? {
                            let world_only = match m_idx {
                                0 => false,
                                1 => true,
                                _ => continue,
                            };
                            print_in_place_status(
                                "CREATING BACKUP",
                                &[format!("Compressing snapshot for '{}'...", server.name)],
                            )?;
                            let engine = BackupEngine::new(paths);
                            match engine
                                .create_backup(&server.name, &server.path, None, world_only, None)
                                .await
                            {
                                Ok(file) => {
                                    let meta = std::fs::metadata(&file)?;
                                    let mb = (meta.len() as f64) / (1024.0 * 1024.0);
                                    show_modal_message(
                                        "SNAPSHOT CREATED",
                                        &[
                                            format!(
                                                "[OK] Successfully saved snapshot for server '{}'!",
                                                server.name
                                            )
                                            .green()
                                            .bold()
                                            .to_string(),
                                            format!("File: {}", file.display()),
                                            format!("Size: {:.2} MB", mb),
                                        ],
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
                    }
                }
            }
            Some(3) => {
                // List
                if registry.servers.is_empty() {
                    show_empty_servers_modal(paths).await?;
                    continue;
                }
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 {
                        (i + 1).to_string()
                    } else {
                        ((b'a' + (i - 9) as u8) as char).to_string()
                    };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let s_header = " Select server to view backups:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];
                        let engine = BackupEngine::new(paths);
                        let list = engine.list_backups(&server.name);
                        if list.is_empty() {
                            show_modal_message(
                                "NO BACKUPS FOUND",
                                &[format!("No backups exist for server '{}'.", server.name)],
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
                            show_modal_message(
                                &format!("BACKUPS FOR {}", server.name),
                                &lines,
                                false,
                            )?;
                        }
                    }
                }
            }
            Some(4) => {
                // Restore
                if registry.servers.is_empty() {
                    show_empty_servers_modal(paths).await?;
                    continue;
                }
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 {
                        (i + 1).to_string()
                    } else {
                        ((b'a' + (i - 9) as u8) as char).to_string()
                    };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let s_header = " Select server to restore:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];

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

                        let engine = BackupEngine::new(paths);

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

                        let b_header =
                            format!(" Select backup archive to restore to '{}':", server.name);
                        let mut b_sel = 0;
                        if let Some(b_idx) = run_menu(&b_header, &b_entries, &mut b_sel)? {
                            let target_archive = if b_idx < list.len() {
                                paths
                                    .backups_dir
                                    .join(&server.name)
                                    .join(&list[b_idx].filename)
                            } else if b_idx == list.len() {
                                match run_input_prompt(
                                    "CUSTOM ARCHIVE",
                                    "Enter path to archive (.tar.gz / .zip):",
                                    None,
                                )? {
                                    Some(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
                                    _ => continue,
                                }
                            } else {
                                continue;
                            };

                            print_in_place_status(
                                "RESTORING SERVER",
                                &[format!(
                                    "Unpacking backup '{}' into '{}'...",
                                    target_archive.display(),
                                    server.path.display()
                                )],
                            )?;
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
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn plugins_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Search and install Minecraft plugins and extensions with one click.\r\n{}",
            box_top(width).cyan().bold(),
            box_title("PLUGINS & EXTENSIONS MANAGER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Search Plugins"),
            MenuEntry::new("2", "Install Plugin"),
            MenuEntry::new("3", "Search Mods"),
            MenuEntry::new("4", "Search Datapacks"),
            MenuEntry::new("5", "Curated Maps"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let query = match run_input_prompt(
                    "SEARCH PLUGINS ONLINE",
                    "Search keyword (e.g. essentials, viaversion, luckperms, worldedit):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                print_in_place_status(
                    "SEARCHING PLUGINS",
                    &[format!("Querying plugin repositories for '{}'...", query)],
                )?;
                let pm = PluginManager::new();
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
                    p_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                    let p_header = format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen_plugin = &results[p_idx];
                            let registry = ServersRegistry::load(paths)?;
                            let compatible: Vec<_> = registry
                                .servers
                                .iter()
                                .filter(|s| {
                                    craft_providers::get_content_capabilities(&s.software).plugins
                                })
                                .collect();
                            if compatible.is_empty() {
                                show_modal_message(
                                    "NO COMPATIBLE SERVERS",
                                    &[
                                        "No registered servers support plugins.".to_string(),
                                        "".to_string(),
                                        "Plugins only exist in non-vanilla server softwares (e.g. Paper, Purpur, Spigot).".yellow().to_string(),
                                    ],
                                    false,
                                )?;
                                continue;
                            }

                            let mut s_entries = Vec::new();
                            for (si, s) in compatible.iter().enumerate() {
                                let hotkey = if si < 9 {
                                    (si + 1).to_string()
                                } else {
                                    ((b'a' + (si - 9) as u8) as char).to_string()
                                };
                                s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                            }
                            s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                            let s_header =
                                format!(" Install '{}' to which server?", chosen_plugin.name);
                            let mut s_sel = 0;
                            if let Some(s_idx) = run_menu(&s_header, &s_entries, &mut s_sel)? {
                                if s_idx < compatible.len() {
                                    let server = compatible[s_idx];
                                    print_in_place_status(
                                        "INSTALLING PLUGIN",
                                        &[format!(
                                            "Downloading '{}' into '{}'...",
                                            chosen_plugin.name,
                                            server.path.display()
                                        )],
                                    )?;
                                    match pm
                                        .install_from_modrinth(
                                            &server.path,
                                            &chosen_plugin.id_or_slug,
                                        )
                                        .await
                                    {
                                        Ok(dest) => {
                                            show_modal_message(
                                                "PLUGIN INSTALLED",
                                                &[
                                                    format!(
                                                        "[OK] Installed '{}' successfully!",
                                                        chosen_plugin.name
                                                    )
                                                    .green()
                                                    .bold()
                                                    .to_string(),
                                                    format!("Server: {}", server.name),
                                                    format!("File:   {}", dest.display()),
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
                }
            }
            Some(1) => {
                let registry = ServersRegistry::load(paths)?;
                let compatible: Vec<_> = registry
                    .servers
                    .iter()
                    .filter(|s| craft_providers::get_content_capabilities(&s.software).plugins)
                    .collect();
                if compatible.is_empty() {
                    show_modal_message(
                        "NO COMPATIBLE SERVERS",
                        &[
                            "No registered servers support plugins.".to_string(),
                            "".to_string(),
                            "Plugins only exist in non-vanilla server softwares (e.g. Paper, Purpur, Spigot).".yellow().to_string(),
                        ],
                        false,
                    )?;
                    continue;
                }

                let mut s_entries = Vec::new();
                for (si, s) in compatible.iter().enumerate() {
                    let hotkey = if si < 9 {
                        (si + 1).to_string()
                    } else {
                        ((b'a' + (si - 9) as u8) as char).to_string()
                    };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let s_header = " Select server to install plugin:";
                let mut s_sel = 0;
                if let Some(s_idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if s_idx < compatible.len() {
                        let server = compatible[s_idx];
                        let project_id = match run_input_prompt(
                            "PLUGIN ID",
                            "Enter Modrinth plugin slug or ID (e.g. luckperms, spark):",
                            None,
                        )? {
                            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
                            _ => continue,
                        };

                        print_in_place_status(
                            "INSTALLING PLUGIN",
                            &[format!(
                                "Installing '{}' to '{}'...",
                                project_id, server.name
                            )],
                        )?;
                        let pm = PluginManager::new();
                        match pm.install_from_modrinth(&server.path, &project_id).await {
                            Ok(dest) => {
                                show_modal_message(
                                    "PLUGIN INSTALLED",
                                    &[
                                        format!(
                                            "[OK] Installed plugin '{}' successfully!",
                                            project_id
                                        )
                                        .green()
                                        .bold()
                                        .to_string(),
                                        format!("Server: {}", server.name),
                                        format!("File:   {}", dest.display()),
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
            Some(2) => {
                let query = match run_input_prompt(
                    "SEARCH MODS ONLINE",
                    "Search keyword (e.g. fabric-api, sodium, lithium, appleskin, jei):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                print_in_place_status(
                    "SEARCHING MODS",
                    &[format!("Querying Modrinth repository for '{}'...", query)],
                )?;
                let pm = PluginManager::new();
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
                    p_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                    let p_header = format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];

                            let registry = ServersRegistry::load(paths)?;
                            let compatible: Vec<_> = registry
                                .servers
                                .iter()
                                .filter(|s| {
                                    craft_providers::get_content_capabilities(&s.software).mods
                                })
                                .collect();
                            if compatible.is_empty() {
                                show_modal_message(
                                    "NO COMPATIBLE SERVERS",
                                    &[
                                        "No registered servers support mods.".to_string(),
                                        "".to_string(),
                                        "Mods only exist in modded server softwares (e.g. Fabric, Quilt, NeoForge).".yellow().to_string(),
                                    ],
                                    false,
                                )?;
                                continue;
                            }

                            let mut s_entries = Vec::new();
                            for (si, s) in compatible.iter().enumerate() {
                                let hotkey = if si < 9 {
                                    (si + 1).to_string()
                                } else {
                                    ((b'a' + (si - 9) as u8) as char).to_string()
                                };
                                s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                            }
                            s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                            let s_header =
                                format!(" Select server to install mod '{}':", chosen.name);
                            let mut s_sel = 0;
                            if let Some(s_idx) = run_menu(&s_header, &s_entries, &mut s_sel)? {
                                if s_idx < compatible.len() {
                                    let server = compatible[s_idx];
                                    print_in_place_status(
                                        "INSTALLING MOD",
                                        &[format!(
                                            "Downloading and installing '{}' to server '{}'...",
                                            chosen.name, server.name
                                        )],
                                    )?;

                                    match pm
                                        .install_mod_from_modrinth(&server.path, &chosen.id_or_slug)
                                        .await
                                    {
                                        Ok(dest) => {
                                            show_modal_message(
                                                "MOD INSTALLED",
                                                &[
                                                    format!(
                                                        "[OK] Installed mod '{}' successfully!",
                                                        chosen.name
                                                    )
                                                    .green()
                                                    .bold()
                                                    .to_string(),
                                                    format!("Server: {}", server.name),
                                                    format!("File:   {}", dest.display()),
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
                }
            }
            Some(3) => {
                let query = match run_input_prompt(
                    "SEARCH DATAPACKS ONLINE",
                    "Search keyword (e.g. terralith, incendium, nullscape, timber):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                print_in_place_status(
                    "SEARCHING DATAPACKS",
                    &[format!("Querying Modrinth repository for '{}'...", query)],
                )?;
                let pm = PluginManager::new();
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
                    p_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                    let p_header = format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];

                            let registry = ServersRegistry::load(paths)?;
                            let compatible: Vec<_> = registry
                                .servers
                                .iter()
                                .filter(|s| {
                                    craft_providers::get_content_capabilities(&s.software).datapacks
                                })
                                .collect();
                            if compatible.is_empty() {
                                show_modal_message(
                                    "NO COMPATIBLE SERVERS",
                                    &[
                                        "No registered servers support datapacks.".to_string(),
                                        "".to_string(),
                                        "Datapacks are only supported on Minecraft Java world servers.".yellow().to_string(),
                                    ],
                                    false,
                                )?;
                                continue;
                            }

                            let mut s_entries = Vec::new();
                            for (si, s) in compatible.iter().enumerate() {
                                let hotkey = if si < 9 {
                                    (si + 1).to_string()
                                } else {
                                    ((b'a' + (si - 9) as u8) as char).to_string()
                                };
                                s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                            }
                            s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                            let s_header =
                                format!(" Select server to install datapack '{}':", chosen.name);
                            let mut s_sel = 0;
                            if let Some(s_idx) = run_menu(&s_header, &s_entries, &mut s_sel)? {
                                if s_idx < compatible.len() {
                                    let server = compatible[s_idx];
                                    let default_world = craft_core::get_default_world(&server.path);
                                    print_in_place_status(
                                        "INSTALLING DATAPACK",
                                        &[format!(
                                            "Downloading and installing '{}' to server '{}' (world: {})...",
                                            chosen.name, server.name, default_world
                                        )],
                                    )?;

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
                                                        "[OK] Installed datapack '{}' successfully!",
                                                        chosen.name
                                                    )
                                                    .green()
                                                    .bold()
                                                    .to_string(),
                                                    format!("Server: {}", server.name),
                                                    format!("File:   {}", dest.display()),
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
                }
            }
            Some(4) => {
                let maps = craft_plugins::world::get_curated_maps();
                let width = get_content_width(80);
                let map_header = format!(
                    "{}\r\n{}\r\n{}\r\n Select a popular community map to install:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("CURATED MAPS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    box_divider(width).dimmed(),
                );

                let mut m_entries = Vec::new();
                for (i, m) in maps.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    let desc = craft_core::truncate_ellipsis(m.description, 40);
                    m_entries.push(MenuEntry::new(
                        hotkey,
                        format!("{:<20} [{}] - {}", m.name, m.category, desc),
                    ));
                }
                m_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let mut m_sel = 0;
                if let Some(m_idx) = run_menu(&map_header, &m_entries, &mut m_sel)? {
                    if m_idx < maps.len() {
                        let chosen = &maps[m_idx];

                        let registry = ServersRegistry::load(paths)?;
                        if registry.servers.is_empty() {
                            show_empty_servers_modal(paths).await?;
                            continue;
                        }

                        let mut s_entries = Vec::new();
                        for (si, s) in registry.servers.iter().enumerate() {
                            let hotkey = if si < 9 {
                                (si + 1).to_string()
                            } else {
                                ((b'a' + (si - 9) as u8) as char).to_string()
                            };
                            s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                        }
                        s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                        let s_header = format!(" Select server to install map '{}':", chosen.name);
                        let mut s_sel = 0;
                        if let Some(s_idx) = run_menu(&s_header, &s_entries, &mut s_sel)? {
                            if s_idx < registry.servers.len() {
                                let server = &registry.servers[s_idx];
                                print_in_place_status(
                                    "DOWNLOADING MAP",
                                    &[
                                        format!("Downloading '{}'...", chosen.name),
                                        format!("Target Server: {}", server.name),
                                    ],
                                )?;
                                match craft_plugins::world::install_world_from_url(
                                    &server.path,
                                    chosen.download_url,
                                    Some(chosen.default_folder),
                                )
                                .await
                                {
                                    Ok((dest, installed_name)) => {
                                        show_modal_message(
                                            "MAP INSTALLED",
                                            &[
                                                format!(
                                                    "[OK] Successfully installed map '{}'!",
                                                    chosen.name
                                                )
                                                .green()
                                                .bold()
                                                .to_string(),
                                                format!("Server: {}", server.name),
                                                format!("Path:   {}", dest.display()),
                                            ],
                                            false,
                                        )?;

                                        // Prompt whether to set as default world - No by default
                                        let cur_default =
                                            craft_core::get_default_world(&server.path);
                                        let p_header = format!(
                                            " World '{}' has been installed into '{}'.\r\n Current default world (level-name): '{}'\r\n\r\n Set '{}' as the default world in server.properties?",
                                            installed_name, server.name, cur_default, installed_name
                                        );
                                        let p_opts = vec![
                                            MenuEntry::new("1", "No (Keep current)")
                                                .with_aliases(&["n", "no"]),
                                            MenuEntry::new("2", "Yes (Set as default)")
                                                .with_aliases(&["y", "yes"]),
                                        ];
                                        let mut p_choice = 0;
                                        if let Some(c) =
                                            run_menu(&p_header, &p_opts, &mut p_choice)?
                                        {
                                            if c == 1 {
                                                craft_core::set_default_world(
                                                    &server.path,
                                                    &installed_name,
                                                )?;
                                                show_modal_message(
                                                    "DEFAULT WORLD UPDATED",
                                                    &[
                                                        format!("[OK] Set '{}' as active default world (level-name).", installed_name)
                                                            .green()
                                                            .bold()
                                                            .to_string(),
                                                    ],
                                                    false,
                                                )?;
                                            }
                                        }
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
            }
            _ => return Ok(()),
        }
    }
}

pub async fn remotes_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Manage remote game server hosts and orchestrated deployments over SSH.\r\n{}",
            box_top(width).cyan().bold(),
            box_title("REMOTE VPS HOSTS (SSH)", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "List Configured Remote Hosts"),
            MenuEntry::new("2", "Test Remote Host Connection"),
            MenuEntry::new("3", "Add New Remote Host"),
            MenuEntry::new("4", "Remove Remote Host"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // List
                let reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message(
                        "NO REMOTE HOSTS",
                        &["No remote hosts configured.", "Add one with option [3]."],
                        false,
                    )?;
                } else {
                    let lines: Vec<String> = reg
                        .remotes
                        .iter()
                        .map(|r| format!("{:<16} {}@{}:{}", r.alias, r.user, r.host, r.port))
                        .collect();
                    show_modal_message("CONFIGURED REMOTE HOSTS", &lines, false)?;
                }
            }
            Some(1) => {
                // Test
                let reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message(
                        "NO REMOTE HOSTS",
                        &["No remote hosts configured to test."],
                        false,
                    )?;
                    continue;
                }

                let mut r_entries = Vec::new();
                for (i, r) in reg.remotes.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    r_entries.push(MenuEntry::new(
                        hotkey,
                        format!("{:<16} ({}@{})", r.alias, r.user, r.host),
                    ));
                }
                r_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let mut r_sel = 0;
                if let Some(r_idx) = run_menu(
                    " Select remote host to test connection:",
                    &r_entries,
                    &mut r_sel,
                )? {
                    if r_idx < reg.remotes.len() {
                        let remote = &reg.remotes[r_idx];
                        print_in_place_status(
                            "TESTING SSH CONNECTION",
                            &[format!(
                                "Connecting to {}@{}:{}...",
                                remote.user, remote.host, remote.port
                            )],
                        )?;

                        let session_res = craft_remote::RemoteSession::connect(remote);
                        match session_res {
                            Ok(session) => {
                                let uname = session
                                    .exec("uname -a")
                                    .map(|(_, out, _)| out.trim().to_string())
                                    .unwrap_or_default();
                                show_modal_message(
                                    "SSH CONNECTION SUCCESSFUL",
                                    &[
                                        format!(
                                            "[OK] Successfully connected to host '{}'!",
                                            remote.alias
                                        )
                                        .green()
                                        .bold()
                                        .to_string(),
                                        format!(
                                            "Remote host: {}@{}:{}",
                                            remote.user, remote.host, remote.port
                                        ),
                                        format!("System info: {}", uname),
                                    ],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message(
                                    "SSH CONNECTION FAILED",
                                    &[format!("[ERROR] {}", e)],
                                    true,
                                )?;
                            }
                        }
                    }
                }
            }
            Some(2) => {
                // Add
                let alias = match run_input_prompt(
                    "ADD REMOTE HOST (1/2)",
                    "Enter host alias (e.g. prod-vps, ovh-node):",
                    None,
                )? {
                    Some(a) if !a.trim().is_empty() => a.trim().to_string(),
                    _ => continue,
                };

                let conn = match run_input_prompt(
                    "ADD REMOTE HOST (2/2)",
                    "Enter SSH connection string (user@host or user@host:port):",
                    None,
                )? {
                    Some(c) if !c.trim().is_empty() => c.trim().to_string(),
                    _ => continue,
                };

                match parse_connection_string(&conn) {
                    Ok((user, host, port)) => {
                        let mut reg = RemotesRegistry::load(paths)?;
                        let config = RemoteHostConfig {
                            alias: alias.clone(),
                            host,
                            port,
                            user,
                            auth_type: RemoteAuthType::Key,
                            key_path: None,
                            password: None,
                            remote_dir: None,
                            os_type: None,
                        };
                        reg.add(config)?;
                        reg.save(paths)?;
                        show_modal_message(
                            "REMOTE HOST ADDED",
                            &[
                                format!("[OK] Remote host '{}' configured successfully!", alias)
                                    .green()
                                    .bold()
                                    .to_string(),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "INVALID CONNECTION STRING",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(3) => {
                // Remove
                let mut reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message(
                        "NO REMOTE HOSTS",
                        &["No remote hosts configured to remove."],
                        false,
                    )?;
                    continue;
                }

                let mut r_entries = Vec::new();
                for (i, r) in reg.remotes.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    r_entries.push(MenuEntry::new(hotkey, r.alias.clone()));
                }
                r_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let mut r_sel = 0;
                if let Some(r_idx) =
                    run_menu(" Select remote host to delete:", &r_entries, &mut r_sel)?
                {
                    if r_idx < reg.remotes.len() {
                        let alias = reg.remotes[r_idx].alias.clone();
                        reg.remove(&alias);
                        reg.save(paths)?;
                        show_modal_message(
                            "REMOTE HOST REMOVED",
                            &[format!("[OK] Removed remote host '{}'.", alias)],
                            false,
                        )?;
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn daemon_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Daemon Control");
    let mut selected = 0;

    loop {
        let is_running = DaemonClient::is_daemon_running(paths);
        let status_badge = if is_running {
            "[ONLINE]".green().bold()
        } else {
            "[OFFLINE]".yellow().bold()
        };

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Daemon Status: {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("DAEMON CONTROL", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            status_badge,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Daemon Status"),
            MenuEntry::new("2", "Start Daemon"),
            MenuEntry::new("3", "Stop Daemon"),
            MenuEntry::new("4", "Restart Daemon"),
            MenuEntry::new("0", "Back").with_aliases(&["b"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                if is_running {
                    let pid = read_pid_file(&paths.pid_file).unwrap_or(0);
                    let mut lines = vec![
                        "[ONLINE] Craft service daemon is active and managing servers."
                            .green()
                            .bold()
                            .to_string(),
                        format!("Process PID: {}", pid),
                    ];
                    if let Ok(mut client) = DaemonClient::connect(paths).await {
                        let running = client.get_running().await.unwrap_or_default();
                        lines.push(format!("Active background servers: {}", running.len()));
                        for p in running {
                            lines.push(format!("  - {}", p.display()));
                        }
                    }
                    show_modal_message("DAEMON STATUS", &lines, false)?;
                } else {
                    show_modal_message(
                        "DAEMON STATUS",
                        &[
                            "[OFFLINE] Craft service daemon is currently stopped."
                                .yellow()
                                .to_string(),
                            "Start the daemon with option [2] to enable 24/7 background servers."
                                .to_string(),
                        ],
                        false,
                    )?;
                }
            }
            Some(1) => {
                if is_running {
                    show_modal_message(
                        "ALREADY RUNNING",
                        &["Craft service daemon is already running.".to_string()],
                        false,
                    )?;
                } else {
                    print_in_place_status(
                        "STARTING DAEMON",
                        &["Launching Craft supervisor daemon in background...".to_string()],
                    )?;
                    match DaemonClient::ensure_daemon_started(paths).await {
                        Ok(_) => show_modal_message(
                            "DAEMON STARTED",
                            &["[OK] Craft service daemon is now online!"
                                .green()
                                .bold()
                                .to_string()],
                            false,
                        )?,
                        Err(e) => show_modal_message(
                            "DAEMON START FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?,
                    }
                }
            }
            Some(2) => {
                if !is_running {
                    show_modal_message(
                        "ALREADY STOPPED",
                        &["Craft service daemon is not running.".to_string()],
                        false,
                    )?;
                } else {
                    print_in_place_status(
                        "STOPPING DAEMON",
                        &["Stopping Craft service daemon...".to_string()],
                    )?;
                    if let Some(pid) = read_pid_file(&paths.pid_file) {
                        let _ = kill_process(pid, false);
                    }
                    show_modal_message(
                        "DAEMON STOPPED",
                        &["[OK] Craft service daemon has been stopped."
                            .green()
                            .bold()
                            .to_string()],
                        false,
                    )?;
                }
            }
            Some(3) => {
                print_in_place_status(
                    "RESTARTING DAEMON",
                    &["Restarting supervisor daemon...".to_string()],
                )?;
                if is_running {
                    if let Some(pid) = read_pid_file(&paths.pid_file) {
                        let _ = kill_process(pid, false);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                match DaemonClient::ensure_daemon_started(paths).await {
                    Ok(_) => show_modal_message(
                        "DAEMON RESTARTED",
                        &["[OK] Craft service daemon restarted successfully."
                            .green()
                            .bold()
                            .to_string()],
                        false,
                    )?,
                    Err(e) => {
                        show_modal_message("RESTART FAILED", &[format!("[ERROR] {}", e)], true)?
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub fn cache_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let cache = CacheManager::new(paths);
        let size = cache.get_cache_size();
        let mb = (size as f64) / (1024.0 * 1024.0);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Total Download Cache: {:.2} MB | Location: {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("CACHE & STORAGE", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            mb,
            paths.cache_dir.display(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Cache Size"),
            MenuEntry::new("2", "Purge Cache"),
            MenuEntry::new("0", "Back").with_aliases(&["b"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                show_modal_message(
                    "CACHE STORAGE",
                    &[
                        format!("Current download cache: {:.2} MB", mb),
                        format!("Cache directory:        {}", paths.cache_dir.display()),
                    ],
                    false,
                )?;
            }
            Some(1) => {
                let width = get_content_width(80);
                let conf_header = format!(
                    "{}\r\n{}\r\n{}\r\n Delete all cached jarfiles and archives ({:.2} MB)?\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("CONFIRM CACHE PURGE", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    mb,
                    box_divider(width).dimmed(),
                );
                let conf_entries = vec![
                    MenuEntry::new("1", "Purge Cache"),
                    MenuEntry::new("2", "Cancel"),
                ];
                let mut c_sel = 1;
                if let Some(0) = run_menu(&conf_header, &conf_entries, &mut c_sel)? {
                    match cache.clean_cache() {
                        Ok(cleaned) => {
                            let cl_mb = (cleaned as f64) / (1024.0 * 1024.0);
                            show_modal_message(
                                "CACHE PURGED",
                                &[
                                    format!("[OK] Cleared {:.2} MB of downloaded caches.", cl_mb)
                                        .green()
                                        .bold()
                                        .to_string(),
                                ],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message("PURGE FAILED", &[format!("[ERROR] {}", e)], true)?;
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn firewall_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Firewall");
    let mut selected = 0;

    loop {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Manage incoming host firewall rules for Minecraft servers.\r\n Supports ufw (Linux), pfctl (macOS), and netsh (Windows).\r\n{}",
            box_top(width).cyan().bold(),
            box_title("FIREWALL RULES", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Allow IP & Port for Registered Server"),
            MenuEntry::new("2", "Allow Custom IP & Port"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let reg = ServersRegistry::load(paths)?;
                if reg.servers.is_empty() {
                    show_modal_message(
                        "NO SERVERS",
                        &["No registered servers found to configure.".to_string()],
                        true,
                    )?;
                    continue;
                }
                let mut s_entries = Vec::new();
                for (i, s) in reg.servers.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    let port = s.port.unwrap_or(25565);
                    s_entries.push(MenuEntry::new(
                        hotkey,
                        format!("{:<20} Port: {:<6} Software: {}", s.name, port, s.software),
                    ));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let mut s_sel = 0;
                let server_idx =
                    match run_menu(" Select Server for Firewall Rule:", &s_entries, &mut s_sel)? {
                        Some(idx) if idx < reg.servers.len() => idx,
                        _ => continue,
                    };

                let s = &reg.servers[server_idx];
                let is_bedrock =
                    s.software.contains("bedrock") || s.software.contains("pocketmine");
                let port = s.port.unwrap_or(if is_bedrock { 19132 } else { 25565 });
                let ip_prompt = run_input_prompt(
                    "ALLOWED IP ADDRESS",
                    "Enter remote IP allowed to connect (e.g. 192.168.1.50 or 0.0.0.0/0 for any):",
                    Some("0.0.0.0/0"),
                )?;
                let ip = match ip_prompt {
                    Some(i) if !i.trim().is_empty() => i.trim().to_string(),
                    _ => continue,
                };

                let proto_name = if is_bedrock { "UDP" } else { "TCP" };
                let action_res = exec_console_action(|| async {
                    println!(
                        "Applying firewall rule for port {} ({})...",
                        port, proto_name
                    );
                    #[cfg(unix)]
                    println!("Superuser privileges (sudo) may be requested.");
                    #[cfg(windows)]
                    println!("Administrator privileges (UAC) may be requested.");
                    println!();
                    craft_net::allow_ip_port(&ip, port, is_bedrock)
                })
                .await;

                match action_res {
                    Ok(_) => {
                        show_modal_message(
                            "FIREWALL RULE ADDED",
                            &[format!(
                                "[OK] Allowed incoming connections from '{}' on port {}.",
                                ip, port
                            )
                            .green()
                            .bold()
                            .to_string()],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("FIREWALL ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(1) => {
                let ip_str = match run_input_prompt(
                    "ALLOWED IP",
                    "Enter IP to allow (or 0.0.0.0/0 for any):",
                    Some("0.0.0.0/0"),
                )? {
                    Some(i) if !i.trim().is_empty() => i.trim().to_string(),
                    _ => continue,
                };
                let port_str = match run_input_prompt(
                    "PORT",
                    "Enter port number to allow (e.g. 25565):",
                    Some("25565"),
                )? {
                    Some(p) if !p.trim().is_empty() => p.trim().to_string(),
                    _ => continue,
                };
                let port: u16 = match port_str.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        show_modal_message(
                            "INVALID PORT",
                            &["Port must be a number between 1 and 65535.".to_string()],
                            true,
                        )?;
                        continue;
                    }
                };
                let proto_sel = run_menu(
                    " Select Protocol:",
                    &[
                        MenuEntry::new("1", "TCP (Java)"),
                        MenuEntry::new("2", "UDP (Bedrock)"),
                        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                    ],
                    &mut 0,
                )?;
                let is_udp = match proto_sel {
                    Some(0) => false,
                    Some(1) => true,
                    _ => continue,
                };
                let proto_name = if is_udp { "UDP" } else { "TCP" };
                let action_res = exec_console_action(|| async {
                    println!(
                        "Applying firewall rule for {}:{} ({})...",
                        ip_str, port, proto_name
                    );
                    #[cfg(unix)]
                    println!("Superuser privileges (sudo) may be requested.");
                    #[cfg(windows)]
                    println!("Administrator privileges (UAC) may be requested.");
                    println!();
                    craft_net::allow_ip_port(&ip_str, port, is_udp)
                })
                .await;

                match action_res {
                    Ok(_) => {
                        show_modal_message(
                            "FIREWALL RULE ADDED",
                            &[format!(
                                "[OK] Successfully allowed {}:{} ({})!",
                                ip_str, port, proto_name
                            )
                            .green()
                            .bold()
                            .to_string()],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("FIREWALL ERROR", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            _ => break,
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub async fn loopback_menu() -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Loopback");
    let mut selected = 0;

    loop {
        let status = craft_net::is_bedrock_loopback_enabled().unwrap_or(false);
        let status_badge = if status {
            "[ENABLED]".green().bold().to_string()
        } else {
            "[DISABLED]".yellow().bold().to_string()
        };
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Windows UWP Bedrock Loopback Status: {}\r\n Enables connecting to a local Bedrock server running on the same PC.\r\n (Requires Windows CheckNetIsolation.exe)\r\n{}",
            box_top(width).cyan().bold(),
            box_title("BEDROCK LOOPBACK EXEMPTION", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            status_badge,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new(
                "1",
                if status {
                    "Re-enable / Refresh Loopback Exemption"
                } else {
                    "Enable Loopback Exemption"
                },
            ),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => match craft_net::enable_bedrock_loopback() {
                Ok(_) => {
                    show_modal_message(
                        "LOOPBACK EXEMPTION APPLIED",
                        &["[OK] Windows UWP Loopback exemption enabled successfully!"
                            .green()
                            .bold()
                            .to_string()],
                        false,
                    )?;
                }
                Err(e) => {
                    show_modal_message(
                        "LOOPBACK ERROR",
                        &[format!("[ERROR] Failed to enable loopback: {}", e)],
                        true,
                    )?;
                }
            },
            _ => break,
        }
    }
    Ok(())
}

pub async fn tools_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Tools");
    let mut selected = 0;

    loop {
        let cache = CacheManager::new(paths);
        let size = cache.get_cache_size();
        let mb = (size as f64) / (1024.0 * 1024.0);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Network diagnostics, host firewall, daemon, and system utilities.\r\n{}",
            box_top(width).cyan().bold(),
            box_title("DIAGNOSTIC & SYSTEM TOOLS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let purge_label = format!("Purge Download Cache ({:.2} MB)", mb);

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum ToolItemAction {
            Ping,
            TickProfile,
            Daemon,
            Firewall,
            ScriptsHooks,
            CanaryFleet,
            LogForensics,
            WorkloadForecasting,
            ModpackCI,
            ZeroTrustMesh,
            RaftConsensus,
            ResourceQuotas,
            DistributedTracing,
            AnvilStorage,
            NumaDpdk,
            LiveMigration,
            #[cfg(target_os = "windows")]
            Loopback,
            PurgeCache,
            Back,
        }

        let mut entries = Vec::new();
        let mut actions = Vec::new();
        let mut num = 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Server Network Ping").with_aliases(&["p", "ping"]),
        );
        actions.push(ToolItemAction::Ping);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Tick Profiling & Network Telemetry")
                .with_aliases(&["t", "profile", "telemetry"]),
        );
        actions.push(ToolItemAction::TickProfile);
        num += 1;

        entries
            .push(MenuEntry::new(num.to_string(), "Daemon Control").with_aliases(&["d", "daemon"]));
        actions.push(ToolItemAction::Daemon);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Firewall Manager (Port/IP Rules)")
                .with_aliases(&["f", "firewall"]),
        );
        actions.push(ToolItemAction::Firewall);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Lua Automation & Lifecycle Hooks")
                .with_aliases(&["s", "script", "hooks"]),
        );
        actions.push(ToolItemAction::ScriptsHooks);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Canary Rollouts & Fleet Healing")
                .with_aliases(&["r", "rollout", "canary", "fleet"]),
        );
        actions.push(ToolItemAction::CanaryFleet);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Log Search & Incident Forensics")
                .with_aliases(&["l", "logs", "forensics"]),
        );
        actions.push(ToolItemAction::LogForensics);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Workload Forecasting & Cost Optimizer")
                .with_aliases(&["w", "forecast", "costs"]),
        );
        actions.push(ToolItemAction::WorkloadForecasting);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Modpack CI/CD & Fast Client Synchronizer")
                .with_aliases(&["m", "modpack", "ci", "sync"]),
        );
        actions.push(ToolItemAction::ModpackCI);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Zero-Trust Mesh & Packet Filtering")
                .with_aliases(&["z", "sdn", "mesh", "wireguard", "filter"]),
        );
        actions.push(ToolItemAction::ZeroTrustMesh);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Raft Consensus & Cluster Arbitration")
                .with_aliases(&["raft", "consensus", "cluster", "leader"]),
        );
        actions.push(ToolItemAction::RaftConsensus);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Resource Quotas & Cgroups v2 Scheduling")
                .with_aliases(&["quota", "cgroup", "limits", "fairshare"]),
        );
        actions.push(ToolItemAction::ResourceQuotas);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Distributed Tracing & OpenTelemetry (OTel)")
                .with_aliases(&["trace", "tracing", "otel"]),
        );
        actions.push(ToolItemAction::DistributedTracing);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Hardware-Accelerated Anvil Storage & io_uring (MCA)")
                .with_aliases(&["anvil", "chunk", "mca", "uring"]),
        );
        actions.push(ToolItemAction::AnvilStorage);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Kernel-Bypassed DPDK & NUMA Memory Pinning")
                .with_aliases(&["numa", "dpdk", "pinning", "isolcpus"]),
        );
        actions.push(ToolItemAction::NumaDpdk);
        num += 1;

        entries.push(
            MenuEntry::new(num.to_string(), "Zero-Downtime Live Migration & Anycast Steering")
                .with_aliases(&["migrate", "live", "anycast", "bgp"]),
        );
        actions.push(ToolItemAction::LiveMigration);
        num += 1;

        #[cfg(target_os = "windows")]
        {
            entries.push(
                MenuEntry::new(num.to_string(), "Windows Bedrock Loopback Exemption")
                    .with_aliases(&["l", "loopback"]),
            );
            actions.push(ToolItemAction::Loopback);
            num += 1;
        }

        entries.push(MenuEntry::new(num.to_string(), purge_label).with_aliases(&["c", "cache"]));
        actions.push(ToolItemAction::PurgeCache);

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));
        actions.push(ToolItemAction::Back);

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < actions.len() => match actions[idx] {
                ToolItemAction::Ping => {
                    ping_menu().await?;
                }
                ToolItemAction::TickProfile => {
                    tick_profile_tui(paths).await?;
                }
                ToolItemAction::Daemon => {
                    daemon_menu(paths).await?;
                }
                ToolItemAction::Firewall => {
                    firewall_menu(paths).await?;
                }
                ToolItemAction::ScriptsHooks => {
                    super::scripts_tui::scripts_hooks_menu(paths).await?;
                }
                ToolItemAction::CanaryFleet => {
                    canary_fleet_tui(paths).await?;
                }
                ToolItemAction::LogForensics => {
                    log_forensics_tui(paths).await?;
                }
                ToolItemAction::WorkloadForecasting => {
                    workload_forecasting_tui(paths).await?;
                }
                ToolItemAction::ModpackCI => {
                    modpack_ci_tui(paths).await?;
                }
                ToolItemAction::ZeroTrustMesh => {
                    zero_trust_mesh_tui(paths).await?;
                }
                ToolItemAction::RaftConsensus => {
                    raft_consensus_tui(paths).await?;
                }
                ToolItemAction::ResourceQuotas => {
                    resource_quotas_tui(paths).await?;
                }
                ToolItemAction::DistributedTracing => {
                    distributed_tracing_tui(paths).await?;
                }
                ToolItemAction::AnvilStorage => {
                    anvil_storage_tui(paths).await?;
                }
                ToolItemAction::NumaDpdk => {
                    numa_dpdk_tui(paths).await?;
                }
                ToolItemAction::LiveMigration => {
                    live_migration_tui(paths).await?;
                }
                #[cfg(target_os = "windows")]
                ToolItemAction::Loopback => {
                    loopback_menu().await?;
                }
                ToolItemAction::PurgeCache => {
                    let width = get_content_width(80);
                    let conf_header = format!(
                        "{}\r\n{}\r\n{}\r\n Delete all cached jarfiles and archives ({:.2} MB)?\r\n{}",
                        box_top(width).cyan().bold(),
                        box_title("CONFIRM CACHE PURGE", width, false).cyan().bold(),
                        box_divider(width).cyan().bold(),
                        mb,
                        box_divider(width).dimmed(),
                    );
                    let conf_entries = vec![
                        MenuEntry::new("1", "Purge Cache"),
                        MenuEntry::new("2", "Cancel"),
                    ];
                    let mut c_sel = 1;
                    if let Some(0) = run_menu(&conf_header, &conf_entries, &mut c_sel)? {
                        match cache.clean_cache() {
                            Ok(cleaned) => {
                                let cl_mb = (cleaned as f64) / (1024.0 * 1024.0);
                                show_modal_message(
                                    "CACHE PURGED",
                                    &[format!(
                                        "[OK] Cleared {:.2} MB of downloaded caches.",
                                        cl_mb
                                    )
                                    .green()
                                    .bold()
                                    .to_string()],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message(
                                    "PURGE FAILED",
                                    &[format!("[ERROR] {}", e)],
                                    true,
                                )?;
                            }
                        }
                    }
                }
                ToolItemAction::Back => return Ok(()),
            },
            _ => return Ok(()),
        }
    }
}

pub async fn tick_profile_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Tick Profile");

    let registry = match ServersRegistry::load(paths) {
        Ok(r) => r,
        Err(e) => {
            show_modal_message(
                "ERROR",
                &[format!("[ERROR] Failed to load server registry: {}", e)],
                true,
            )?;
            return Ok(());
        }
    };

    let server_name = if registry.servers.is_empty() {
        match run_input_prompt(
            "TICK PROFILER",
            "Enter server name to inspect:",
            None,
        )? {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => return Ok(()),
        }
    } else {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a server to inspect tick profiling & telemetry:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("SELECT SERVER FOR PROFILING", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries: Vec<MenuEntry> = registry
            .servers
            .iter()
            .enumerate()
            .map(|(i, s)| MenuEntry::new((i + 1).to_string(), &s.name))
            .collect();
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        let mut sel = 0;
        match run_menu(&header, &entries, &mut sel)? {
            Some(idx) if idx < registry.servers.len() => registry.servers[idx].name.clone(),
            _ => return Ok(()),
        }
    };

    print_in_place_status(
        "COLLECTING TELEMETRY",
        &[format!("Querying tick profiling data for '{}'...", server_name)],
    )?;

    let mut tick_info = None;
    let mut packet_info = None;
    let mut hist_info = None;

    if let Ok(mut client) = DaemonClient::connect(paths).await {
        if let Ok((summary, sparkline)) = client.get_tick_profile(server_name.clone()).await {
            tick_info = Some((summary, sparkline));
        }
        if let Ok(stats) = client.get_packet_stats(server_name.clone()).await {
            packet_info = Some(stats);
        }
        if let Ok((hist, lines)) = client.get_latency_histogram(server_name.clone()).await {
            hist_info = Some((hist, lines));
        }
    }

    let mut lines = Vec::new();
    lines.push(format!("Server: {}", server_name.clone().cyan().bold()));
    lines.push("".to_string());

    if let Some((ref summary, ref sparkline)) = tick_info {
        let grade = match summary.health {
            craft_net::TickHealthGrade::Pristine => "[PRISTINE]".green().bold(),
            craft_net::TickHealthGrade::Stable => "[STABLE]".cyan().bold(),
            craft_net::TickHealthGrade::Degraded => "[DEGRADED]".yellow().bold(),
            craft_net::TickHealthGrade::Overloaded => "[OVERLOADED]".red().bold(),
        };
        lines.push(format!("Tick Health:     {}", grade));
        lines.push(format!("Current/Avg TPS: {:.1} / {:.1}", summary.current_tps, summary.avg_tps));
        lines.push(format!("Current MSPT:    {:.2} ms (Jitter: {:.2} ms)", summary.current_mspt, summary.jitter_ms));
        lines.push(format!("Percentiles:     P50: {:.1}ms | P90: {:.1}ms | P99: {:.1}ms", summary.mspt_p50, summary.mspt_p90, summary.mspt_p99));
        lines.push(format!("Latency Trail:   [{}]", sparkline));
    } else {
        lines.push("[WARN] Daemon offline or no tick data sampled yet.".yellow().to_string());
    }

    lines.push("".to_string());
    if let Some(ref stats) = packet_info {
        lines.push(format!(
            "Netty Ingress:   {} PPS ({:.1} KB/s)",
            stats.rx_pps,
            stats.rx_bytes_sec as f64 / 1024.0
        ));
        lines.push(format!(
            "Netty Egress:    {} PPS ({:.1} KB/s)",
            stats.tx_pps,
            stats.tx_bytes_sec as f64 / 1024.0
        ));
        let burst_status = if stats.flood_warning || stats.burst_detected {
            format!("[WARN] Rate {} pps (Elevated)", stats.rx_pps).yellow().bold().to_string()
        } else {
            format!("[OK] Rate {} pps (Nominal)", stats.rx_pps).green().bold().to_string()
        };
        lines.push(format!("Traffic Health:  {}", burst_status));
    }

    if let Some((ref hist, _)) = hist_info {
        lines.push("".to_string());
        lines.push(format!(
            "Micro-Histogram: {} samples recorded",
            hist.total_count
        ));
        let p50 = hist.quantile_us(0.50);
        let p99 = hist.quantile_us(0.99);
        lines.push(format!(
            "Distribution:    P50: {:.1}us ({:.2}ms) | P99: {:.1}us ({:.2}ms)",
            p50, p50 / 1000.0, p99, p99 / 1000.0
        ));
    }

    show_modal_message("TICK PROFILING & TELEMETRY", &lines, false)?;
    Ok(())
}

pub async fn canary_fleet_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Fleet Rollouts");

    let clusters = craft_core::ClustersRegistry::load(paths).unwrap_or_default();
    let rollouts = craft_core::RolloutRegistry::load(paths).unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(format!("Configured Clusters: {}", clusters.clusters.len()));
    lines.push(format!("Active Rollouts:     {}", rollouts.active_rollouts.len()));
    lines.push("".to_string());

    if rollouts.active_rollouts.is_empty() {
        lines.push("[OK] All cluster fleets operating normally (no active rollouts).".green().to_string());
    } else {
        lines.push("[ACTIVE ROLLOUTS]".yellow().bold().to_string());
        for (cluster, id) in &rollouts.active_rollouts {
            if let Some(record) = rollouts.get_rollout(id) {
                lines.push(format!(
                    " - Cluster '{}': Target v{} ({}) - Stage: {}",
                    cluster, record.plan.target_version, record.plan.strategy, record.stage.name()
                ));
            }
        }
    }

    if let Ok(mut client) = DaemonClient::connect(paths).await {
        for cluster in &clusters.clusters {
            if let Ok(fleet) = client.get_fleet_health(cluster.name.clone()).await {
                lines.push("".to_string());
                let health_str = if fleet.overall_healthy { "[HEALTHY]".green() } else { "[DEGRADED]".red() };
                lines.push(format!("Cluster '{}' Fleet Health: {}", cluster.name, health_str));
                for (node_id, health) in &fleet.node_statuses {
                    lines.push(format!(
                        "   * Node '{}': {} | {:.1} TPS | {:.1}ms MSPT",
                        node_id, health.status, health.current_tps, health.current_mspt
                    ));
                }
            }
        }
    }

    show_modal_message("CANARY ROLLOUTS & FLEET HEALING", &lines, false)?;
    Ok(())
}

pub async fn log_forensics_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Log Forensics");

    let query_input = match run_input_prompt(
        "LOG SEARCH & INCIDENT FORENSICS",
        "Enter search query or error keyword (e.g. exception, error, crash):",
        Some("exception"),
    )? {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => return Ok(()),
    };

    let query = craft_core::LogQuery {
        query_pattern: query_input.clone(),
        limit: 25,
        ..Default::default()
    };

    let result = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client.search_logs(query).await.ok()
        } else {
            None
        }
    } else {
        let service = craft_daemon::LogIngestionService::new(paths);
        service.search(&query).await.ok()
    };

    let mut lines = Vec::new();
    lines.push(format!("Query Pattern: \"{}\"", query_input));
    lines.push("".to_string());

    if let Some(res) = result {
        lines.push(format!(
            "Matches Found: {} | Lines Scanned: {} | Duration: {:.2}ms",
            res.total_matches,
            res.scanned_lines,
            res.duration_micros as f64 / 1000.0
        ));
        lines.push("".to_string());

        if res.matches.is_empty() {
            lines.push("[OK] No matching log entries found.".to_string());
        } else {
            lines.push("[MATCHING LOG ENTRIES]".cyan().bold().to_string());
            for m in res.matches.iter().take(15) {
                let lvl_str = match m.level {
                    craft_core::LogLevel::Fatal | craft_core::LogLevel::Error => format!("[{}]", m.level).red(),
                    craft_core::LogLevel::Warn => format!("[{}]", m.level).yellow(),
                    _ => format!("[{}]", m.level).green(),
                };
                lines.push(format!(
                    "{} {} [{}] {}",
                    m.timestamp.format("%H:%M:%S"),
                    lvl_str,
                    m.server_name,
                    if m.message.len() > 60 {
                        format!("{}...", &m.message[..57])
                    } else {
                        m.message.clone()
                    }
                ));
            }
        }
    } else {
        lines.push("[WARN] Failed to query daemon log service.".yellow().to_string());
    }

    // Also display recent incident post-mortems if any
    let service = craft_daemon::LogIngestionService::new(paths);
    if let Ok(incidents) = service.list_incidents(None) {
        if !incidents.is_empty() {
            lines.push("".to_string());
            lines.push("[RECENT CRASH INCIDENT POST-MORTEMS]".red().bold().to_string());
            for inc in incidents.iter().take(3) {
                let auth_str = if inc.authenticity_valid { "[AUTHENTIC]".green() } else { "[TAMPERED]".red() };
                lines.push(format!(
                    " * {} | {} | {} | {}",
                    inc.incident_id,
                    inc.server_name,
                    inc.culprit_exception,
                    auth_str
                ));
                if let Some(ref plug) = inc.suspected_plugin {
                    lines.push(format!("   Culprit: [PLUGIN: {}]", plug));
                }
            }
        }
    }

    show_modal_message("LOG FORENSICS & INCIDENT AUDIT", &lines, false)?;
    Ok(())
}

pub async fn workload_forecasting_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Workload Forecasting & Cost Optimizer");

    let servers = ServersRegistry::load(paths)?;
    if servers.servers.is_empty() {
        let _ = show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let server_name = if servers.servers.len() == 1 {
        servers.servers[0].name.clone()
    } else {
        let mut server_sel = 0;
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select server to forecast workload & cost metrics:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("SELECT SERVER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, s) in servers.servers.iter().enumerate() {
            entries.push(MenuEntry::new((i + 1).to_string(), &s.name));
        }
        entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut server_sel)? {
            Some(idx) if idx < servers.servers.len() => servers.servers[idx].name.clone(),
            _ => return Ok(()),
        }
    };

    let forecast = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client.get_workload_forecast(server_name.clone(), 24).await.ok()
        } else {
            None
        }
    } else {
        None
    };

    let forecast = forecast.unwrap_or_else(|| {
        let sample_file = paths.workload_dir.join(format!("{}.json", server_name));
        let samples = craft_core::load_workload_samples(&sample_file).unwrap_or_default();
        let forecaster = if samples.is_empty() {
            craft_core::SeasonalForecaster::new()
        } else {
            craft_core::SeasonalForecaster::fit(&samples)
        };
        forecaster.forecast(&server_name, chrono::Utc::now(), 24)
    });

    let cost_report = craft_core::CostOptimizationModel::compute_savings(
        &server_name,
        720.0,
        210.0,
        4.0,
        8.0,
        craft_core::DEFAULT_VCPU_HOURLY_COST,
        craft_core::DEFAULT_RAM_GIB_HOURLY_COST,
    );

    let sparkline = craft_core::generate_forecast_sparkline(&forecast.points);

    let mut lines = Vec::new();
    lines.push(format!("Server Target:      {}", server_name.yellow().bold()));
    lines.push(format!("Forecast Horizon:   24 Hours (Next Day)"));
    lines.push(format!("Peak Projection:    {:.1} players", forecast.peak_players));
    if let (Some(qs), Some(qe)) = (forecast.quiet_window_start, forecast.quiet_window_end) {
        lines.push(format!("Quiet Window:       {:02}:00 - {:02}:00 UTC", qs, qe));
    }
    lines.push("".to_string());
    if let Some(mins) = forecast.next_surge_predicted_in_mins {
        lines.push(format!("[WARN] SURGE ALERT: {:.1} players in {} minutes!", forecast.peak_players, mins));
    } else {
        lines.push("[STATUS] Workload profile steady (no imminent surge)".to_string());
    }
    lines.push(format!("Workload Sparkline: {}", sparkline));
    lines.push("".to_string());
    lines.push("[COST OPTIMIZATION LEDGER]".cyan().bold().to_string());
    lines.push(format!("vCPU Hours Saved:   {:.1} core-hrs", cost_report.vcpu_hours_saved));
    lines.push(format!("RAM Hours Saved:    {:.1} GiB-hrs", cost_report.ram_gib_hours_saved));
    lines.push(format!("Tracked Window:     {:.0} hours ({:.1}% hibernated)", cost_report.total_tracked_hours, cost_report.efficiency_score));
    lines.push(format!("Net Dollar Savings: ${:.2}", cost_report.realized_savings_usd));
    lines.push(format!("Projected Savings:  ${:.2}/month", cost_report.projected_monthly_savings_usd));

    show_modal_message("WORKLOAD FORECASTING & COST OPTIMIZER", &lines, false)?;
    Ok(())
}

pub async fn modpack_ci_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Modpack CI/CD & Fast Client Synchronizer");

    let registry = ModpackRegistry::load(paths).unwrap_or_default();
    let mut lines = Vec::new();

    if registry.modpacks.is_empty() {
        lines.push("[NO MODPACKS REGISTERED]".yellow().bold().to_string());
        lines.push("No modpacks have been built or indexed on this system yet.".to_string());
        lines.push("".to_string());
        lines.push("To package your first modpack with automated CI:".to_string());
        lines.push("  craft modpack build <server-dir> --name mypack --version 1.0.0".green().to_string());
        lines.push("".to_string());
        lines.push("To generate sub-megabyte binary delta patches:".to_string());
        lines.push("  craft modpack delta <v1.tar.zst> <v2.tar.zst>".green().to_string());
        lines.push("".to_string());
        lines.push("To synchronize client files:".to_string());
        lines.push("  craft modpack sync mypack".green().to_string());
    } else {
        lines.push(format!("Total Registered Modpacks: {}", registry.modpacks.len().to_string().cyan()));
        lines.push("".to_string());

        for (name, record) in &registry.modpacks {
            lines.push(format!("[MODPACK: {}]", name).yellow().bold().to_string());
            lines.push(format!("  Releases Indexed: {}", record.versions.len()));
            lines.push(format!("  Delta Patches:    {}", record.deltas.len()));

            if let Some(latest) = record.versions.last() {
                lines.push(format!("  Latest Version:   {} ({})", latest.version.green(), latest.loader.purple()));
                lines.push(format!("  Minecraft:        {}", latest.minecraft_version));
                lines.push(format!("  Components:       {} mods/resources", latest.components.len()));
                if let Some(ref sh) = latest.server_archive_hash {
                    let short_sh: String = sh.chars().take(16).collect();
                    lines.push(format!("  Server SHA-256:   {}...", short_sh));
                }
                if let Some(ref ch) = latest.client_archive_hash {
                    let short_ch: String = ch.chars().take(16).collect();
                    lines.push(format!("  Client SHA-256:   {}...", short_ch));
                }
            }

            if !record.deltas.is_empty() {
                lines.push("  Recent Binary Delta Patches:".dimmed().to_string());
                for d in record.deltas.iter().take(3) {
                    lines.push(format!(
                        "   * {} -> {} | Delta: {} | Reduction: {:.1}%",
                        d.source_version.cyan(),
                        d.target_version.green(),
                        format_size(d.delta_size),
                        d.reduction_percent
                    ));
                }
            }
            lines.push("".to_string());
        }
    }

    show_modal_message("MODPACK CI/CD & FAST CLIENT SYNCHRONIZER", &lines, false)?;
    Ok(())
}

pub async fn zero_trust_mesh_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Zero-Trust SDN Mesh & Packet Filtering");

    let registry = craft_core::SdnRegistry::load(paths)?;
    let mut lines = Vec::new();

    lines.push(format!("[SDN OVERLAY MESH: {}]", registry.mesh.mesh_name).cyan().bold().to_string());
    lines.push(format!("  Overlay CIDR:    {}", registry.mesh.overlay_cidr.yellow()));
    lines.push(format!("  Local Node:      {} ({})", registry.mesh.local_node.name.white().bold(), registry.mesh.local_node.node_id.dimmed()));
    lines.push(format!("  Local Zone:      {:?}", registry.mesh.local_node.zone).magenta().to_string());
    lines.push(format!("  Tunnel Endpoint: {}:{}", registry.mesh.local_node.tunnel_ip.green(), registry.mesh.local_node.listen_port));
    lines.push(format!("  Active Peers:    {}", registry.mesh.peers.len().to_string().bold()));
    lines.push(format!("  Policy:          {} ({} rules, default: {:?})", registry.mesh.policy.name.yellow(), registry.mesh.policy.rules.len(), registry.mesh.policy.default_action));

    let mtls_str = if registry.mesh.mtls_enabled {
        "[OK] Enabled & Enforced".green().to_string()
    } else {
        "[DISABLED]".yellow().to_string()
    };
    lines.push(format!("  mTLS Security:   {}", mtls_str));
    lines.push("".to_string());

    if registry.mesh.peers.is_empty() {
        lines.push("[INFO] No peers configured. Peers establish dynamic overlays.".dimmed().to_string());
    } else {
        lines.push("[REGISTERED WIREGUARD PEERS]".cyan().bold().to_string());
        for peer in registry.mesh.peers.iter().take(5) {
            lines.push(format!(
                " * {} ({}) | Zone: {:?} | Tunnel: {} | Allowed: {}",
                peer.name.white().bold(),
                peer.node_id.dimmed(),
                peer.zone,
                peer.tunnel_ip.green(),
                peer.allowed_ips.join(", ")
            ));
        }
    }

    lines.push("".to_string());
    lines.push("[ACTIVE MICROSEGMENTATION RULES]".cyan().bold().to_string());
    for rule in registry.mesh.policy.rules.iter().take(4) {
        let ports_str = if rule.ports.is_empty() {
            "any".to_string()
        } else {
            rule.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        };
        lines.push(format!(
            " * {:?} -> {:?} : {} (ports: {}) => {:?}",
            rule.source_zone,
            rule.target_zone,
            rule.protocol,
            ports_str,
            rule.action
        ));
    }

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push("  craft sdn status       View mesh topology and node status".green().to_string());
    lines.push("  craft sdn up           Provision and activate WireGuard interface".green().to_string());
    lines.push("  craft sdn policy       Inspect and apply eBPF/nftables filter policy".green().to_string());
    lines.push("  craft sdn audit        Run zero-trust inter-server security matrix audit".green().to_string());
    lines.push("  craft sdn rotate-keys  Trigger zero-downtime key & certificate rotation".green().to_string());

    show_modal_message("ZERO-TRUST INTER-SERVER SDN MESH", &lines, false)?;
    Ok(())
}

pub async fn raft_consensus_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Raft Consensus & Cluster Arbitration");

    let status = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.get_raft_status().await.unwrap_or_else(|_| {
            craft_daemon::RaftConsensusService::get_status(paths).unwrap_or(
                craft_daemon::RaftStatusSummary {
                    node_id: "local-node".to_string(),
                    role: craft_core::RaftRole::Follower,
                    current_term: 0,
                    leader_id: None,
                    commit_index: 0,
                    last_applied: 0,
                    log_entries_count: 0,
                    active_locks_count: 0,
                    cluster_nodes: Vec::new(),
                    is_quorum_intact: false,
                    edge_tie_breaker: None,
                    group_id: 0,
                    joint_consensus: None,
                },
            )
        })
    } else {
        craft_daemon::RaftConsensusService::get_status(paths)?
    };

    let (multi_reg, multi_statuses) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        if let Ok((reg, statuses, _)) = client.get_multiraft_status(None).await {
            (Some(reg), Some(statuses))
        } else {
            (None, None)
        }
    } else if let Ok((reg, statuses, _)) = craft_daemon::MultiRaftService::new(paths.clone()).get_status(None) {
        (Some(reg), Some(statuses))
    } else {
        (None, None)
    };

    let mut lines = Vec::new();
    lines.push(
        format!("[RAFT CONSENSUS ENGINE: {}]", status.node_id)
            .cyan()
            .bold()
            .to_string(),
    );
    lines.push(
        format!("  Local Role:       [{}]", status.role)
            .yellow()
            .bold()
            .to_string(),
    );
    lines.push(format!("  Current Term:     {}", status.current_term));
    lines.push(format!(
        "  Active Leader:    {}",
        status
            .leader_id
            .as_deref()
            .unwrap_or("None (Pending Election)")
            .white()
            .bold()
    ));
    lines.push(format!("  Commit Index:     {}", status.commit_index));
    lines.push(format!("  Last Applied:     {}", status.last_applied));
    lines.push(format!("  WAL Entries:      {}", status.log_entries_count));
    lines.push(format!("  Active Locks:     {}", status.active_locks_count));

    let quorum_str = if status.is_quorum_intact {
        "[OK] Intact (Quorum Formed)".green().to_string()
    } else {
        "[WARN] Degraded / Sub-Quorum Partition"
            .yellow()
            .bold()
            .to_string()
    };
    lines.push(format!("  Quorum Status:    {}", quorum_str));
    lines.push(format!(
        "  Edge Tie-Breaker: {}",
        status
            .edge_tie_breaker
            .as_deref()
            .unwrap_or("None")
            .dimmed()
    ));
    lines.push("".to_string());

    if let Some(ref reg) = multi_reg {
        lines.push(
            format!("[MULTI-RAFT PARTITIONS: {}]", reg.partitions.len())
                .cyan()
                .bold()
                .to_string(),
        );
        for p in reg.partitions.iter().take(4) {
            let p_lead = multi_statuses
                .as_ref()
                .and_then(|s| s.get(&p.group_id))
                .and_then(|st| st.leader_id.as_deref())
                .or(p.leader_node_id.as_deref())
                .unwrap_or("None");
            lines.push(format!(
                " * Group {}: {} | Range: [{}..{}] | Leader: {}",
                p.group_id,
                p.name.white().bold(),
                p.key_range_start,
                p.key_range_end,
                p_lead
            ));
        }
        lines.push("".to_string());
    }

    if status.cluster_nodes.is_empty() {
        lines.push(
            "[INFO] Standalone local node. Add peers to form a replicated cluster."
                .dimmed()
                .to_string(),
        );
    } else {
        lines.push("[RAFT CLUSTER MEMBERSHIP]".cyan().bold().to_string());
        for node in status.cluster_nodes.iter().take(5) {
            lines.push(format!(
                " * {} ({}:{}) | Voting: {} | Priority: {}",
                node.id.white().bold(),
                node.address,
                node.raft_port,
                if node.voting_member { "Yes" } else { "No" },
                node.priority
            ));
        }
    }

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push(
        "  craft raft status         View consensus state, partitions, and cluster nodes"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft reconfigure    Online joint consensus membership transition"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft compact        WAL log compaction and snapshot generation"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft partition      Multi-Raft key routing and group management"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft propose        Propose a replicated state mutation to leader"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft lock / unlock  Linearizable distributed locking"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft raft logs           Inspect append-only Write-Ahead Log (WAL)"
            .green()
            .to_string(),
    );

    show_modal_message("RAFT CONSENSUS & CLUSTER ARBITRATION", &lines, false)?;
    Ok(())
}

pub async fn resource_quotas_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Resource Quotas");

    let usage = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .list_quota_usage(None)
            .await
            .unwrap_or_else(|_| craft_daemon::QuotaService::list_quota_usage(paths, None).unwrap_or_default())
    } else {
        craft_daemon::QuotaService::list_quota_usage(paths, None)?
    };

    let driver = craft_core::CgroupV2Driver::new(paths);
    let mut lines = Vec::new();

    lines.push(format!(
        "Host Cgroup Root:   {}",
        driver.cgroup_root().display().to_string().white().bold()
    ));
    lines.push(format!(
        "Execution Driver:   {}",
        if driver.is_mock() {
            "[MOCK / USERSPACE EMULATION]".yellow().to_string()
        } else {
            "[KERNEL CGROUPS V2 NATIVE]".green().bold().to_string()
        }
    ));
    lines.push(format!(
        "Active Cgroups:     {}",
        driver.list_active_cgroups().len().to_string().cyan().bold()
    ));
    lines.push(format!(
        "Managed Servers:    {}",
        usage.len().to_string().white().bold()
    ));
    lines.push("".to_string());

    if usage.is_empty() {
        lines.push("[INFO] No server resource quotas configured yet.".dimmed().to_string());
        lines.push("Run 'craft quota set <server> --cpu 150 --memory 2048' to enforce quotas.".dimmed().to_string());
    } else {
        lines.push("[SERVER RESOURCE ALLOCATIONS & UTILIZATION]".cyan().bold().to_string());
        for item in usage.iter().take(6) {
            let cpu_str = match item.limits.cpu_max_quota {
                Some(q) => format!("{}%", q),
                None => "unlimited".to_string(),
            };
            let mem_str = match item.limits.memory_max_bytes {
                Some(m) => craft_core::format_size(m),
                None => "unlimited".to_string(),
            };
            let cur_mem = if item.stats.memory_current_bytes > 0 {
                craft_core::format_size(item.stats.memory_current_bytes)
            } else {
                "0 B".to_string()
            };
            let status_badge = match item.health_indicator.as_str() {
                "[PRISTINE]" => "[PRISTINE]".green().to_string(),
                "[NORMAL]" => "[NORMAL]".white().to_string(),
                "[THROTTLED]" => "[THROTTLED]".yellow().bold().to_string(),
                "[OOM_RISK]" => "[OOM_RISK]".red().bold().to_string(),
                _ => item.health_indicator.clone(),
            };

            lines.push(format!(
                " * {} ({}) | CPU: {} (wgt {}) | Mem: {}/{} | {}",
                item.server_name.white().bold(),
                item.limits.priority.name().dimmed(),
                cpu_str,
                item.limits.cpu_weight,
                cur_mem,
                mem_str,
                status_badge
            ));
        }
        if usage.len() > 6 {
            lines.push(format!("   ... and {} more servers", usage.len() - 6).dimmed().to_string());
        }
    }

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push(
        "  craft quota list                 List all active server cgroups and utilization"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft quota get <server>         Inspect detailed CPU/memory/IO metrics"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft quota set <server> ...     Hot-apply CPU, memory limits and priority"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft quota tenant list|set      Manage tenant aggregate resource budgets"
            .green()
            .to_string(),
    );
    lines.push(
        "  craft quota balance              Enforce fair-share scheduling arbitration"
            .green()
            .to_string(),
    );

    show_modal_message("RESOURCE QUOTAS & CGROUPS V2 SCHEDULING", &lines, false)?;
    Ok(())
}

pub async fn distributed_tracing_tui(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Distributed Tracing & OTel");

    let status = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_tracing_status().await {
            Ok(s) => s,
            Err(_) => craft_daemon::TracingService::global(paths).get_status(),
        }
    } else {
        craft_daemon::TracingService::global(paths).get_status()
    };

    let spans = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.query_traces(None, None, None, false, Some(8)).await {
            Ok(s) => s,
            Err(_) => {
                craft_daemon::TracingService::global(paths)
                    .query_traces(None, None, None, false, Some(8))
            }
        }
    } else {
        craft_daemon::TracingService::global(paths)
            .query_traces(None, None, None, false, Some(8))
    };

    let mut lines = Vec::new();
    lines.push("DISTRIBUTED REAL-TIME TRACING & OPENTELEMETRY".bold().to_string());
    lines.push("W3C traceparent Propagation & Bounded Circular Span Ring Buffer".dimmed().to_string());
    lines.push("".to_string());

    lines.push(format!(
        "Engine Status:     {}",
        if status.enabled { "[ACTIVE] Tracing Enabled".green() } else { "[DISABLED]".yellow() }
    ));
    lines.push(format!("Service Name:      {}", status.service_name.cyan()));
    lines.push(format!(
        "Sample Ratio:      {:.2}",
        status.sample_ratio
    ));
    lines.push(format!(
        "Ring Buffer:       {}/{} spans (dropped: {})",
        status.spans_buffered,
        status.buffer_capacity,
        if status.spans_dropped > 0 { status.spans_dropped.to_string().yellow() } else { "0".normal() }
    ));
    lines.push(format!(
        "Total Recorded:    {}",
        status.spans_recorded
    ));
    lines.push(format!(
        "OTLP Exporter:     {} (endpoint: {})",
        if status.otlp_endpoint.is_some() { "Configured".green() } else { "Disabled".dimmed() },
        status.otlp_endpoint.as_deref().unwrap_or("[none]")
    ));

    lines.push("".to_string());
    lines.push("Recent Recorded Spans:".bold().to_string());
    if spans.is_empty() {
        lines.push("  [INFO] No recorded spans in ring buffer.".dimmed().to_string());
    } else {
        for span in spans.iter().take(6) {
            let dur_str = if span.duration_micros < 1000 {
                format!("{}µs", span.duration_micros)
            } else {
                format!("{:.2}ms", span.duration_micros as f64 / 1000.0)
            };
            let status_badge = match span.status.code.as_str() {
                "OK" => "[OK]".green(),
                "ERROR" => "[ERR]".red(),
                _ => "[---]".dimmed(),
            };
            let tid_hex = span.trace_id.to_hex();
            let tid_short = if tid_hex.len() >= 8 {
                &tid_hex[..8]
            } else {
                "trace"
            };
            lines.push(format!(
                "  {} {:<22} {:<14} {:>8} ({})",
                status_badge,
                span.name,
                span.service_name.cyan(),
                dur_str.yellow(),
                tid_short
            ));
        }
    }

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push("  craft trace status               Display runtime sampler and buffer health".green().to_string());
    lines.push("  craft trace list [--service <s>] Query recorded traces in ring buffer".green().to_string());
    lines.push("  craft trace get <trace-id>       Inspect ASCII causal span tree & attributes".green().to_string());
    lines.push("  craft trace export               Trigger immediate OTLP/HTTP batch export".green().to_string());
    lines.push("  craft trace config --enabled ... Hot-reconfigure sampling & OTLP endpoint".green().to_string());

    show_modal_message("DISTRIBUTED TRACING & OPENTELEMETRY", &lines, false)?;
    Ok(())
}

pub async fn anvil_storage_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Anvil Storage & io_uring");

    let status = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_anvil_status().await {
            Ok(s) => s,
            Err(_) => craft_daemon::AnvilService::global(paths).get_status(),
        }
    } else {
        craft_daemon::AnvilService::global(paths).get_status()
    };

    let mem_used_mb = (status.cache_memory_used_bytes as f64) / (1024.0 * 1024.0);
    let mem_limit_mb = (status.cache_memory_limit_bytes as f64) / (1024.0 * 1024.0);
    let r_mb = (status.total_bytes_read as f64) / (1024.0 * 1024.0);
    let w_mb = (status.total_bytes_written as f64) / (1024.0 * 1024.0);
    let ratio_pct = status.cache_hit_ratio * 100.0;

    let mut lines = Vec::new();
    lines.push("HARDWARE-ACCELERATED ANVIL STORAGE ENGINE (MCA)".bold().to_string());
    lines.push("Linux io_uring Pipelines, Direct Memory Chunk Cache & Zero-Copy Framing".dimmed().to_string());
    lines.push("".to_string());

    let engine_badge = if status.engine == "io_uring" {
        "[ACTIVE] Linux io_uring (Zero-Copy Ring Pipeline)".green()
    } else {
        "[FALLBACK] Threaded preadv2/pwritev2 Driver".yellow()
    };
    lines.push(format!("I/O Engine:          {}", engine_badge));
    lines.push(format!(
        "Cached Chunks:       {} chunks resident in memory",
        status.active_cached_chunks
    ));
    lines.push(format!(
        "Cache Direct Memory: {:.2} MB / {:.2} MB",
        mem_used_mb, mem_limit_mb
    ));

    // Progress bar for memory utilization
    let usage_frac = if mem_limit_mb > 0.0 {
        (mem_used_mb / mem_limit_mb).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled_slots = (usage_frac * 24.0).round() as usize;
    let empty_slots = 24usize.saturating_sub(filled_slots);
    let bar = format!("[{}{}] {:.1}%", "=".repeat(filled_slots).cyan(), " ".repeat(empty_slots), usage_frac * 100.0);
    lines.push(format!("Memory Utilization:  {}", bar));

    lines.push(format!(
        "Cache Hit Ratio:     {:.2}% ({} hits / {} misses)",
        ratio_pct, status.cache_hit_count, status.cache_miss_count
    ));
    lines.push(format!(
        "Evictions / Prefetch:{} evicted / {} prefetched",
        status.cache_eviction_count, status.cache_prefetch_count
    ));
    lines.push(format!(
        "Total I/O Ops:       {} operations ({:.2} MB read / {:.2} MB written)",
        status.total_io_ops, r_mb, w_mb
    ));
    lines.push(format!(
        "Context Switch Sav:  {} syscall switches saved",
        status.context_switch_savings
    ));
    lines.push(format!(
        "Average I/O Latency: {:.2} µs",
        status.avg_io_latency_micros
    ));

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push("  craft anvil status                Display cache metrics and context-switch savings".green().to_string());
    lines.push("  craft anvil inspect <server> <f>  Inspect MCA sector allocation, header & fragmentation".green().to_string());
    lines.push("  craft anvil prefetch <server>     Prefetch chunk radius into LRU cache".green().to_string());
    lines.push("  craft anvil bench [--chunks 32]   Run I/O read/write throughput benchmark".green().to_string());
    lines.push("  craft anvil config --engine uring Configure preferred storage engine & memory limit".green().to_string());

    show_modal_message("HARDWARE-ACCELERATED ANVIL STORAGE (MCA)", &lines, false)?;
    Ok(())
}

pub async fn numa_dpdk_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("DPDK & NUMA Pinning");

    let status = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_numa_status().await {
            Ok(s) => s,
            Err(_) => craft_daemon::DpdkNumaService::global(paths).get_numa_status().await,
        }
    } else {
        craft_daemon::DpdkNumaService::global(paths).get_numa_status().await
    };

    let dpdk_stats = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.get_dpdk_status(None).await.ok()
    } else {
        Some(craft_daemon::DpdkNumaService::global(paths).get_dpdk_status(None))
    };

    let total_bytes: u64 = status.topology.nodes.iter().map(|n| n.total_memory_bytes).sum();
    let free_bytes: u64 = status.topology.nodes.iter().map(|n| n.free_memory_bytes).sum();
    let total_mb = (total_bytes as f64) / (1024.0 * 1024.0);
    let free_mb = (free_bytes as f64) / (1024.0 * 1024.0);
    let used_mb = (total_mb - free_mb).max(0.0);

    let mut lines = Vec::new();
    lines.push("KERNEL-BYPASSED DPDK PACKET PROCESSING & NUMA PINNING".bold().to_string());
    lines.push("Hardware Core Isolation, NUMA Local Node Memory & Ring Buffer Pipelines".dimmed().to_string());
    lines.push("".to_string());

    let numa_badge = if status.topology.is_numa_available {
        "[ACTIVE] Hardware NUMA Architecture Available".green()
    } else {
        "[INFO] Unified Memory Architecture (UMA) Fallback".yellow()
    };
    lines.push(format!("NUMA Architecture:   {}", numa_badge));
    lines.push(format!("Online NUMA Nodes:   {} nodes", status.topology.nodes.len()));
    lines.push(format!("Logical CPU Cores:   {} cores", status.topology.total_cpus));
    lines.push(format!(
        "NUMA Memory Capacity:{:.2} MB used / {:.2} MB total ({:.2} MB free)",
        used_mb, total_mb, free_mb
    ));

    // Progress bar for memory utilization
    let usage_frac = if total_mb > 0.0 {
        (used_mb / total_mb).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled_slots = (usage_frac * 24.0).round() as usize;
    let empty_slots = 24usize.saturating_sub(filled_slots);
    let bar = format!("[{}{}] {:.1}%", "=".repeat(filled_slots).cyan(), " ".repeat(empty_slots), usage_frac * 100.0);
    lines.push(format!("NUMA RAM Allocated:  {}", bar));

    let isolated_str = if status.isolated_cpus.is_empty() {
        "None (run `craft numa boot-args` to configure isolcpus)".to_string()
    } else {
        craft_core::format_cpu_range_string(&status.isolated_cpus)
    };
    lines.push(format!("Isolated CPU Cores:  {}", isolated_str.cyan()));

    let dpdk_badge = if let Some(ref d) = dpdk_stats {
        if d.is_hardware_driver_active {
            "[ACTIVE] Hardware Poll-Mode Driver (vfio-pci)".green()
        } else {
            format!("[FALLBACK] Ring Buffer ({:.0} pps, {:.2} µs)", d.throughput_pps, d.avg_jitter_micros).yellow()
        }
    } else {
        "[FALLBACK] Cache-Aligned Ring Buffer".yellow()
    };
    lines.push(format!("Kernel-Bypass DPDK:  {}", dpdk_badge));
    lines.push(format!("Pinned Game Servers: {} servers", status.pinned_servers_count));

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push("  craft numa status                 Inspect NUMA topology, memory per node, and hugepages".green().to_string());
    lines.push("  craft numa pin <server> --cpus <> Pin game server to CPU cores and NUMA memory node".green().to_string());
    lines.push("  craft numa policy <server> --pol  Update memory policy (local, interleave, bind)".green().to_string());
    lines.push("  craft numa bench [--node 0]       Benchmark memory bandwidth & ring burst throughput".green().to_string());
    lines.push("  craft numa boot-args --cores <>   Generate Linux isolcpus and nohz_full boot parameters".green().to_string());

    show_modal_message("KERNEL-BYPASSED DPDK & NUMA MEMORY PINNING", &lines, false)?;
    Ok(())
}

pub async fn live_migration_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Live Migration & Anycast");

    let plans = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.list_migrations().await.unwrap_or_default()
    } else {
        craft_daemon::MigrationService::global(paths).list_migrations().unwrap_or_default()
    };

    let reg = craft_core::MigrationRegistry::load(paths).unwrap_or_default();

    let mut lines = Vec::new();
    lines.push("DISTRIBUTED LIVE MIGRATION & GLOBAL ANYCAST STEERING".bold().to_string());
    lines.push("Iterative Dirty Memory Pre-Copy, Sub-150ms Freeze SLA & TCP Handoff".dimmed().to_string());
    lines.push("".to_string());

    let active_count = plans.iter().filter(|p| p.status.is_active()).count();
    let completed_count = plans.iter().filter(|p| matches!(p.status, craft_core::MigrationStage::Completed)).count();
    let rollback_count = plans.iter().filter(|p| matches!(p.status, craft_core::MigrationStage::RolledBack { .. })).count();

    lines.push(format!("Active Migrations:   {} in-flight", active_count));
    lines.push(format!("Completed (Zero-Loss):{} instances", completed_count));
    lines.push(format!("Rolled Back (Safe):  {} instances", rollback_count));
    lines.push(format!("Anycast Routes:      {} configured ({} active)", reg.routes.len(), reg.routes.iter().filter(|r| r.active).count()));

    if !plans.is_empty() {
        lines.push("".to_string());
        lines.push("Recent Live Migrations:".dimmed().to_string());
        for p in plans.iter().take(3) {
            let status_badge = match p.status {
                craft_core::MigrationStage::Completed => "[COMPLETED]".green(),
                craft_core::MigrationStage::RolledBack { .. } => "[ROLLED_BACK]".red(),
                craft_core::MigrationStage::FreezeAndHandoff => "[FREEZE]".yellow(),
                _ => "[PRE_COPY]".blue(),
            };
            lines.push(format!(
                "  * {} ({}) {} -> {} ({} rounds, SLA: {}ms)",
                p.migration_id, p.server_name, p.source_node, p.target_node, p.rounds.len(), p.freeze_timeout_ms
            ));
            lines.push(format!("    Status: {}", status_badge));
        }
    }

    if !reg.routes.is_empty() {
        lines.push("".to_string());
        lines.push("Anycast BGP Steering Routes:".dimmed().to_string());
        for r in reg.routes.iter().take(3) {
            let active_str = if r.active { "[ANNOUNCED]".green() } else { "[WITHDRAWN]".dimmed() };
            lines.push(format!("  * {} (AS{}) -> {} community: {}", r.prefix, r.asn, active_str, r.community.first().map(|s| s.as_str()).unwrap_or("-")));
        }
    }

    lines.push("".to_string());
    lines.push("CLI Commands:".dimmed().to_string());
    lines.push("  craft migrate live <server> --target-node <> Execute zero-downtime live migration".green().to_string());
    lines.push("  craft migrate status [--id <id>]             Inspect iterative pre-copy convergence".green().to_string());
    lines.push("  craft migrate abort --id <id>                Trigger immediate rollback failback".green().to_string());
    lines.push("  craft anycast route <announce|withdraw>      Steer BGP Anycast routes dynamically".green().to_string());

    show_modal_message("ZERO-DOWNTIME LIVE MIGRATION & ANYCAST", &lines, false)?;
    Ok(())
}





