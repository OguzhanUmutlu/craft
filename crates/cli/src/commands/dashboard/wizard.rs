use colored::Colorize;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;
use craft_providers::get_all_softwares;

use super::get_system_summary;
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, run_menu_with_tick_handler, show_modal_message, AltScreenGuard, EventDecision,
    MenuEntry, NavGuard,
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
        CustomRuntime,
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

    let mut custom_runtime_type = craft_scripting::CustomRuntimeType::Lua;
    let mut custom_port: u16 = 8080;
    let mut custom_exec: String = "server.lua".to_string();
    let mut custom_sel = 0;

    let mut game_sel = 0;
    let mut cat_sel = 0;
    let mut java_sel = 0;
    let mut sw_sel = 0;
    let mut ver_sel = 0;
    let mut mem_sel = 1;
    let mut start_sel = 0;

    let get_cat_crumbs = |cat_idx: usize,
                          java_type: usize,
                          game_id: &str|
     -> (Option<&'static str>, Option<&'static str>) {
        if game_id == "minecraft" {
            match cat_idx {
                0 => {
                    let sub = if java_type == 1 {
                        "Plugins"
                    } else if java_type == 2 {
                        "Modded"
                    } else {
                        "Vanilla"
                    };
                    (Some("Java Edition"), Some(sub))
                }
                1 => (Some("Bedrock Edition"), None),
                2 => (Some("Network Proxies"), None),
                3 => (Some("Hybrid & Cross-Play"), None),
                4 => (Some("Minecraft Softwares"), None),
                _ => (None, None),
            }
        } else if cat_idx == 99 {
            (Some("All Softwares"), None)
        } else {
            (None, None)
        }
    };

    let get_ver_step_num =
        |game_id: &str, cat_idx: usize, java_type: usize, has_name_override: bool| -> usize {
            if game_id != "minecraft" && cat_idx != 99 {
                if has_name_override {
                    2
                } else {
                    3
                }
            } else if cat_idx == 99 {
                if has_name_override {
                    3
                } else {
                    4
                }
            } else if cat_idx == 0 && java_type == 0 {
                if has_name_override {
                    4
                } else {
                    5
                }
            } else if cat_idx == 0 {
                if has_name_override {
                    5
                } else {
                    6
                }
            } else {
                if has_name_override {
                    4
                } else {
                    5
                }
            }
        };

    loop {
        match step {
            WizardStep::Name => {
                let current_step = 1;
                let total_steps = 6;
                let title = format!(
                    "SERVER SETUP WIZARD (STEP {}/{})",
                    current_step, total_steps
                );
                match run_input_prompt(&title, "Enter server name:", Some(&server_name))? {
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
                let current_step = if has_name_override { 1 } else { 2 };
                let total_steps = if has_name_override { 5 } else { 6 };
                let title = format!(
                    "SELECT GAME ENVIRONMENT (STEP {}/{})",
                    current_step, total_steps
                );
                let width = get_content_width(80);
                let game_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose game environment for '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let game_entries = vec![
                    MenuEntry::new("1", "Minecraft (Java, Bedrock, Proxies)"),
                    MenuEntry::new("2", "Palworld"),
                    MenuEntry::new("3", "Terraria (TShock)"),
                    MenuEntry::new("4", "Valheim"),
                    MenuEntry::new("5", "Factorio (Headless)"),
                    MenuEntry::new("6", "Custom Game (Generic binary/script)"),
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
                        selected_sw_name = "Palworld";
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
                        selected_sw_name = "Valheim";
                        step = WizardStep::Version;
                    }
                    Some(4) => {
                        game_id = "factorio";
                        selected_sw_id = "factorio";
                        selected_sw_name = "Factorio Headless";
                        step = WizardStep::Version;
                    }
                    Some(5) => {
                        game_id = "custom";
                        selected_sw_id = "custom";
                        selected_sw_name = "Custom Game";
                        step = WizardStep::CustomRuntime;
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

            WizardStep::CustomRuntime => {
                let _sub_nav = NavGuard::enter("Custom Runtime");
                let current_step = if has_name_override { 2 } else { 3 };
                let total_steps = current_step + 1;
                let title = format!(
                    "SELECT RUNTIME ENVIRONMENT (STEP {}/{})",
                    current_step, total_steps
                );
                let width = get_content_width(80);
                let cr_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose runtime type for custom server '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let cr_entries = vec![
                    MenuEntry::new(
                        "1",
                        "Lua Script Server (Interactive server with Craft standard library)",
                    ),
                    MenuEntry::new(
                        "2",
                        "Native Binary Executable (C/C++, Rust, Go, Unity, Unreal binary)",
                    ),
                    MenuEntry::new(
                        "3",
                        "Shell / Batch Script Server (Custom bash / batch wrapper)",
                    ),
                    MenuEntry::new("0", "Back to Game Selection").with_aliases(&["b"]),
                ];

                let cr_choice = run_menu(&cr_header, &cr_entries, &mut custom_sel)?;
                match cr_choice {
                    Some(0) => {
                        custom_runtime_type = craft_scripting::CustomRuntimeType::Lua;
                        custom_exec = "server.lua".to_string();
                        if let Ok(Some(p_str)) = run_input_prompt(
                            "CUSTOM SERVER PORT",
                            "Enter listening network port for Lua server:",
                            Some("8080"),
                        ) {
                            if let Ok(p) = p_str.trim().parse::<u16>() {
                                custom_port = p;
                            }
                        }
                        step = WizardStep::Autostart;
                    }
                    Some(1) => {
                        custom_runtime_type = craft_scripting::CustomRuntimeType::Binary;
                        let default_bin = if cfg!(windows) {
                            "server.exe"
                        } else {
                            "./server"
                        };
                        if let Ok(Some(exec_str)) = run_input_prompt(
                            "BINARY EXECUTABLE PATH",
                            "Enter executable file path or command:",
                            Some(default_bin),
                        ) {
                            if !exec_str.trim().is_empty() {
                                custom_exec = exec_str.trim().to_string();
                            } else {
                                custom_exec = default_bin.to_string();
                            }
                        } else {
                            custom_exec = default_bin.to_string();
                        }
                        if let Ok(Some(p_str)) = run_input_prompt(
                            "CUSTOM SERVER PORT",
                            "Enter listening network port for binary server:",
                            Some("8080"),
                        ) {
                            if let Ok(p) = p_str.trim().parse::<u16>() {
                                custom_port = p;
                            }
                        }
                        step = WizardStep::Autostart;
                    }
                    Some(2) => {
                        custom_runtime_type = craft_scripting::CustomRuntimeType::Script;
                        let default_script = if cfg!(windows) {
                            "server.cmd"
                        } else {
                            "./server.sh"
                        };
                        if let Ok(Some(exec_str)) = run_input_prompt(
                            "SCRIPT FILE PATH",
                            "Enter script file path:",
                            Some(default_script),
                        ) {
                            if !exec_str.trim().is_empty() {
                                custom_exec = exec_str.trim().to_string();
                            } else {
                                custom_exec = default_script.to_string();
                            }
                        } else {
                            custom_exec = default_script.to_string();
                        }
                        if let Ok(Some(p_str)) = run_input_prompt(
                            "CUSTOM SERVER PORT",
                            "Enter listening network port for script server:",
                            Some("8080"),
                        ) {
                            if let Ok(p) = p_str.trim().parse::<u16>() {
                                custom_port = p;
                            }
                        }
                        step = WizardStep::Autostart;
                    }
                    _ => {
                        step = WizardStep::GameSelect;
                    }
                }
            }

            WizardStep::MinecraftCategory => {
                let current_step = if has_name_override { 2 } else { 3 };
                let total_steps = current_step + 3;
                let title = format!(
                    "SELECT MINECRAFT CATEGORY (STEP {}/{})",
                    current_step, total_steps
                );
                let width = get_content_width(80);
                let mc_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose Minecraft platform category for '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
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
                let _sub_nav = NavGuard::enter("Java Edition");
                let current_step = if has_name_override { 3 } else { 4 };
                let total_steps = current_step + 3;
                let title = format!(
                    "SELECT JAVA SERVER TYPE (STEP {}/{})",
                    current_step, total_steps
                );
                let width = get_content_width(80);
                let jt_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose server type for Java Edition:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
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
                let (cat1, cat2) = get_cat_crumbs(cat_idx, java_type, game_id);
                let _nav_c1 = cat1.map(NavGuard::enter);
                let _nav_c2 = cat2.map(NavGuard::enter);

                let (current_step, total_steps) = if cat_idx == 0 {
                    let s = if has_name_override { 4 } else { 5 };
                    (s, s + 3)
                } else if cat_idx == 99 {
                    let s = if has_name_override { 2 } else { 3 };
                    (s, s + 2)
                } else {
                    let s = if has_name_override { 3 } else { 4 };
                    (s, s + 2)
                };
                let title = format!(
                    "SELECT SERVER SOFTWARE (STEP {}/{})",
                    current_step, total_steps
                );

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
                                vec![(
                                    "vanilla_java",
                                    "Vanilla Java",
                                    "Official Mojang Java server",
                                )]
                            }
                        }
                        1 => vec![
                            (
                                "vanilla_bedrock",
                                "Vanilla Bedrock BDS",
                                "Official Mojang Bedrock BDS",
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
                    box_title(&title, width, false).cyan().bold(),
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
                let (cat1, cat2) = get_cat_crumbs(cat_idx, java_type, game_id);
                let _nav_c1 = cat1.map(NavGuard::enter);
                let _nav_c2 = cat2.map(NavGuard::enter);
                let _nav_sw = NavGuard::enter(selected_sw_name);

                let sw_obj = craft_providers::find_software(selected_sw_id);
                let is_java = sw_obj
                    .as_ref()
                    .map(|s| s.edition() == craft_providers::ServerEdition::Java)
                    .unwrap_or(true);
                let current_step = get_ver_step_num(game_id, cat_idx, java_type, has_name_override);
                let min_menus_left = if is_java { 2 } else { 1 };
                let total_steps = current_step + min_menus_left;
                let title = format!(
                    "SELECT SERVER VERSION (STEP {}/{})",
                    current_step, total_steps
                );

                let width = get_content_width(80);
                let mut settings = craft_core::GlobalSettings::load(paths).unwrap_or_default();
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
                    "{}\r\n{}\r\n{}\r\n Status: {} | Auto-Update: {}\r\n Select release version for {}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false)
                        .cyan()
                        .bold(),
                    box_divider(width).cyan().bold(),
                    cache_status,
                    auto_status,
                    selected_sw_name,
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

                let display_versions = std::sync::Arc::new(std::sync::Mutex::new(versions_list));

                let mut ver_entries: Vec<MenuEntry> = Vec::new();

                let back_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

                let toggle_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("a", "Toggle Auto-Update"));

                let update_idx = ver_entries.len();
                ver_entries.push(MenuEntry::new("u", "Update Versions"));

                let versions_start_idx = ver_entries.len();
                {
                    let v_list = display_versions.lock().unwrap();
                    for v in v_list.iter() {
                        let label = if v == &recommended_ver {
                            format!("{} (Recommended)", v)
                        } else {
                            v.clone()
                        };
                        ver_entries.push(MenuEntry::button(label));
                    }
                }

                let rec_offset = {
                    let v_list = display_versions.lock().unwrap();
                    v_list
                        .iter()
                        .position(|v| v == &recommended_ver)
                        .unwrap_or(0)
                };
                let default_ver_sel = versions_start_idx + rec_offset;
                if ver_sel < versions_start_idx || ver_sel >= ver_entries.len() {
                    ver_sel = default_ver_sel;
                }

                let (bg_tx, bg_rx) = std::sync::mpsc::channel::<
                    std::result::Result<craft_providers::VersionCatalog, String>,
                >();
                let is_updating = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let is_updating_action = is_updating.clone();
                let is_updating_tick = is_updating.clone();
                let auto_update_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                    settings.auto_update_catalog,
                ));
                let auto_update_action = auto_update_flag.clone();
                let auto_update_tick = auto_update_flag.clone();
                let display_versions_clone = display_versions.clone();
                let selected_sw_id_str = selected_sw_id.to_string();
                let cache_status_str = cache_status.to_string();

                let ver_choice = run_menu_with_tick_handler(
                    &ver_header,
                    &ver_entries,
                    &mut ver_sel,
                    std::time::Duration::from_millis(60),
                    |ch, idx, entries, header_rows| {
                        if ch == 'a'
                            || ch == 'A'
                            || entries.get(idx).map(|e| e.hotkey.as_str()) == Some("a")
                        {
                            let new_val =
                                !auto_update_action.load(std::sync::atomic::Ordering::SeqCst);
                            auto_update_action.store(new_val, std::sync::atomic::Ordering::SeqCst);
                            settings.auto_update_catalog = new_val;
                            let _ = settings.save(paths);
                            let new_auto = if new_val {
                                "ON".green()
                            } else {
                                "OFF".dimmed()
                            };
                            for row in header_rows.iter_mut() {
                                if modalx::strip_ansi(row).contains("Auto-Update:") {
                                    let status_part = if is_updating_action
                                        .load(std::sync::atomic::Ordering::SeqCst)
                                    {
                                        "Updating...".yellow()
                                    } else {
                                        cache_status_str.as_str().green()
                                    };
                                    *row = format!(
                                        " Status: {} | Auto-Update: {}",
                                        status_part, new_auto
                                    );
                                }
                            }
                            EventDecision::Cancel
                        } else if ch == 'u' || ch == 'U' || idx == update_idx {
                            if is_updating_action.load(std::sync::atomic::Ordering::SeqCst) {
                                return EventDecision::Cancel;
                            }
                            is_updating_action.store(true, std::sync::atomic::Ordering::SeqCst);

                            // Grey out update button
                            if update_idx < entries.len() {
                                entries[update_idx] = modalx::SelectItem::new(
                                    "-",
                                    "Updating...".dimmed().to_string(),
                                );
                            }

                            // Update header status
                            let auto_part =
                                if auto_update_action.load(std::sync::atomic::Ordering::SeqCst) {
                                    "ON".green()
                                } else {
                                    "OFF".dimmed()
                                };
                            for row in header_rows.iter_mut() {
                                if modalx::strip_ansi(row).contains("Auto-Update:") {
                                    *row = format!(
                                        " Status: {} | Auto-Update: {}",
                                        "Updating...".yellow(),
                                        auto_part
                                    );
                                }
                            }

                            // Spawn background catalog update
                            let tx = bg_tx.clone();
                            std::thread::Builder::new()
                                .name("craft-catalog-update".to_string())
                                .spawn(move || {
                                    let rt = match tokio::runtime::Builder::new_current_thread()
                                        .enable_all()
                                        .build()
                                    {
                                        Ok(r) => r,
                                        Err(e) => {
                                            let _ = tx.send(Err(e.to_string()));
                                            return;
                                        }
                                    };
                                    let res = rt.block_on(async {
                                        let mgr = craft_providers::CatalogManager::new()?;
                                        mgr.update().await
                                    });
                                    let _ = tx.send(res.map_err(|e| e.to_string()));
                                })
                                .ok();

                            EventDecision::Cancel
                        } else {
                            EventDecision::Proceed
                        }
                    },
                    |selected_idx, entries, header_rows| -> bool {
                        if is_updating_tick.load(std::sync::atomic::Ordering::SeqCst) {
                            if let Ok(res) = bg_rx.try_recv() {
                                is_updating_tick.store(false, std::sync::atomic::Ordering::SeqCst);

                                // Restore update button
                                if update_idx < entries.len() {
                                    entries[update_idx] =
                                        modalx::SelectItem::new("u", "Update Versions");
                                }

                                match res {
                                    Ok(cat) => {
                                        let mut new_versions =
                                            cat.get_versions(&selected_sw_id_str);
                                        if new_versions.is_empty() {
                                            if let Some(sw) =
                                                craft_providers::find_software(&selected_sw_id_str)
                                            {
                                                new_versions = sw.bundled_versions();
                                            }
                                        }
                                        craft_core::sort_versions_descending(&mut new_versions);
                                        if new_versions.is_empty() {
                                            new_versions.push("latest".to_string());
                                        }

                                        let new_rec =
                                            craft_providers::find_software(&selected_sw_id_str)
                                                .map(|s| s.recommended_version())
                                                .unwrap_or_else(|| new_versions[0].clone());

                                        // Preserve currently selected version
                                        let prev_selected_ver = {
                                            let v_list = display_versions_clone.lock().unwrap();
                                            if *selected_idx >= versions_start_idx {
                                                v_list
                                                    .get(*selected_idx - versions_start_idx)
                                                    .cloned()
                                            } else {
                                                None
                                            }
                                        };

                                        // Update shared versions list
                                        {
                                            let mut v_list = display_versions_clone.lock().unwrap();
                                            *v_list = new_versions.clone();
                                        }

                                        // Rebuild version entries
                                        entries.truncate(versions_start_idx);
                                        for v in &new_versions {
                                            let label = if v == &new_rec {
                                                format!("{} (Recommended)", v)
                                            } else {
                                                v.clone()
                                            };
                                            entries.push(modalx::SelectItem::button(label));
                                        }

                                        // Keep selected version highlighted if it still exists; fallback to recommended
                                        if let Some(ref prev_v) = prev_selected_ver {
                                            if let Some(pos) =
                                                new_versions.iter().position(|v| v == prev_v)
                                            {
                                                *selected_idx = versions_start_idx + pos;
                                            } else {
                                                let rec_pos = new_versions
                                                    .iter()
                                                    .position(|v| v == &new_rec)
                                                    .unwrap_or(0);
                                                *selected_idx = versions_start_idx + rec_pos;
                                            }
                                        }

                                        // Update header
                                        let auto_part = if auto_update_tick
                                            .load(std::sync::atomic::Ordering::SeqCst)
                                        {
                                            "ON".green()
                                        } else {
                                            "OFF".dimmed()
                                        };
                                        for row in header_rows.iter_mut() {
                                            if modalx::strip_ansi(row).contains("Auto-Update:") {
                                                *row = format!(
                                                    " Status: {} | Auto-Update: {}",
                                                    "Fresh Cache".green(),
                                                    auto_part
                                                );
                                            }
                                        }
                                        return true;
                                    }
                                    Err(_) => {
                                        // On error, restore header status
                                        let auto_part = if auto_update_tick
                                            .load(std::sync::atomic::Ordering::SeqCst)
                                        {
                                            "ON".green()
                                        } else {
                                            "OFF".dimmed()
                                        };
                                        for row in header_rows.iter_mut() {
                                            if modalx::strip_ansi(row).contains("Auto-Update:") {
                                                *row = format!(
                                                    " Status: {} | Auto-Update: {}",
                                                    "Offline (Cache)".yellow(),
                                                    auto_part
                                                );
                                            }
                                        }
                                        return true;
                                    }
                                }
                            }
                        }
                        false
                    },
                )?;
                match ver_choice {
                    Some(idx) if idx == back_idx => {
                        if game_id != "minecraft" && cat_idx != 99 {
                            step = WizardStep::GameSelect;
                        } else if cat_idx == 0 && java_type == 0 {
                            step = WizardStep::JavaType;
                        } else {
                            step = WizardStep::Software;
                        }
                    }
                    Some(idx) if idx == toggle_idx => {
                        settings.auto_update_catalog = !settings.auto_update_catalog;
                        let _ = settings.save(paths);
                    }
                    Some(idx) if idx >= versions_start_idx => {
                        let v_idx = idx - versions_start_idx;
                        let v_list = display_versions.lock().unwrap();
                        if v_idx < v_list.len() {
                            version = v_list[v_idx].clone();
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
                let (cat1, cat2) = get_cat_crumbs(cat_idx, java_type, game_id);
                let _nav_c1 = cat1.map(NavGuard::enter);
                let _nav_c2 = cat2.map(NavGuard::enter);
                let _nav_sw = NavGuard::enter(selected_sw_name);

                let ver_step_num = get_ver_step_num(game_id, cat_idx, java_type, has_name_override);
                let current_step = ver_step_num + 1;
                let total_steps = current_step + 1;
                let title = format!(
                    "ALLOCATE SERVER MEMORY (STEP {}/{})",
                    current_step, total_steps
                );

                let (_os, total_ram, used_ram, ram_pct) = get_system_summary();
                let width = get_content_width(80);
                let mem_header = format!(
                    "{}\r\n{}\r\n{}\r\n Host RAM: {:.1} / {:.1} GB ({:.1}%) | Choose memory allocation limit:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
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
                let (cat1, cat2) = get_cat_crumbs(cat_idx, java_type, game_id);
                let _nav_c1 = cat1.map(NavGuard::enter);
                let _nav_c2 = cat2.map(NavGuard::enter);
                let _nav_sw = NavGuard::enter(selected_sw_name);

                let sw_obj = craft_providers::find_software(selected_sw_id);
                let is_java = sw_obj
                    .as_ref()
                    .map(|s| s.edition() == craft_providers::ServerEdition::Java)
                    .unwrap_or(true);
                let ver_step_num = get_ver_step_num(game_id, cat_idx, java_type, has_name_override);
                let current_step = if selected_sw_id == "custom" {
                    if has_name_override {
                        3
                    } else {
                        4
                    }
                } else if is_java {
                    ver_step_num + 2
                } else {
                    ver_step_num + 1
                };
                let total_steps = current_step;
                let title = format!(
                    "INITIALIZATION MODE (STEP {}/{})",
                    current_step, total_steps
                );

                let width = get_content_width(80);
                let start_header = format!(
                    "{}\r\n{}\r\n{}\r\n How should server '{}' be initialized upon creation?\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&title, width, false).cyan().bold(),
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
                        if selected_sw_id == "custom" {
                            step = WizardStep::CustomRuntime;
                        } else {
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
            }

            WizardStep::Execute => {
                break;
            }
        }
    }

    // Execution
    print_in_place_status(
        "CREATING SERVER",
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
        if selected_sw_id == "custom" {
            Some(custom_port)
        } else {
            None
        },
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
        if selected_sw_id == "custom" {
            Some(custom_runtime_type.as_str())
        } else {
            None
        },
        if selected_sw_id == "custom" {
            Some(custom_exec.as_str())
        } else {
            None
        },
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
