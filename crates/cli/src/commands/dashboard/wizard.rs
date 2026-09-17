use colored::Colorize;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;
use craft_providers::get_all_softwares;

use super::get_system_summary;
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, run_menu_with_handler, show_modal_message, AltScreenGuard, EventDecision, MenuEntry,
    NavGuard,
};
use crate::commands::new::handle_new;

pub async fn gui_create_server_wizard(paths: &CraftPaths) -> Result<()> {
    gui_create_server_wizard_with_name("", paths).await
}

pub async fn gui_create_server_wizard_with_name(
    name_override: &str,
    paths: &CraftPaths,
) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Create Server");

    let has_name_override = !name_override.trim().is_empty();
    let mut server_name = if has_name_override {
        name_override.trim().to_string()
    } else {
        "my-server".to_string()
    };

    enum WizardStep {
        Name,
        GameSelect,
        MinecraftCategory,
        JavaType,
        Software,
        Version,
        Memory,
        Autostart,
        Execute,
    }

    let mut step = if has_name_override {
        let registry = ServersRegistry::load(paths)?;
        if registry
            .servers
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&server_name))
        {
            show_modal_message(
                "NAME ALREADY REGISTERED",
                &[
                    format!(
                        "A server named '{}' already exists on this machine.",
                        server_name
                    ),
                    "Please choose a different name for your new server.".to_string(),
                ],
                true,
            )?;
            return Ok(());
        }
        WizardStep::GameSelect
    } else {
        WizardStep::Name
    };

    let mut game_id: &'static str = "minecraft";
    let mut cat_idx: usize = 0;
    let mut java_type: usize = 0;
    let mut selected_sw_id: &'static str = "paper";
    let mut selected_sw_name: &'static str = "Paper";
    let mut version: String = "latest".to_string();
    let mut memory: String = "4G".to_string();
    let mut start_now: bool = true;

    let mut game_sel = 0;
    let mut cat_sel = 0;
    let mut java_sel = 0;
    let mut sw_sel = 0;
    let mut ver_sel = 0;
    let mut mem_sel = 1;
    let mut start_sel = 0;

    loop {
        match step {
            WizardStep::Name => {
                match run_input_prompt(
                    "SERVER SETUP WIZARD (STEP 1/6)",
                    "Enter server name:",
                    Some(&server_name),
                )? {
                    Some(n) if !n.trim().is_empty() => {
                        let name = n.trim().to_string();
                        let registry = ServersRegistry::load(paths)?;
                        if registry
                            .servers
                            .iter()
                            .any(|s| s.name.eq_ignore_ascii_case(&name))
                        {
                            show_modal_message(
                                "NAME ALREADY REGISTERED",
                                &[
                                    format!(
                                        "A server named '{}' already exists on this machine.",
                                        name
                                    ),
                                    "Please choose a different name for your new server."
                                        .to_string(),
                                ],
                                true,
                            )?;
                            continue;
                        }
                        server_name = name;
                        step = WizardStep::GameSelect;
                    }
                    _ => return Ok(()),
                }
            }

            WizardStep::GameSelect => {
                let width = get_content_width(80);
                let game_header =
                    format!(
                    "{}\r\n{}\r\n{}\r\n Choose dedicated game environment for server '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 2/6: SELECT GAME ENVIRONMENT", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let game_entries = vec![
                    MenuEntry::new("1", "Minecraft (Java, Bedrock, Proxies)"),
                    MenuEntry::new("2", "Palworld (Dedicated Server)"),
                    MenuEntry::new("3", "Terraria (TShock Dedicated Server)"),
                    MenuEntry::new("4", "Valheim (Dedicated Server)"),
                    MenuEntry::new("5", "Factorio (Headless Dedicated Server)"),
                    MenuEntry::new("6", "Custom Game Server (Generic binary/script)"),
                    MenuEntry::new("7", "Browse All 21 Softwares"),
                    MenuEntry::new("0", if has_name_override { "Cancel" } else { "Back" })
                        .with_aliases(&["b"]),
                ];

                let game_choice = run_menu(&game_header, &game_entries, &mut game_sel)?;
                match game_choice {
                    Some(0) => {
                        game_id = "minecraft";
                        step = WizardStep::MinecraftCategory;
                    }
                    Some(1) => {
                        game_id = "palworld";
                        selected_sw_id = "palserver";
                        selected_sw_name = "Palworld Dedicated Server";
                        step = WizardStep::Version;
                    }
                    Some(2) => {
                        game_id = "terraria";
                        selected_sw_id = "tshock";
                        selected_sw_name = "TShock (Terraria)";
                        step = WizardStep::Version;
                    }
                    Some(3) => {
                        game_id = "valheim";
                        selected_sw_id = "valheim";
                        selected_sw_name = "Valheim Dedicated Server";
                        step = WizardStep::Version;
                    }
                    Some(4) => {
                        game_id = "factorio";
                        selected_sw_id = "factorio";
                        selected_sw_name = "Factorio Headless Server";
                        step = WizardStep::Version;
                    }
                    Some(5) => {
                        game_id = "custom";
                        selected_sw_id = "custom";
                        selected_sw_name = "Custom Game Server";
                        step = WizardStep::Version;
                    }
                    Some(6) => {
                        game_id = "all";
                        cat_idx = 99;
                        step = WizardStep::Software;
                    }
                    _ => {
                        if has_name_override {
                            return Ok(());
                        } else {
                            step = WizardStep::Name;
                        }
                    }
                }
            }

            WizardStep::MinecraftCategory => {
                let width = get_content_width(80);
                let mc_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose Minecraft platform category for '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 2b: SELECT MINECRAFT CATEGORY", width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let mc_entries = vec![
                    MenuEntry::new("1", "Java Edition"),
                    MenuEntry::new("2", "Bedrock Edition"),
                    MenuEntry::new("3", "Network Proxies & Bridges"),
                    MenuEntry::new("4", "Hybrid & Cross-Play"),
                    MenuEntry::new("5", "All Minecraft Softwares"),
                    MenuEntry::new("0", "Back to Game Selection").with_aliases(&["b"]),
                ];

                let mc_choice = run_menu(&mc_header, &mc_entries, &mut cat_sel)?;
                match mc_choice {
                    Some(0) => {
                        cat_idx = 0;
                        step = WizardStep::JavaType;
                    }
                    Some(1) => {
                        cat_idx = 1;
                        step = WizardStep::Software;
                    }
                    Some(2) => {
                        cat_idx = 2;
                        step = WizardStep::Software;
                    }
                    Some(3) => {
                        cat_idx = 3;
                        step = WizardStep::Software;
                    }
                    Some(4) => {
                        cat_idx = 4;
                        step = WizardStep::Software;
                    }
                    _ => {
                        step = WizardStep::GameSelect;
                    }
                }
            }

            WizardStep::JavaType => {
                let width = get_content_width(80);
                let jt_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose server type for Java Edition:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 2c: SELECT JAVA SERVER TYPE", width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    box_divider(width).dimmed(),
                );

                let jt_entries = vec![
                    MenuEntry::new("1", "Vanilla"),
                    MenuEntry::new("2", "Plugins"),
                    MenuEntry::new("3", "Modded"),
                    MenuEntry::new("0", "Back").with_aliases(&["b"]),
                ];

                let jt_choice = run_menu(&jt_header, &jt_entries, &mut java_sel)?;
                match jt_choice {
                    Some(0) => {
                        // Vanilla: auto-advance directly to Version selection
                        java_type = 0;
                        selected_sw_id = "vanilla_java";
                        selected_sw_name = "Vanilla Java";
                        step = WizardStep::Version;
                    }
                    Some(1) => {
                        // Plugins (Paper, Purpur, Folia, Spigot)
                        java_type = 1;
                        step = WizardStep::Software;
                    }
                    Some(2) => {
                        // Modded (Fabric, Quilt, NeoForge)
                        java_type = 2;
                        step = WizardStep::Software;
                    }
                    _ => {
                        step = WizardStep::MinecraftCategory;
                    }
                }
            }

            WizardStep::Software => {
                let software_choices: Vec<(&'static str, &'static str, &'static str)> =
                    match cat_idx {
                        0 => {
                            if java_type == 1 {
                                vec![
                                    (
                                        "paper",
                                        "Paper",
                                        "High-performance standard Java server (Rec.)",
                                    ),
                                    (
                                        "purpur",
                                        "Purpur",
                                        "Paper fork with extensive gameplay tweaks",
                                    ),
                                    ("folia", "Folia", "Multi-threaded regional ticking server"),
                                    ("spigot", "Spigot", "Classic Bukkit / Spigot plugin server"),
                                ]
                            } else if java_type == 2 {
                                vec![
                                    ("fabric", "Fabric", "Lightweight modular modded server"),
                                    ("quilt", "Quilt", "Community-driven modular modded server"),
                                    (
                                        "neoforge",
                                        "NeoForge",
                                        "Modern Forge-compatible modded server",
                                    ),
                                ]
                            } else {
                                vec![
                                    (
                                        "vanilla_java",
                                        "Vanilla Java",
                                        "Official Mojang Java dedicated server",
                                    ),
                                ]
                            }
                        }
                        1 => vec![
                            (
                                "vanilla_bedrock",
                                "Vanilla Bedrock BDS",
                                "Official Mojang Bedrock Dedicated Server",
                            ),
                            (
                                "pocketmine",
                                "PocketMine-MP",
                                "High-performance C++ / PHP Bedrock server",
                            ),
                            (
                                "nukkit",
                                "NukkitX",
                                "Java-based multi-threaded Bedrock server",
                            ),
                        ],
                        2 => vec![
                            (
                                "velocity",
                                "Velocity",
                                "Next-generation ultra-fast proxy (Rec.)",
                            ),
                            ("waterfall", "Waterfall", "Optimized BungeeCord proxy fork"),
                            (
                                "bungeecord",
                                "BungeeCord",
                                "Classic multi-server network proxy",
                            ),
                            ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
                        ],
                        3 => vec![
                            (
                                "geyser",
                                "GeyserMC Standalone",
                                "Cross-play bridge for Bedrock clients",
                            ),
                            ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
                        ],
                        4 => craft_providers::get_softwares_for_game("minecraft")
                            .into_iter()
                            .map(|s| (s.id(), s.name(), s.description()))
                            .collect(),
                        _ => get_all_softwares()
                            .into_iter()
                            .map(|s| (s.id(), s.name(), s.description()))
                            .collect(),
                    };

                let width = get_content_width(80);
                let sw_header = format!(
                    "{}\r\n{}\r\n{}\r\n Select the server software implementation:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 3/6: SELECT SERVER SOFTWARE", width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    box_divider(width).dimmed(),
                );

                let mut sw_entries: Vec<MenuEntry> = software_choices
                    .iter()
                    .enumerate()
                    .map(|(i, (_id, name, desc))| {
                        let hotkey = if i < 9 {
                            (i + 1).to_string()
                        } else {
                            ((b'a' + (i - 9) as u8) as char).to_string()
                        };
                        MenuEntry::new(hotkey, format!("{:<20} - {}", name, desc))
                    })
                    .collect();
                sw_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                let sw_choice = run_menu(&sw_header, &sw_entries, &mut sw_sel)?;
                match sw_choice {
                    Some(idx) if idx < software_choices.len() => {
                        selected_sw_id = software_choices[idx].0;
                        selected_sw_name = software_choices[idx].1;
                        step = WizardStep::Version;
                    }
                    _ => {
                        if cat_idx == 0 {
                            step = WizardStep::JavaType;
                        } else if cat_idx == 99 {
                            step = WizardStep::GameSelect;
                        } else {
                            step = WizardStep::MinecraftCategory;
                        }
                    }
                }
            }

            WizardStep::Version => {
                let width = get_content_width(80);
                let mut settings = craft_core::GlobalSettings::load(paths).unwrap_or_default();
                let sw_obj = craft_providers::find_software(selected_sw_id);
                let catalog_mgr = craft_providers::CatalogManager::new().ok();

                if let Some(ref mgr) = catalog_mgr {
                    mgr.spawn_background_update_if_needed(settings.auto_update_catalog);
                }

                let cache_status = if let Some(ref mgr) = catalog_mgr {
                    if mgr.is_cache_fresh() {
                        "Fresh Cache".green()
                    } else if mgr.cache_exists() {
                        "Cached".yellow()
                    } else {
                        "Bundled Fallback".cyan()
                    }
                } else {
                    "Bundled".cyan()
                };
                let auto_status = if settings.auto_update_catalog {
                    "ON".green()
                } else {
                    "OFF".dimmed()
                };

                let ver_header = format!(
                    "{}\r\n{}\r\n{}\r\n Select release version for {}:\r\n Status: {} | Auto-Update: {}\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 4/6: SELECT SERVER VERSION", width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    selected_sw_name,
                    cache_status,
                    auto_status,
                    box_divider(width).dimmed(),
                );

                let catalog_versions = if let Some(ref mgr) = catalog_mgr {
                    mgr.load().get_versions(selected_sw_id)
                } else {
                    Vec::new()
                };

                let mut versions_list = if !catalog_versions.is_empty() {
                    catalog_versions
                } else {
                    sw_obj
                        .as_ref()
                        .map(|s| s.bundled_versions())
                        .unwrap_or_default()
                };

                craft_core::sort_versions_descending(&mut versions_list);
                if versions_list.is_empty() {
                    versions_list.push("latest".to_string());
                }

                let recommended_ver = sw_obj
                    .as_ref()
                    .map(|s| s.recommended_version())
                    .unwrap_or_else(|| versions_list[0].clone());

                let display_versions: Vec<String> = versions_list.into_iter().take(30).collect();

                let mut ver_entries: Vec<MenuEntry> = display_versions
                    .iter()
                    .map(|v| {
                        let label = if v == &recommended_ver {
                            format!("{} (Recommended)", v)
                        } else {
                            v.clone()
                        };
                        MenuEntry::button(label)
                    })
                    .collect();

                let custom_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("c", "Custom Version"));

                let update_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("u", "Update Versions"));

                let toggle_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new(
                    "a",
                    format!(
                        "Toggle Auto-Update: [{}]",
                        if settings.auto_update_catalog { "ON" } else { "OFF" }
                    ),
                ));

                let back_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                let ver_choice = run_menu_with_handler(
                    &ver_header,
                    &ver_entries,
                    &mut ver_sel,
                    |ch, idx, entries, _header_rows| {
                        if ch == 'a' || ch == 'A' || entries.get(idx).map(|e| e.hotkey.as_str()) == Some("a") {
                            settings.auto_update_catalog = !settings.auto_update_catalog;
                            let _ = settings.save(paths);
                            if let Some(entry) = entries.iter_mut().find(|e| e.hotkey == "a") {
                                entry.label = format!(
                                    "Toggle Auto-Update: [{}]",
                                    if settings.auto_update_catalog { "ON" } else { "OFF" }
                                );
                            }
                            EventDecision::Cancel
                        } else {
                            EventDecision::Proceed
                        }
                    },
                )?;
                match ver_choice {
                    Some(idx) if idx < custom_idx => {
                        version = display_versions[idx].clone();
                        let is_java = sw_obj
                            .as_ref()
                            .map(|s| s.edition() == craft_providers::ServerEdition::Java)
                            .unwrap_or(true);
                        if is_java {
                            step = WizardStep::Memory;
                        } else {
                            step = WizardStep::Autostart;
                        }
                    }
                    Some(idx) if idx == custom_idx => {
                        let default_v = display_versions.first().map(|s| s.as_str()).unwrap_or("latest");
                        match run_input_prompt(
                            "CUSTOM SERVER VERSION",
                            "Enter target release version string:",
                            Some(default_v),
                        )? {
                            Some(v) if !v.trim().is_empty() => {
                                version = v.trim().to_string();
                                let is_java = sw_obj
                                    .as_ref()
                                    .map(|s| s.edition() == craft_providers::ServerEdition::Java)
                                    .unwrap_or(true);
                                if is_java {
                                    step = WizardStep::Memory;
                                } else {
                                    step = WizardStep::Autostart;
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(idx) if idx == update_idx => {
                        if let Some(ref mgr) = catalog_mgr {
                            match mgr.update().await {
                                Ok(cat) => {
                                    let total = cat.softwares.values().map(|s| s.versions.len()).sum::<usize>();
                                    show_modal_message(
                                        "VERSION CATALOG UPDATED",
                                        &[
                                            "Version catalog refreshed successfully from remote endpoint.".to_string(),
                                            format!("Softwares: {} | Total versions indexed: {}", cat.softwares.len(), total),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "CATALOG UPDATE FAILED",
                                        &[
                                            "Unable to fetch version catalog from remote endpoint.".to_string(),
                                            e.to_string(),
                                        ],
                                        true,
                                    )?;
                                }
                            }
                        }
                    }
                    Some(idx) if idx == toggle_idx => {
                        settings.auto_update_catalog = !settings.auto_update_catalog;
                        let _ = settings.save(paths);
                    }
                    Some(idx) if idx == back_idx => {
                        if game_id != "minecraft" && cat_idx != 99 {
                            step = WizardStep::GameSelect;
                        } else if cat_idx == 0 && java_type == 0 {
                            step = WizardStep::JavaType;
                        } else {
                            step = WizardStep::Software;
                        }
                    }
                    _ => {
                        if game_id != "minecraft" && cat_idx != 99 {
                            step = WizardStep::GameSelect;
                        } else if cat_idx == 0 && java_type == 0 {
                            step = WizardStep::JavaType;
                        } else {
                            step = WizardStep::Software;
                        }
                    }
                }
            }

            WizardStep::Memory => {
                let (_os, total_ram, used_ram, ram_pct) = get_system_summary();
                let width = get_content_width(80);
                let mem_header = format!(
                    "{}\r\n{}\r\n{}\r\n Host RAM: {:.1} / {:.1} GB ({:.1}%) | Choose memory allocation limit:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 5/6: ALLOCATE SERVER MEMORY", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    used_ram,
                    total_ram,
                    ram_pct,
                    box_divider(width).dimmed(),
                );

                let mem_entries = vec![
                    MenuEntry::new("1", "2G"),
                    MenuEntry::new("2", "4G (Recommended)"),
                    MenuEntry::new("3", "8G"),
                    MenuEntry::new("4", "16G"),
                    MenuEntry::new("c", "Custom Limit"),
                    MenuEntry::new("0", "Back").with_aliases(&["b"]),
                ];

                let mem_choice = run_menu(&mem_header, &mem_entries, &mut mem_sel)?;
                match mem_choice {
                    Some(0) => {
                        memory = "2G".to_string();
                        step = WizardStep::Autostart;
                    }
                    Some(1) => {
                        memory = "4G".to_string();
                        step = WizardStep::Autostart;
                    }
                    Some(2) => {
                        memory = "8G".to_string();
                        step = WizardStep::Autostart;
                    }
                    Some(3) => {
                        memory = "16G".to_string();
                        step = WizardStep::Autostart;
                    }
                    Some(4) => {
                        match run_input_prompt(
                            "CUSTOM MEMORY LIMIT",
                            "Enter memory limit with suffix (e.g. 6G, 12G, 512M):",
                            Some("4G"),
                        )? {
                            Some(m) if !m.trim().is_empty() => {
                                memory = m.trim().to_string();
                                step = WizardStep::Autostart;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        step = WizardStep::Version;
                    }
                }
            }

            WizardStep::Autostart => {
                let width = get_content_width(80);
                let start_header =
                    format!(
                    "{}\r\n{}\r\n{}\r\n How should server '{}' be initialized upon creation?\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 6/6: INITIALIZATION MODE", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let start_entries = vec![
                    MenuEntry::new("1", "Start Server Immediately"),
                    MenuEntry::new("2", "Create Server Only"),
                    MenuEntry::new("0", "Back").with_aliases(&["b"]),
                ];

                let start_choice = run_menu(&start_header, &start_entries, &mut start_sel)?;
                match start_choice {
                    Some(0) => {
                        start_now = true;
                        step = WizardStep::Execute;
                    }
                    Some(1) => {
                        start_now = false;
                        step = WizardStep::Execute;
                    }
                    _ => {
                        let sw_obj = craft_providers::find_software(selected_sw_id);
                        let is_java = sw_obj
                            .as_ref()
                            .map(|s| s.edition() == craft_providers::ServerEdition::Java)
                            .unwrap_or(true);
                        if is_java {
                            step = WizardStep::Memory;
                        } else {
                            step = WizardStep::Version;
                        }
                    }
                }
            }

            WizardStep::Execute => {
                break;
            }
        }
    }

    // Execution
    print_in_place_status(
        "CREATING DEDICATED SERVER",
        &[
            format!(
                "Setting up server '{}' ({} {})...",
                server_name, selected_sw_name, version
            ),
            "Downloading server assets and configuring runtime environment...".to_string(),
            "Please wait...".to_string(),
        ],
    )?;

    let res = handle_new(
        &server_name,
        Some(selected_sw_id),
        Some(&version),
        None, // port: defaults to 25565
        None, // custom_path
        Some(&memory),
        true,  // agree_eula
        false, // tmp
        true,  // no_start: wizard manages starting via daemon directly
        true,  // yes = true (non-interactive execution)
        true,  // aikar G1GC flags
        false, // zgc
        false, // shenandoah
        None,  // jvm_flags
        paths,
    )
    .await;

    match res {
        Ok(_) => {
            if start_now {
                let s_path = paths.servers_dir.join(&server_name);
                let _ = DaemonClient::ensure_daemon_started(paths).await;
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    let _ = client.start_server(&s_path).await;
                }
            }

            let status_note = if start_now {
                "[RUNNING] Server has started in the background daemon."
                    .green()
                    .to_string()
            } else {
                "[STOPPED] Server created. Start anytime with option [3]."
                    .dimmed()
                    .to_string()
            };

            show_modal_message(
                "SERVER SETUP COMPLETE",
                &[
                    format!("[OK] Server '{}' was registered successfully!", server_name)
                        .green()
                        .bold()
                        .to_string(),
                    format!("Software: {} (Version: {})", selected_sw_name, version),
                    format!("Memory:   {}", memory),
                    format!("Status:   {}", status_note),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "SERVER CREATION FAILED",
                &[format!(
                    "[ERROR] Failed to set up server '{}': {}",
                    server_name, e
                )],
                true,
            )?;
        }
    }

    Ok(())
}
