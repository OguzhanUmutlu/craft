use colored::Colorize;
use craft_core::{
    auto_heal_server_file, find_best_java, get_jar_java_version, get_server_running_pid,
    is_process_running, read_pid_file, remove_pid_file, CraftError, CraftPaths, Result,
    ServerProperties, ServersRegistry,
};
use craft_providers::{find_software, ServerEdition};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;

pub async fn handle_fix(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if name_arg.is_empty() {
            None
        } else {
            Some(name_arg)
        },
        true,
    )?;

    if !server_path.exists() {
        return Err(CraftError::InvalidPath(
            server_path.to_string_lossy().to_string(),
        ));
    }

    let mut registry = ServersRegistry::load(paths)?;
    let server_config = registry
        .find_by_path(&server_path)
        .cloned()
        .ok_or_else(|| {
            CraftError::ServerNotFound(format!(
                "Server at '{}' is not registered.",
                server_path.display()
            ))
        })?;

    println!(
        "{}",
        format!(
            "Diagnosing and repairing server '{}'...",
            server_path.display()
        )
        .cyan()
    );

    let software = find_software(&server_config.software)
        .ok_or_else(|| CraftError::UnknownSoftware(server_config.software.clone()))?;

    let mut fixed_items = Vec::new();
    let mut warnings = Vec::new();

    // 1. Stale server.lock and PID file check
    let lock_file = server_path.join("server.lock");
    let is_running = if let Some(pid) = get_server_running_pid(&server_path) {
        is_process_running(pid)
    } else {
        false
    };

    if !is_running {
        // Clean up stale server.lock if process is not alive
        if lock_file.exists() {
            let _ = fs::remove_file(&lock_file);
            fixed_items.push("Removed stale server.lock (no active server process detected)".to_string());
        }

        // Clean up stale pid file if process is not alive
        if let Some(stale_pid) = read_pid_file(&server_path) {
            if !is_process_running(stale_pid) {
                let _ = remove_pid_file(&server_path);
                fixed_items.push(format!("Removed stale server.pid (PID {})", stale_pid));
            }
        }
    }

    // 2. Ensure server file exists (restore from archive if missing)
    let expected_file = software.default_server_file();
    if !server_path.join(expected_file).exists() {
        if let Some(source) = auto_heal_server_file(&server_path, expected_file) {
            fixed_items.push(format!("Restored {} from '{}'", expected_file, source));
        }
    }

    // 3. Minecraft EULA acceptance check
    if software.game_id() == "minecraft" && software.edition() == ServerEdition::Java {
        let eula_path = server_path.join("eula.txt");
        let needs_eula = if eula_path.exists() {
            fs::read_to_string(&eula_path)
                .map(|content| content.contains("eula=false"))
                .unwrap_or(true)
        } else {
            true
        };

        if needs_eula {
            let eula_content = "# By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=true\n";
            fs::write(&eula_path, eula_content)?;
            fixed_items.push("Accepted Minecraft EULA in eula.txt".to_string());
        }
    }

    // 4. Inspect Java and re-detect if needed
    let mut resolved_java = server_config.java_path.clone();
    if software.edition() == ServerEdition::Java || software.edition() == ServerEdition::Proxy {
        let jar = server_path.join(software.default_server_file());
        if jar.exists() {
            if let Ok(ver) = get_jar_java_version(&jar) {
                let java_needed = if let Some(ref current_java) = resolved_java {
                    !current_java.exists()
                } else {
                    true
                };

                if java_needed {
                    if let Ok(best) = find_best_java(ver) {
                        resolved_java = Some(best.path);
                        fixed_items.push(format!("Configured Java {} runtime", best.major_version));
                    }
                }
            }
        }
    }

    // 5. Regenerate start script if missing or Java runtime updated
    let script_name = if cfg!(windows) {
        "start.cmd"
    } else {
        "start.sh"
    };
    let script_path = server_path.join(script_name);
    let mut script_needs_regen = !script_path.exists();
    if resolved_java != server_config.java_path {
        script_needs_regen = true;
    }
    if script_needs_regen {
        let memory = server_config.memory.as_deref().unwrap_or("2G");
        software.generate_start_script_with_flags(
            &server_path,
            &server_config.version,
            resolved_java.as_deref(),
            memory,
            server_config.jvm_args.as_deref(),
        )?;
        fixed_items.push(format!("Updated {}", script_name));
    }

    // 6. Fix Unix executable permissions across all game archetypes
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executables = [
            script_path.clone(),
            server_path.join("bedrock_server"),
            server_path.join("bin").join("php7").join("bin").join("php"),
            server_path.join("bin").join("x64").join("factorio"),
            server_path.join("TerrariaServer.bin.x86_64"),
            server_path.join("valheim_server.x86_64"),
            server_path.join("PalServer.sh"),
            server_path.join(software.default_server_file()),
        ];

        for bin in &executables {
            if bin.exists() {
                if let Ok(meta) = fs::metadata(bin) {
                    let mut perms = meta.permissions();
                    if perms.mode() & 0o111 != 0o111 {
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(bin, perms);
                        let name = bin.file_name().and_then(|n| n.to_str()).unwrap_or("binary");
                        fixed_items.push(format!("Set executable permissions on {}", name));
                    }
                }
            }
        }
    }

    // 7. Port collision detection
    let server_props = server_path.join("server.properties");
    if server_props.exists() {
        if let Ok(props) = ServerProperties::load(&server_path) {
            if let Some(port_str) = props.get("server-port") {
                if let Ok(port) = port_str.parse::<u16>() {
                    // Check if another registered server uses this port
                    for other in &registry.servers {
                        if other.path != server_path {
                            let other_props = other.path.join("server.properties");
                            if other_props.exists() {
                                if let Ok(op) = ServerProperties::load(&other.path) {
                                    if op.get("server-port") == Some(port_str) {
                                        warnings.push(format!(
                                            "Port {} collides with another registered server '{}' at '{}'",
                                            port, other.name, other.path.display()
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    // If server is not running, test if port is already bound on host
                    if !is_running {
                        if TcpListener::bind(("127.0.0.1", port)).is_err() {
                            warnings.push(format!(
                                "Port {} is currently bound by another process on this machine.",
                                port
                            ));
                        }
                    }
                }
            }
        }
    }

    // 8. Ghost registry verification
    for s in &registry.servers {
        if !s.path.exists() {
            warnings.push(format!(
                "Registry entry '{}' points to a non-existent directory '{}'",
                s.name, s.path.display()
            ));
        }
    }

    // 9. Update registry config
    if let Some(s) = registry.servers.iter_mut().find(|s| s.path == server_path) {
        s.java_path = resolved_java;
    }
    registry.save(paths)?;

    if fixed_items.is_empty() && warnings.is_empty() {
        println!(
            "{}",
            "No issues found. Server configuration is healthy.".green()
        );
    } else {
        if !fixed_items.is_empty() {
            println!("{}", "Server repaired successfully:".green().bold());
            for item in &fixed_items {
                println!("  [FIXED] {}", item);
            }
        }

        if !warnings.is_empty() {
            println!("\n{}", "Diagnostic Warnings:".yellow().bold());
            for warn in &warnings {
                println!("  [WARN] {}", warn.yellow());
            }
        }
    }

    Ok(())
}
