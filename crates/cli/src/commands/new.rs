use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use craft_core::{auto_heal_server_file, find_best_java, get_jar_java_version, CraftError, CraftPaths, Result, ServerConfig, ServersRegistry};
use craft_daemon::DaemonClient;
use craft_providers::{find_software, get_all_softwares, CacheManager, ServerEdition};
use crate::commands::run::run_foreground_server;

#[allow(clippy::too_many_arguments)]
pub async fn handle_new(
    name_input: &str,
    software_input: Option<&str>,
    version_input: Option<&str>,
    custom_path: Option<PathBuf>,
    memory_input: Option<&str>,
    mut agree_eula: bool,
    tmp: bool,
    no_start: bool,
    yes: bool,
    mut aikar: bool,
    mut zgc: bool,
    mut shenandoah: bool,
    jvm_flags: Option<Vec<String>>,
    paths: &CraftPaths,
) -> Result<()> {
    let is_tty = std::io::stdin().is_terminal() && !yes;
    let theme = ColorfulTheme::default();

    // 1. Determine server name
    let server_name = if !name_input.trim().is_empty() {
        name_input.trim().to_string()
    } else if is_tty {
        println!("{}", "=== Craft Server Setup Wizard ===".cyan().bold());
        Input::with_theme(&theme)
            .with_prompt("Enter server name")
            .default("my-server".to_string())
            .interact_text()?
    } else {
        "my-server".to_string()
    };

    // 2. Determine target software
    let selected_software_id = if let Some(sw) = software_input {
        sw.to_string()
    } else if is_tty {
        println!();
        println!("{}", "Select server platform category:".cyan().bold());
        let categories = &[
            "[1] Java Edition",
            "[2] Bedrock Edition",
            "[3] Network Proxies",
            "[4] Hybrid & Cross-Play",
            "[5] Browse All 16 Platforms",
        ];
        let cat_idx = Select::with_theme(&theme)
            .with_prompt("Category")
            .items(categories)
            .default(0)
            .interact()?;

        let software_choices: Vec<(&'static str, &'static str, &'static str)> = match cat_idx {
            0 => {
                println!();
                println!("{}", "Select Java server type:".cyan().bold());
                let java_types = &[
                    "[1] Plugins & Vanilla (Paper, Purpur, Folia, Spigot, Vanilla)",
                    "[2] Modded Servers (Fabric, Quilt, NeoForge)",
                ];
                let java_choice = Select::with_theme(&theme)
                    .with_prompt("Java Type")
                    .items(java_types)
                    .default(0)
                    .interact()?;

                if java_choice == 0 {
                    vec![
                        ("paper", "Paper", "High-performance standard Java server (Rec.)"),
                        ("purpur", "Purpur", "Paper fork with extensive gameplay tweaks"),
                        ("folia", "Folia", "Multi-threaded regional ticking server"),
                        ("spigot", "Spigot", "Classic Bukkit / Spigot plugin server"),
                        ("vanilla_java", "Vanilla Java", "Official Mojang Java dedicated server"),
                    ]
                } else {
                    vec![
                        ("fabric", "Fabric", "Lightweight modular modded server"),
                        ("quilt", "Quilt", "Community-driven modular modded server"),
                        ("neoforge", "NeoForge", "Modern Forge-compatible modded server"),
                    ]
                }
            }
            1 => vec![
                ("vanilla_bedrock", "Vanilla Bedrock BDS", "Official Mojang Bedrock Dedicated Server"),
                ("pocketmine", "PocketMine-MP", "High-performance C++ / PHP Bedrock server"),
                ("nukkit", "NukkitX", "Java-based multi-threaded Bedrock server"),
            ],
            2 => vec![
                ("velocity", "Velocity", "Next-generation ultra-fast proxy (Rec.)"),
                ("waterfall", "Waterfall", "Optimized BungeeCord proxy fork"),
                ("bungeecord", "BungeeCord", "Classic multi-server network proxy"),
                ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
            ],
            3 => vec![
                ("geyser", "GeyserMC Standalone", "Cross-play bridge for Bedrock clients"),
                ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
            ],
            _ => {
                get_all_softwares()
                    .into_iter()
                    .map(|s| (s.id(), s.name(), s.description()))
                    .collect()
            }
        };

        println!();
        println!("{}", "Select server software:".cyan().bold());
        let item_labels: Vec<String> = software_choices
            .iter()
            .map(|(_, name, desc)| format!("{:<20} - {}", name, desc))
            .collect();

        let choice_idx = Select::with_theme(&theme)
            .with_prompt("Software")
            .items(&item_labels)
            .default(0)
            .interact()?;

        software_choices[choice_idx].0.to_string()
    } else {
        "paper".to_string()
    };

    let software = find_software(&selected_software_id).ok_or_else(|| {
        CraftError::UnknownSoftware(selected_software_id.clone())
    })?;

    // 3. Determine target version
    let bundled = software.bundled_versions();
    let default_version = bundled.first().cloned().unwrap_or_else(|| "latest".to_string());

    let version = if let Some(v) = version_input {
        if v == "latest" {
            default_version
        } else {
            v.to_string()
        }
    } else if is_tty {
        println!();
        println!("{}", "Select software version:".cyan().bold());
        let mut version_options = vec![
            format!("latest (Recommended: {})", default_version),
        ];
        for b in bundled.iter().take(5) {
            if b != &default_version {
                version_options.push(b.clone());
            }
        }
        version_options.push("Custom version...".to_string());

        let ver_choice = Select::with_theme(&theme)
            .with_prompt("Version")
            .items(&version_options)
            .default(0)
            .interact()?;

        if ver_choice == 0 {
            default_version
        } else if ver_choice == version_options.len() - 1 {
            Input::with_theme(&theme)
                .with_prompt("Enter custom Minecraft version")
                .default(default_version)
                .interact_text()?
        } else {
            version_options[ver_choice].clone()
        }
    } else {
        default_version
    };

    // 4. Determine target directory
    let target_dir = if let Some(p) = custom_path {
        p
    } else {
        paths.servers_dir.join(&server_name)
    };

    if target_dir.exists() {
        let is_empty = fs::read_dir(&target_dir)
            .map(|mut r| r.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            return Err(CraftError::DirectoryNotEmpty(target_dir.to_string_lossy().to_string()));
        }
    } else {
        fs::create_dir_all(&target_dir)?;
    }

    // 5. Determine memory allocation
    let memory = if let Some(m) = memory_input {
        m.to_string()
    } else if is_tty {
        println!();
        println!("{}", "Select memory allocation (RAM):".cyan().bold());
        let mem_options = &[
            "2G (Standard)",
            "4G (Recommended for Paper / Fabric)",
            "8G (Heavy Modpacks / Folia / Large Worlds)",
            "16G (High-capacity Network / Multi-world)",
            "Custom memory...",
        ];
        let mem_choice = Select::with_theme(&theme)
            .with_prompt("Memory")
            .items(mem_options)
            .default(1)
            .interact()?;

        match mem_choice {
            0 => "2G".to_string(),
            1 => "4G".to_string(),
            2 => "8G".to_string(),
            3 => "16G".to_string(),
            _ => Input::with_theme(&theme)
                .with_prompt("Enter memory limit (e.g. 6G, 12G)")
                .default("4G".to_string())
                .interact_text()?,
        }
    } else {
        "2G".to_string()
    };

    // 6. JVM Garbage Collection Presets (for Java Edition)
    if software.edition() == ServerEdition::Java && !aikar && !zgc && !shenandoah {
        if is_tty {
            println!();
            println!("{}", "Select JVM Garbage Collection Preset:".cyan().bold());
            let gc_options = &[
                "Aikar G1GC (Industry standard for Paper/Spigot/Folia - Recommended)",
                "ZGC (Ultra-low latency for Java 21+)",
                "Shenandoah GC (Low-pause collector)",
                "Standard JVM Defaults",
            ];
            let gc_choice = Select::with_theme(&theme)
                .with_prompt("JVM GC Tuning")
                .items(gc_options)
                .default(0)
                .interact()?;

            match gc_choice {
                0 => aikar = true,
                1 => zgc = true,
                2 => shenandoah = true,
                _ => {}
            }
        } else {
            aikar = true;
        }
    }

    // 7. EULA Acceptance
    if !agree_eula {
        if is_tty {
            agree_eula = Confirm::with_theme(&theme)
                .with_prompt("Accept Minecraft EULA? (required to start server)")
                .default(true)
                .interact()?;
        } else {
            agree_eula = true;
        }
    }

    // 8. Download assets and set up server
    println!();
    println!("{}", format!("Setting up {} version {} in '{}'...", software.name(), version, target_dir.display()).cyan());

    let assets = software.get_assets(&version)?;
    let cache = CacheManager::new(paths);

    for asset in assets {
        cache.fetch_and_install(
            software.id(),
            &version,
            &asset.filename,
            &asset.url,
            &target_dir,
            asset.sha256.as_deref(),
        ).await?;
    }

    // Run post download hooks
    software.post_download(&target_dir, &version).await?;

    // Self-healing: ensure default server file exists (e.g. if provider downloaded versioned name)
    auto_heal_server_file(&target_dir, software.default_server_file());

    // Check Java version requirements for Java edition
    let mut java_path = None;
    if software.edition() == ServerEdition::Java {
        let jar_path = target_dir.join(software.default_server_file());
        if jar_path.exists() {
            if let Ok(req_ver) = get_jar_java_version(&jar_path) {
                println!("{}", format!("Detected bytecode requirement: Java {}", req_ver).dimmed());
                if let Ok(inst) = find_best_java(req_ver) {
                    println!("{}", format!("Selected Java runtime: Java {} ({})", inst.major_version, inst.path.display()).green());
                    java_path = Some(inst.path);
                }
            }
        }
    }

    // Prepare JVM tuning flags
    let mut flags = Vec::new();
    if aikar {
        flags.extend(vec![
            "-XX:+UseG1GC".to_string(),
            "-XX:+ParallelRefProcEnabled".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:G1NewSizePercent=30".to_string(),
            "-XX:G1MaxNewSizePercent=40".to_string(),
            "-XX:G1ReservePercent=20".to_string(),
            "-XX:G1HeapWastePercent=5".to_string(),
            "-XX:G1MixedGCCountTarget=4".to_string(),
            "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
            "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
            "-XX:G1RSetUpdatingPauseTimePercent=5".to_string(),
            "-XX:SurvivorRatio=32".to_string(),
            "-XX:+PerfDisableSharedMem".to_string(),
            "-XX:MaxTenuringThreshold=1".to_string(),
            "-Dusing.aikars.flags=https://mcflags.emc.gs".to_string(),
            "-Daikars.new.flags=true".to_string(),
        ]);
        println!("{}", "Applied Aikar G1GC JVM flags.".cyan());
    } else if zgc {
        flags.extend(vec![
            "-XX:+UseZGC".to_string(),
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
        ]);
        println!("{}", "Applied ZGC low-latency JVM flags.".cyan());
    } else if shenandoah {
        flags.extend(vec![
            "-XX:+UseShenandoahGC".to_string(),
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
        ]);
        println!("{}", "Applied Shenandoah GC JVM flags.".cyan());
    }

    if let Some(custom) = jvm_flags {
        flags.extend(custom);
    }

    let final_jvm_flags = if flags.is_empty() { None } else { Some(flags) };

    // Generate start scripts
    software.generate_start_script_with_flags(&target_dir, &version, java_path.as_deref(), &memory, final_jvm_flags.as_deref())?;

    // Handle EULA
    if agree_eula {
        let eula_file = target_dir.join("eula.txt");
        let _ = fs::write(eula_file, "eula=true\n");
        println!("{}", "EULA accepted automatically.".green());
    }

    // Register server
    let server_config = ServerConfig {
        name: server_name.clone(),
        path: target_dir.clone(),
        software: software.id().to_string(),
        version: version.clone(),
        auto: false,
        java_path,
        memory: Some(memory.to_string()),
        port: None,
        jvm_args: final_jvm_flags,
        created_at: Some(chrono::Utc::now()),
        backup_method: None,
    };

    let mut registry = ServersRegistry::load(paths)?;
    let _ = registry.add(server_config);
    registry.save(paths)?;

    println!("{}", format!("Server '{}' successfully installed!", server_name).green().bold());

    // 9. Startup decision
    if no_start {
        println!("{}", format!("Server configured without starting. Start anytime with 'craft run {}'.", server_name).dimmed());
        return Ok(());
    }

    let start_choice = if is_tty {
        println!();
        let start_options = &[
            "[1] Background Daemon (Runs 24/7 supervisor in background)",
            "[2] Foreground Terminal (Interactive console in current session)",
            "[3] Do not start yet (Exit)",
        ];
        Select::with_theme(&theme)
            .with_prompt("Startup mode")
            .items(start_options)
            .default(0)
            .interact()?
    } else {
        0 // Background Daemon by default in non-interactive
    };

    match start_choice {
        0 => {
            println!("{}", format!("Starting '{}' via supervisor daemon...", server_name).cyan());
            DaemonClient::ensure_daemon_started(paths).await?;
            let mut client = DaemonClient::connect(paths).await?;
            client.start_server(&target_dir).await?;
            println!("{}", format!("Server '{}' is running in the background!", server_name).green().bold());
            println!("{}", format!("Use 'craft view {}' to attach to its live console.", server_name).dimmed());
            Ok(())
        }
        1 => {
            println!("{}", format!("Starting server '{}' in foreground...", server_name).cyan());
            let run_res = run_foreground_server(&target_dir).await;

            if tmp {
                println!("{}", "Temporary server: Cleaning up files...".yellow());
                let _ = fs::remove_dir_all(&target_dir);
                let mut reg = ServersRegistry::load(paths)?;
                reg.remove(&target_dir);
                let _ = reg.save(paths);
                println!("{}", "Temporary server removed.".dimmed());
            }

            run_res
        }
        _ => {
            println!("{}", format!("Server '{}' is ready. Start anytime with 'craft run {}'.", server_name, server_name).dimmed());
            Ok(())
        }
    }
}
