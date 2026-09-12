use colored::Colorize;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;
use craft_providers::get_all_softwares;

use crate::commands::new::handle_new;
use super::get_system_summary;
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};

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
        Category,
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
        WizardStep::Category
    } else {
        WizardStep::Name
    };

    let mut cat_idx: usize = 0;
    let mut java_type: usize = 0;
    let mut selected_sw_id: &'static str = "paper";
    let mut selected_sw_name: &'static str = "Paper";
    let mut version: String = "latest".to_string();
    let mut memory: String = "4G".to_string();
    let mut start_now: bool = true;

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
                                    "Please choose a different name for your new server.".to_string(),
                                ],
                                true,
                            )?;
                            continue;
                        }
                        server_name = name;
                        step = WizardStep::Category;
                    }
                    _ => return Ok(()),
                }
            }

            WizardStep::Category => {
                let width = get_content_width(80);
                let cat_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose platform category for server '{}':\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 2/6: SELECT PLATFORM CATEGORY", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server_name,
                    box_divider(width).dimmed(),
                );

                let cat_entries = vec![
                    MenuEntry::new("1", "Java Edition"),
                    MenuEntry::new("2", "Bedrock Edition"),
                    MenuEntry::new("3", "Network Proxies"),
                    MenuEntry::new("4", "Hybrid & Cross-Play"),
                    MenuEntry::new("5", "Browse All 16 Platforms"),
                    MenuEntry::new("0", if has_name_override { "Cancel" } else { "Back" }).with_aliases(&["b"]),
                ];

                let cat_choice = run_menu(&cat_header, &cat_entries, &mut cat_sel)?;
                match cat_choice {
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
                        if has_name_override {
                            return Ok(());
                        } else {
                            step = WizardStep::Name;
                        }
                    }
                }
            }

            WizardStep::JavaType => {
                let width = get_content_width(80);
                let jt_header = format!(
                    "{}\r\n{}\r\n{}\r\n Choose server type for Java Edition:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 2: SELECT JAVA SERVER TYPE", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    box_divider(width).dimmed(),
                );

                let jt_entries = vec![
                    MenuEntry::new("1", "Plugins & Vanilla"),
                    MenuEntry::new("2", "Modded Servers"),
                    MenuEntry::new("0", "Back to Platform Categories").with_aliases(&["b"]),
                ];

                let jt_choice = run_menu(&jt_header, &jt_entries, &mut java_sel)?;
                match jt_choice {
                    Some(0) => {
                        java_type = 0;
                        step = WizardStep::Software;
                    }
                    Some(1) => {
                        java_type = 1;
                        step = WizardStep::Software;
                    }
                    _ => {
                        step = WizardStep::Category;
                    }
                }
            }

            WizardStep::Software => {
                let software_choices: Vec<(&'static str, &'static str, &'static str)> = match cat_idx {
                    0 => {
                        if java_type == 0 {
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
                                (
                                    "folia",
                                    "Folia",
                                    "Multi-threaded regional ticking server",
                                ),
                                ("spigot", "Spigot", "Classic Bukkit / Spigot plugin server"),
                                (
                                    "vanilla_java",
                                    "Vanilla Java",
                                    "Official Mojang Java dedicated server",
                                ),
                            ]
                        } else {
                            vec![
                                (
                                    "fabric",
                                    "Fabric",
                                    "Lightweight modular modded server",
                                ),
                                (
                                    "quilt",
                                    "Quilt",
                                    "Community-driven modular modded server",
                                ),
                                (
                                    "neoforge",
                                    "NeoForge",
                                    "Modern Forge-compatible modded server",
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
                        (
                            "waterfall",
                            "Waterfall",
                            "Optimized BungeeCord proxy fork",
                        ),
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
                    _ => get_all_softwares()
                        .into_iter()
                        .map(|s| (s.id(), s.name(), s.description()))
                        .collect(),
                };

                let width = get_content_width(80);
                let sw_header = format!(
                    "{}\r\n{}\r\n{}\r\n Select the server software implementation:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 3/6: SELECT SERVER SOFTWARE", width, false).cyan().bold(),
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
                        } else {
                            step = WizardStep::Category;
                        }
                    }
                }
            }

            WizardStep::Version => {
                let width = get_content_width(80);
                let ver_header = format!(
                    "{}\r\n{}\r\n{}\r\n Select Minecraft release version for {}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("STEP 4/6: SELECT SERVER VERSION", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    selected_sw_name,
                    box_divider(width).dimmed(),
                );

                let ver_entries = vec![
                    MenuEntry::new("1", "latest (Recommended)"),
                    MenuEntry::new("2", "1.21.4"),
                    MenuEntry::new("3", "1.21.1"),
                    MenuEntry::new("4", "1.20.4"),
                    MenuEntry::new("5", "1.20.1"),
                    MenuEntry::new("6", "1.19.4"),
                    MenuEntry::new("7", "1.18.2"),
                    MenuEntry::new("8", "1.16.5"),
                    MenuEntry::new("c", "Custom Version"),
                    MenuEntry::new("0", "Back").with_aliases(&["b"]),
                ];

                let ver_choice = run_menu(&ver_header, &ver_entries, &mut ver_sel)?;
                match ver_choice {
                    Some(0) => {
                        version = "latest".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(1) => {
                        version = "1.21.4".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(2) => {
                        version = "1.21.1".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(3) => {
                        version = "1.20.4".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(4) => {
                        version = "1.20.1".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(5) => {
                        version = "1.19.4".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(6) => {
                        version = "1.18.2".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(7) => {
                        version = "1.16.5".to_string();
                        step = WizardStep::Memory;
                    }
                    Some(8) => {
                        match run_input_prompt(
                            "CUSTOM MINECRAFT VERSION",
                            "Enter target version (e.g. 1.21.3, 1.20.2):",
                            Some("1.21.4"),
                        )? {
                            Some(v) if !v.trim().is_empty() => {
                                version = v.trim().to_string();
                                step = WizardStep::Memory;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        step = WizardStep::Software;
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
                let start_header = format!(
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
                        step = WizardStep::Memory;
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
        "CREATING MINECRAFT SERVER",
        &[
            format!(
                "Setting up server '{}' ({} {})...",
                server_name, selected_sw_name, version
            ),
            "Downloading server jarfile and configuring runtime environment...".to_string(),
            "Please wait...".to_string(),
        ],
    )?;

    let res = handle_new(
        &server_name,
        Some(selected_sw_id),
        Some(&version),
        None,       // port: defaults to 25565
        None,       // custom_path
        Some(&memory),
        true,       // agree_eula
        false,      // tmp
        true,       // no_start: wizard manages starting via daemon directly
        true,       // yes = true (non-interactive execution)
        true,       // aikar G1GC flags
        false,      // zgc
        false,      // shenandoah
        None,       // jvm_flags
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
                    format!(
                        "[OK] Server '{}' was registered successfully!",
                        server_name
                    )
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
