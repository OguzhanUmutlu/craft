use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftPaths, Result, ServerConfig, ServersRegistry};
use craft_net::RconClient;
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};

pub async fn developer_tools_menu(server: &ServerConfig, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Developer Tools");
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let current_server = registry
            .servers
            .iter()
            .find(|s| s.path == server.path || s.name.eq_ignore_ascii_case(&server.name))
            .cloned()
            .unwrap_or_else(|| server.clone());

        let width = get_content_width(80);
        let debug_status = if let Some(port) = current_server.jdwp_debug_port {
            format!("[ENABLED (Port {})]", port).green().bold().to_string()
        } else {
            "[DISABLED]".dimmed().to_string()
        };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:        {:<20} | Platform: {}\r\n JDWP Debugger: {}\r\n Select a developer tool or scaffolding wizard:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("DEVELOPER TOOLS: {}", current_server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            current_server.name.white().bold(),
            current_server.software.cyan(),
            debug_status,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Scaffolding Templates (Paper, Velocity, Datapack)"),
            MenuEntry::new("2", "JVM JDWP Remote Debugger Setup"),
            MenuEntry::new("3", "Link Local Development Build (.jar)"),
            MenuEntry::new("4", "Instant In-Game Reload (RCON)"),
            MenuEntry::new("5", "Generate Dockerfile & Docker Compose"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Templates
                template_scaffolding_menu(paths).await?;
            }
            Some(1) => {
                // JDWP Debugger
                jdwp_setup_menu(&current_server, paths).await?;
            }
            Some(2) => {
                // Link local JAR
                link_local_jar_menu(&current_server).await?;
            }
            Some(3) => {
                // Instant In-Game Reload
                rcon_reload_menu(&current_server).await?;
            }
            Some(4) => {
                // Dockerize
                match crate::commands::dockerize::handle_dockerize(&current_server.name, paths) {
                    Ok(_) => {
                        show_modal_message(
                            "DOCKERIZED",
                            &[
                                format!("[OK] Generated Dockerfile and docker-compose.yml for '{}'!", current_server.name).green().bold().to_string(),
                                format!("Located in: {}", current_server.path.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("Failed to dockerize: {}", e)], true)?;
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn template_scaffolding_menu(_paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;
    let width = get_content_width(80);

    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select project template type to scaffold:\r\n{}",
        box_top(width).cyan().bold(),
        box_title("PROJECT TEMPLATES", width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        box_divider(width).dimmed(),
    );

    let entries = vec![
        MenuEntry::new("1", "Paper / Purpur / Spigot Plugin (Gradle + Java 21 / Kotlin)"),
        MenuEntry::new("2", "Velocity Proxy Plugin (Gradle + Java 21)"),
        MenuEntry::new("3", "Minecraft Datapack (pack.mcmeta + functions)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    match run_menu(&header, &entries, &mut selected)? {
        Some(0) => {
            if let Some(name) = run_input_prompt("PLUGIN NAME", "Enter plugin project name (e.g. MyAwesomePlugin):", None)? {
                crate::commands::template::handle_template(
                    crate::cli::TemplateCommands::Plugin {
                        name: name.trim().to_string(),
                        platform: "paper".to_string(),
                    },
                )?;
            }
        }
        Some(1) => {
            if let Some(name) = run_input_prompt("VELOCITY PLUGIN NAME", "Enter Velocity plugin project name:", None)? {
                crate::commands::template::handle_template(
                    crate::cli::TemplateCommands::Plugin {
                        name: name.trim().to_string(),
                        platform: "velocity".to_string(),
                    },
                )?;
            }
        }
        Some(2) => {
            if let Some(name) = run_input_prompt("DATAPACK NAME", "Enter datapack name (e.g. custom_crafting):", None)? {
                crate::commands::template::handle_template(
                    crate::cli::TemplateCommands::Datapack {
                        name: name.trim().to_string(),
                    },
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn jdwp_setup_menu(server: &ServerConfig, paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;
    let width = get_content_width(80);

    let is_enabled = server.jdwp_debug_port.is_some();
    let status_str = if let Some(p) = server.jdwp_debug_port {
        format!("ENABLED on Port {}", p).green().bold().to_string()
    } else {
        "DISABLED".yellow().bold().to_string()
    };

    let header = format!(
        "{}\r\n{}\r\n{}\r\n Server: {}\r\n Current Status: {}\r\n Configure JVM JDWP remote debugging for IDE attaching:\r\n{}",
        box_top(width).cyan().bold(),
        box_title("JVM JDWP DEBUGGER SETUP", width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        server.name.white().bold(),
        status_str,
        box_divider(width).dimmed(),
    );

    let entries = if is_enabled {
        vec![
            MenuEntry::new("1", "Disable Remote Debugging"),
            MenuEntry::new("2", "Change Debug Port"),
            MenuEntry::new("3", "View IDE Connection Snippets (VS Code / IntelliJ)"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ]
    } else {
        vec![
            MenuEntry::new("1", "Enable Remote Debugging (Default Port: 5005)"),
            MenuEntry::new("2", "Enable with Custom Port"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ]
    };

    match run_menu(&header, &entries, &mut selected)? {
        Some(0) => {
            if is_enabled {
                crate::commands::dev::handle_dev(
                    &server.name,
                    crate::commands::dev::DevAction::Debug { port: 5005, disable: true },
                    paths,
                ).await?;
                show_modal_message("DEBUGGER DISABLED", &["[OK] Remote debugging disabled and start scripts regenerated.".to_string()], false)?;
            } else {
                crate::commands::dev::handle_dev(
                    &server.name,
                    crate::commands::dev::DevAction::Debug { port: 5005, disable: false },
                    paths,
                ).await?;
                show_modal_message("DEBUGGER ENABLED", &["[OK] Enabled JDWP remote debugging on port 5005!".to_string()], false)?;
            }
        }
        Some(1) => {
            if is_enabled {
                if let Some(port_str) = run_input_prompt("DEBUG PORT", "Enter new port:", Some("5005"))? {
                    if let Ok(port) = port_str.trim().parse::<u16>() {
                        crate::commands::dev::handle_dev(
                            &server.name,
                            crate::commands::dev::DevAction::Debug { port, disable: false },
                            paths,
                        ).await?;
                        show_modal_message("PORT UPDATED", &[format!("[OK] Set JDWP debug port to {}!", port)], false)?;
                    }
                }
            } else {
                if let Some(port_str) = run_input_prompt("CUSTOM PORT", "Enter debug port:", Some("5005"))? {
                    if let Ok(port) = port_str.trim().parse::<u16>() {
                        crate::commands::dev::handle_dev(
                            &server.name,
                            crate::commands::dev::DevAction::Debug { port, disable: false },
                            paths,
                        ).await?;
                        show_modal_message("DEBUGGER ENABLED", &[format!("[OK] Enabled JDWP on port {}!", port)], false)?;
                    }
                }
            }
        }
        Some(2) if is_enabled => {
            let port = server.jdwp_debug_port.unwrap_or(5005);
            let lines = vec![
                format!("Debug Port: {}", port).cyan().bold().to_string(),
                "Host:       localhost / 127.0.0.1".to_string(),
                "Mode:       Attach (Socket)".to_string(),
                "".to_string(),
                "VS Code launch.json:".yellow().to_string(),
                format!(r#"{{ "type": "java", "name": "Attach", "request": "attach", "hostName": "localhost", "port": {} }}"#, port),
                "".to_string(),
                "IntelliJ IDEA:".yellow().to_string(),
                format!("Run -> Edit Configurations -> '+' -> Remote JVM Debug -> Port {}", port),
            ];
            show_modal_message("IDE CONFIGURATIONS", &lines, false)?;
        }
        _ => {}
    }
    Ok(())
}

async fn link_local_jar_menu(server: &ServerConfig) -> Result<()> {
    if let Some(path_str) = run_input_prompt(
        "LINK DEVELOPMENT JAR",
        "Enter path to your compiled plugin or mod JAR file:",
        None,
    )? {
        let jar_path = PathBuf::from(path_str.trim());
        if !jar_path.exists() {
            show_modal_message("FILE NOT FOUND", &[format!("File '{}' does not exist.", jar_path.display())], true)?;
            return Ok(());
        }

        let folder = match run_menu(
            "Select Destination Folder in Server:",
            &[
                MenuEntry::new("1", "plugins/ (Standard for Paper, Spigot, Velocity)"),
                MenuEntry::new("2", "mods/ (Fabric, Quilt, NeoForge)"),
                MenuEntry::new("0", "Cancel"),
            ],
            &mut 0,
        )? {
            Some(0) => "plugins",
            Some(1) => "mods",
            _ => return Ok(()),
        };

        let dest_dir = server.path.join(folder);
        let _ = std::fs::create_dir_all(&dest_dir);

        let resolved_jar = jar_path.canonicalize().unwrap_or_else(|_| jar_path.clone());
        let filename = resolved_jar.file_name().unwrap_or_default().to_os_string();
        let dest_path = dest_dir.join(&filename);

        #[cfg(unix)]
        {
            if dest_path.exists() {
                let _ = std::fs::remove_file(&dest_path);
            }
            if let Err(e) = std::os::unix::fs::symlink(&resolved_jar, &dest_path) {
                show_modal_message("ERROR", &[format!("Failed to create symlink: {}", e)], true)?;
                return Ok(());
            }
        }

        #[cfg(windows)]
        {
            if dest_path.exists() {
                let _ = std::fs::remove_file(&dest_path);
            }
            if let Err(_) = std::os::windows::fs::symlink_file(&resolved_jar, &dest_path) {
                let _ = std::fs::copy(&resolved_jar, &dest_path);
            }
        }

        show_modal_message(
            "JAR LINKED",
            &[
                format!("[OK] Linked '{}' -> '{}/{}'!", filename.to_string_lossy(), server.name, folder).green().bold().to_string(),
                "Your build artifact will now automatically be loaded on server restarts!".to_string(),
            ],
            false,
        )?;
    }
    Ok(())
}

async fn rcon_reload_menu(server: &ServerConfig) -> Result<()> {
    let props_path = server.path.join("server.properties");
    let props = craft_core::ServerProperties::load(&props_path).unwrap_or_default();

    if !props.get_bool("enable-rcon").unwrap_or(false) {
        show_modal_message(
            "RCON REQUIRED",
            &[
                "RCON remote console is disabled for this server.".yellow().to_string(),
                "Please enable RCON in 'Server Properties' before using in-game reload.".to_string(),
            ],
            true,
        )?;
        return Ok(());
    }

    let port = props.get_u16("rcon.port").unwrap_or(25575);
    let pass = props.get("rcon.password").unwrap_or("craft");

    let reload_cmds = vec![
        MenuEntry::new("1", "reload confirm (Paper / Purpur / Spigot)"),
        MenuEntry::new("2", "datapack reload (Datapack hot reload)"),
        MenuEntry::new("3", "Custom in-game command"),
        MenuEntry::new("0", "Cancel"),
    ];

    let mut sel = 0;
    if let Some(idx) = run_menu("Select In-Game Reload Command:", &reload_cmds, &mut sel)? {
        let cmd = match idx {
            0 => "reload confirm".to_string(),
            1 => "datapack reload".to_string(),
            2 => {
                if let Some(c) = run_input_prompt("RCON COMMAND", "Enter command:", Some("reload confirm"))? {
                    c
                } else {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        };

        let _ = print_in_place_status("SENDING RCON COMMAND", &[format!("Dispatching '{}'...", cmd)]);
        match RconClient::connect("127.0.0.1", port, pass).await {
            Ok(mut client) => {
                match client.send_command(&cmd).await {
                    Ok(resp) => {
                        show_modal_message(
                            "COMMAND EXECUTED",
                            &[
                                format!("[OK] Dispatched '{}' over RCON!", cmd).green().bold().to_string(),
                                format!("Response: {}", resp.trim()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("RCON ERROR", &[format!("Command failed: {}", e)], true)?;
                    }
                }
            }
            Err(e) => {
                show_modal_message("CONNECTION FAILED", &[format!("Could not connect to RCON on 127.0.0.1:{}: {}", port, e)], true)?;
            }
        }
    }
    Ok(())
}
