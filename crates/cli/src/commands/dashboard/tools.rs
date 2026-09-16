use std::path::PathBuf;
use colored::Colorize;

use craft_backup::BackupEngine;
use craft_core::{
    kill_process, read_pid_file, CraftPaths, RemoteAuthType, RemoteHostConfig, RemotesRegistry,
    Result, ServersRegistry,
};
use craft_daemon::DaemonClient;
use craft_plugins::PluginManager;
use craft_providers::CacheManager;

use crate::commands::remote::parse_connection_string;
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};
use super::server_control::show_empty_servers_modal;

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
        box_title("SELECT PING PROTOCOL", width, false).cyan().bold(),
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
        Some(2) => (Some(craft_core::QueryProtocolKind::MinecraftBedrockRakNet), 19132),
        Some(3) => (Some(craft_core::QueryProtocolKind::ValveA2S), 27015),
        _ => return Ok(()),
    };

    let (host, port) = if let Some(idx) = target.find(':') {
        let (h, p) = target.split_at(idx);
        let port_num: u16 = p[1..]
            .parse()
            .unwrap_or(default_port);
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
                    format!("Game / Map:  {} ({}) / {}", res.game_name, res.game_folder, res.map_name),
                    format!("Players:     {}/{} (Bots: {})", res.online_players, res.max_players, res.bots),
                    format!("Server Type: {} [{}]", res.server_type, res.environment),
                    format!("VAC Secured: {}", if res.vac_secured { "Yes" } else { "No" }),
                    format!("Latency:     {} ms", res.latency_ms),
                ],
                false,
            )?;
        }
        Ok(craft_net::UniversalPingStatus::PortProbe { latency_ms, transport, .. }) => {
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
            format!("[GDrive: {}]", gd.folder_id).green().bold().to_string()
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
            MenuEntry::new("1", "Setup Backup Systems (Local, S3 / R2 / MinIO, Google Drive)").with_aliases(&["s", "c"]),
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
                                .create_backup(&server.name, &server.path, None, world_only)
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

                    let p_header =
                        format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen_plugin = &results[p_idx];
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

                            let s_header = format!(
                                " Install '{}' to which server?",
                                chosen_plugin.name
                            );
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

                    let p_header =
                        format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];

                            let registry = ServersRegistry::load(paths)?;
                            let compatible: Vec<_> = registry
                                .servers
                                .iter()
                                .filter(|s| craft_providers::get_content_capabilities(&s.software).mods)
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

                            let s_header = format!(
                                " Select server to install mod '{}':",
                                chosen.name
                            );
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
                                        .install_mod_from_modrinth(
                                            &server.path,
                                            &chosen.id_or_slug,
                                        )
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

                    let p_header =
                        format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen = &results[p_idx];

                            let registry = ServersRegistry::load(paths)?;
                            let compatible: Vec<_> = registry
                                .servers
                                .iter()
                                .filter(|s| craft_providers::get_content_capabilities(&s.software).datapacks)
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

                            let s_header = format!(
                                " Select server to install datapack '{}':",
                                chosen.name
                            );
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

                        let s_header = format!(
                            " Select server to install map '{}':",
                            chosen.name
                        );
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
                                        let cur_default = craft_core::get_default_world(&server.path);
                                        let p_header = format!(
                                            " World '{}' has been installed into '{}'.\r\n Current default world (level-name): '{}'\r\n\r\n Set '{}' as the default world in server.properties?",
                                            installed_name, server.name, cur_default, installed_name
                                        );
                                        let p_opts = vec![
                                            MenuEntry::new("1", "No (Keep current)").with_aliases(&["n", "no"]),
                                            MenuEntry::new("2", "Yes (Set as default)").with_aliases(&["y", "yes"]),
                                        ];
                                        let mut p_choice = 0;
                                        if let Some(c) = run_menu(&p_header, &p_opts, &mut p_choice)? {
                                            if c == 1 {
                                                craft_core::set_default_world(&server.path, &installed_name)?;
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
                        .map(|r| {
                            format!("{:<16} {}@{}:{}", r.alias, r.user, r.host, r.port)
                        })
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
                if let Some(r_idx) =
                    run_menu(" Select remote host to test connection:", &r_entries, &mut r_sel)?
                {
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
                            &[format!(
                                "[OK] Remote host '{}' configured successfully!",
                                alias
                            )
                            .green()
                            .bold()
                            .to_string()],
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
                    Err(e) => show_modal_message(
                        "RESTART FAILED",
                        &[format!("[ERROR] {}", e)],
                        true,
                    )?,
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
                    show_modal_message("NO SERVERS", &["No registered servers found to configure.".to_string()], true)?;
                    continue;
                }
                let mut s_entries = Vec::new();
                for (i, s) in reg.servers.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    let port = s.port.unwrap_or(25565);
                    s_entries.push(MenuEntry::new(hotkey, format!("{:<20} Port: {:<6} Software: {}", s.name, port, s.software)));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                let mut s_sel = 0;
                if let Some(idx) = run_menu(" Select Server for Firewall Rule:", &s_entries, &mut s_sel)? {
                    if idx < reg.servers.len() {
                        let s = &reg.servers[idx];
                        let is_bedrock = s.software.contains("bedrock") || s.software.contains("pocketmine");
                        let port = s.port.unwrap_or(if is_bedrock { 19132 } else { 25565 });
                        let ip_prompt = run_input_prompt(
                            "ALLOWED IP ADDRESS",
                            "Enter remote IP allowed to connect (e.g. 192.168.1.50 or 0.0.0.0/0 for any):",
                            Some("0.0.0.0/0"),
                        )?;
                        if let Some(ip) = ip_prompt {
                            let ip = ip.trim();
                            if !ip.is_empty() {
                                let _ = print_in_place_status("APPLYING FIREWALL RULE", &[format!("Adding rule for port {} ({})...", port, if is_bedrock { "UDP" } else { "TCP" })]);
                                match craft_net::allow_ip_port(ip, port, is_bedrock) {
                                    Ok(_) => {
                                        show_modal_message("FIREWALL RULE ADDED", &[format!("[OK] Allowed incoming connections from '{}' on port {}.", ip, port).green().bold().to_string()], false)?;
                                    }
                                    Err(e) => {
                                        show_modal_message("FIREWALL ERROR", &[format!("[ERROR] {}", e)], true)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                let ip_str = match run_input_prompt("ALLOWED IP", "Enter IP to allow (or 0.0.0.0/0 for any):", Some("0.0.0.0/0"))? {
                    Some(i) if !i.trim().is_empty() => i.trim().to_string(),
                    _ => continue,
                };
                let port_str = match run_input_prompt("PORT", "Enter port number to allow (e.g. 25565):", Some("25565"))? {
                    Some(p) if !p.trim().is_empty() => p.trim().to_string(),
                    _ => continue,
                };
                let port: u16 = match port_str.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        show_modal_message("INVALID PORT", &["Port must be a number between 1 and 65535.".to_string()], true)?;
                        continue;
                    }
                };
                let proto_sel = run_menu(" Select Protocol:", &[MenuEntry::new("1", "TCP (Java)"), MenuEntry::new("2", "UDP (Bedrock)")], &mut 0)?;
                let is_udp = proto_sel == Some(1);
                let _ = print_in_place_status("APPLYING FIREWALL RULE", &[format!("Adding rule for {}:{}...", ip_str, port)]);
                match craft_net::allow_ip_port(&ip_str, port, is_udp) {
                    Ok(_) => {
                        show_modal_message("FIREWALL RULE ADDED", &[format!("[OK] Successfully allowed {}:{} ({})!", ip_str, port, if is_udp { "UDP" } else { "TCP" }).green().bold().to_string()], false)?;
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
            MenuEntry::new("1", if status { "Re-enable / Refresh Loopback Exemption" } else { "Enable Loopback Exemption" }),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                match craft_net::enable_bedrock_loopback() {
                    Ok(_) => {
                        show_modal_message("LOOPBACK EXEMPTION APPLIED", &["[OK] Windows UWP Loopback exemption enabled successfully!".green().bold().to_string()], false)?;
                    }
                    Err(e) => {
                        show_modal_message("LOOPBACK ERROR", &[format!("[ERROR] Failed to enable loopback: {}", e)], true)?;
                    }
                }
            }
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
        let entries = vec![
            MenuEntry::new("1", "Server Network Ping").with_aliases(&["p", "ping"]),
            MenuEntry::new("2", "Daemon Control").with_aliases(&["d", "daemon"]),
            MenuEntry::new("3", "Firewall Manager (Port/IP Rules)").with_aliases(&["f", "firewall"]),
            MenuEntry::new("4", "Windows Bedrock Loopback Exemption").with_aliases(&["l", "loopback"]),
            MenuEntry::new("5", purge_label).with_aliases(&["c", "cache"]),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                ping_menu().await?;
            }
            Some(1) => {
                daemon_menu(paths).await?;
            }
            Some(2) => {
                firewall_menu(paths).await?;
            }
            Some(3) => {
                loopback_menu().await?;
            }
            Some(4) => {
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
            _ => return Ok(()),
        }
    }
}

