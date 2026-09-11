use colored::Colorize;

use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_providers::get_all_softwares;

use crate::commands::new::handle_new;
use super::get_system_summary;
use super::screen::{
    print_in_place_status, run_input_prompt, run_menu, show_modal_message, AltScreenGuard,
    MenuEntry,
};

pub async fn gui_create_server_wizard(paths: &CraftPaths) -> Result<()> {
    gui_create_server_wizard_with_name("", paths).await
}

pub async fn gui_create_server_wizard_with_name(
    name_override: &str,
    paths: &CraftPaths,
) -> Result<()> {
    let _guard = AltScreenGuard::enter();

    // Step 1: Server Name
    let server_name = if !name_override.trim().is_empty() {
        name_override.trim().to_string()
    } else {
        match run_input_prompt(
            "SERVER SETUP WIZARD (STEP 1/6)",
            "Enter server name:",
            Some("my-server"),
        )? {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => return Ok(()),
        }
    };

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

    // Step 2: Software Category
    let cat_header = format!(
        "{}\r\n{}\r\n{}\r\n Choose platform category for server '{}':\r\n{}",
        "================================================================================"
            .cyan()
            .bold(),
        "                       STEP 2/6: SELECT PLATFORM CATEGORY                       "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
        server_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let cat_entries = vec![
        MenuEntry::new(
            "1",
            "Java High-Performance (Paper, Purpur, Folia, Spigot, Vanilla Java)",
        ),
        MenuEntry::new("2", "Modded & Hybrid (Fabric, Quilt, NeoForge)"),
        MenuEntry::new(
            "3",
            "Network Proxies (Velocity, Waterfall, BungeeCord, GeyserMC, WaterdogPE)",
        ),
        MenuEntry::new(
            "4",
            "Bedrock Dedicated (Vanilla Bedrock BDS, PocketMine-MP, NukkitX)",
        ),
        MenuEntry::new("5", "Browse All 16 Platforms"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut cat_sel = 0;
    let cat_choice = run_menu(&cat_header, &cat_entries, &mut cat_sel)?;
    let cat_idx = match cat_choice {
        Some(idx) if idx < 5 => idx,
        _ => return Ok(()),
    };

    let software_choices: Vec<(&'static str, &'static str, &'static str)> = match cat_idx {
        0 => vec![
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
        ],
        1 => vec![
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
            (
                "geyser",
                "GeyserMC Standalone",
                "Cross-play bridge for Bedrock clients",
            ),
            ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
        ],
        3 => vec![
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
        _ => get_all_softwares()
            .into_iter()
            .map(|s| (s.id(), s.name(), s.description()))
            .collect(),
    };

    // Step 3: Specific Software
    let sw_header = format!(
        "{}\r\n{}\r\n{}\r\n Select the server software implementation:\r\n{}",
        "================================================================================"
            .cyan()
            .bold(),
        "                        STEP 3/6: SELECT SERVER SOFTWARE                        "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
        "--------------------------------------------------------------------------------".dimmed()
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
    sw_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

    let mut sw_sel = 0;
    let sw_choice = run_menu(&sw_header, &sw_entries, &mut sw_sel)?;
    let (selected_sw_id, selected_sw_name) = match sw_choice {
        Some(idx) if idx < software_choices.len() => {
            (software_choices[idx].0, software_choices[idx].1)
        }
        _ => return Ok(()),
    };

    // Step 4: Version
    let ver_header = format!(
        "{}\r\n{}\r\n{}\r\n Select Minecraft release version for {}:\r\n{}",
        "================================================================================"
            .cyan()
            .bold(),
        "                        STEP 4/6: SELECT SERVER VERSION                         "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
        selected_sw_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let ver_entries = vec![
        MenuEntry::new(
            "1",
            "latest (Recommended - Automatically resolves latest release)",
        ),
        MenuEntry::new("2", "1.21.4 (Latest Stable Java Release)"),
        MenuEntry::new("3", "1.21.1"),
        MenuEntry::new("4", "1.20.4"),
        MenuEntry::new("5", "1.20.1"),
        MenuEntry::new("6", "1.19.4"),
        MenuEntry::new("7", "1.18.2"),
        MenuEntry::new("8", "1.16.5"),
        MenuEntry::new("c", "Custom Version (Type Manually)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut ver_sel = 0;
    let ver_choice = run_menu(&ver_header, &ver_entries, &mut ver_sel)?;
    let version = match ver_choice {
        Some(0) => "latest".to_string(),
        Some(1) => "1.21.4".to_string(),
        Some(2) => "1.21.1".to_string(),
        Some(3) => "1.20.4".to_string(),
        Some(4) => "1.20.1".to_string(),
        Some(5) => "1.19.4".to_string(),
        Some(6) => "1.18.2".to_string(),
        Some(7) => "1.16.5".to_string(),
        Some(8) => {
            match run_input_prompt(
                "CUSTOM MINECRAFT VERSION",
                "Enter target version (e.g. 1.21.3, 1.20.2):",
                Some("1.21.4"),
            )? {
                Some(v) if !v.trim().is_empty() => v.trim().to_string(),
                _ => return Ok(()),
            }
        }
        _ => return Ok(()),
    };

    // Step 5: Memory Allocation
    let (_os, total_ram, used_ram, ram_pct) = get_system_summary();
    let mem_header = format!(
        "{}\r\n{}\r\n{}\r\n Host RAM: {:.1} / {:.1} GB ({:.1}%) | Choose memory allocation limit:\r\n{}",
        "================================================================================"
            .cyan()
            .bold(),
        "                       STEP 5/6: ALLOCATE SERVER MEMORY                         "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
        used_ram,
        total_ram,
        ram_pct,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let mem_entries = vec![
        MenuEntry::new("1", "2G  (Standard lightweight / proxy testing)"),
        MenuEntry::new("2", "4G  (Recommended standard survival server)"),
        MenuEntry::new("3", "8G  (Large player counts / heavy plugins / mods)"),
        MenuEntry::new("4", "16G (High-capacity multi-world or network hub)"),
        MenuEntry::new("c", "Custom Limit (Type Manually)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut mem_sel = 1;
    let mem_choice = run_menu(&mem_header, &mem_entries, &mut mem_sel)?;
    let memory = match mem_choice {
        Some(0) => "2G".to_string(),
        Some(1) => "4G".to_string(),
        Some(2) => "8G".to_string(),
        Some(3) => "16G".to_string(),
        Some(4) => {
            match run_input_prompt(
                "CUSTOM MEMORY LIMIT",
                "Enter memory limit with suffix (e.g. 6G, 12G, 512M):",
                Some("4G"),
            )? {
                Some(m) if !m.trim().is_empty() => m.trim().to_string(),
                _ => return Ok(()),
            }
        }
        _ => return Ok(()),
    };

    // Step 6: Autostart Preference
    let start_header = format!(
        "{}\r\n{}\r\n{}\r\n How should server '{}' be initialized upon creation?\r\n{}",
        "================================================================================"
            .cyan()
            .bold(),
        "                         STEP 6/6: INITIALIZATION MODE                          "
            .cyan()
            .bold(),
        "================================================================================"
            .cyan()
            .bold(),
        server_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let start_entries = vec![
        MenuEntry::new("1", "Start Server Immediately (Background Daemon)"),
        MenuEntry::new("2", "Create Server Only (Do not start now)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut start_sel = 0;
    let start_choice = run_menu(&start_header, &start_entries, &mut start_sel)?;
    let start_now = match start_choice {
        Some(0) => true,
        Some(1) => false,
        _ => return Ok(()),
    };

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
        None,
        Some(&memory),
        true,       // agree_eula
        false,      // tmp
        !start_now, // no_start
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
