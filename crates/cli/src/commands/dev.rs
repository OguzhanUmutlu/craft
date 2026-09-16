use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_net::RconClient;

#[derive(clap::Subcommand, Debug, Clone)]
pub enum DevAction {
    /// Link an external development build JAR (e.g. build/libs/*.jar) to server plugins/mods
    Link {
        /// Absolute or relative path to the compiled .jar file
        jar: PathBuf,
        /// Target folder ("plugins" or "mods", defaults to "plugins")
        #[arg(short, long)]
        folder: Option<String>,
    },
    /// Configure JVM JDWP remote debugging (port 5005) for IDE attachment
    Debug {
        /// Remote debugging port (default 5005)
        #[arg(short, long, default_value_t = 5005)]
        port: u16,
        /// Disable remote debugging
        #[arg(long)]
        disable: bool,
    },
    /// Trigger an immediate in-game reload over RCON (reload confirm / datapack reload)
    Reload {
        /// Custom in-game reload command (default: "reload confirm")
        #[arg(short, long)]
        command: Option<String>,
    },
}

pub async fn handle_dev(
    server_name: &str,
    action: DevAction,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server_name), true)?;
    let mut registry = ServersRegistry::load(paths)?;

    match action {
        DevAction::Link { jar, folder } => {
            let canonical_jar = match jar.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    return Err(CraftError::Other(format!(
                        "Source JAR file '{}' does not exist.",
                        jar.display()
                    )));
                }
            };

            let target_server = registry
                .servers
                .iter()
                .find(|s| s.path == server_path || s.name.eq_ignore_ascii_case(server_name))
                .cloned();

            let caps = target_server
                .as_ref()
                .map(|s| craft_providers::get_content_capabilities(&s.software))
                .unwrap_or_default();

            if !caps.plugins && !caps.mods {
                return Err(CraftError::Other(format!(
                    "Server '{}' does not support plugins or mods.",
                    server_name
                )));
            }

            let folder_name = match folder.as_deref() {
                Some("mods") => {
                    if !caps.mods {
                        return Err(CraftError::Other(format!(
                            "Server '{}' does not support mods. Mods only exist in modded server softwares.",
                            server_name
                        )));
                    }
                    "mods"
                }
                Some("plugins") => {
                    if !caps.plugins {
                        return Err(CraftError::Other(format!(
                            "Server '{}' does not support plugins. Plugins only exist in non-vanilla server softwares.",
                            server_name
                        )));
                    }
                    "plugins"
                }
                Some(other) => other,
                None => {
                    if caps.mods && !caps.plugins {
                        "mods"
                    } else {
                        "plugins"
                    }
                }
            };
            let target_dir = server_path.join(folder_name);
            std::fs::create_dir_all(&target_dir)?;

            let file_name = canonical_jar
                .file_name()
                .ok_or_else(|| CraftError::Other("Invalid JAR filename".to_string()))?;
            let dest_link = target_dir.join(file_name);

            if dest_link.exists() {
                let _ = std::fs::remove_file(&dest_link);
            }

            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&canonical_jar, &dest_link)
                    .map_err(|e| CraftError::Other(format!("Failed to create symlink: {}", e)))?;
            }

            #[cfg(windows)]
            {
                if let Err(_) = std::os::windows::fs::symlink_file(&canonical_jar, &dest_link) {
                    std::fs::copy(&canonical_jar, &dest_link)
                        .map_err(|e| CraftError::Other(format!("Failed to link/copy JAR: {}", e)))?;
                }
            }

            println!(
                "{}: Linked development build '{}' -> '{}/{}'!",
                "Success".green().bold(),
                canonical_jar.display().to_string().cyan(),
                server_name,
                dest_link.display().to_string().yellow()
            );
            Ok(())
        }
        DevAction::Debug { port, disable } => {
            let (target_server_config, target_jvm_args) = {
                let server = registry
                    .servers
                    .iter_mut()
                    .find(|s| s.path == server_path || s.name.eq_ignore_ascii_case(server_name))
                    .ok_or_else(|| CraftError::ServerNotFound(server_name.to_string()))?;

                if disable {
                    server.jdwp_debug_port = None;
                    if let Some(ref mut args) = server.jvm_args {
                        args.retain(|a| !a.contains("jdwp="));
                    }
                    (server.clone(), None)
                } else {
                    server.jdwp_debug_port = Some(port);
                    let jdwp_flag = format!(
                        "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:{}",
                        port
                    );
                    let mut args = server.jvm_args.clone().unwrap_or_default();
                    args.retain(|a| !a.contains("jdwp="));
                    args.push(jdwp_flag);
                    server.jvm_args = Some(args.clone());
                    (server.clone(), Some(args))
                }
            };

            registry.save(paths)?;
            regenerate_server_script(&target_server_config, target_jvm_args.as_deref())?;

            if disable {
                println!("{}: Disabled remote JDWP debugging for '{}'.", "Success".green().bold(), server_name);
            } else {
                println!("{}: Enabled JDWP remote debugging on port {} for '{}'!", "Success".green().bold(), port.to_string().cyan().bold(), server_name);
                println!("\n{}", "=== IDE Debugger Configuration ===".cyan().bold());
                println!("  Port:      {}", port.to_string().yellow().bold());
                println!("  Host:      {}", "localhost / 127.0.0.1".yellow());
                println!("  Transport: dt_socket (Attach mode)");
                println!("\n{}", "--- VS Code launch.json Snippet ---".dimmed());
                println!(
                    r#"{{
  "type": "java",
  "name": "Attach to Craft: {}",
  "request": "attach",
  "hostName": "localhost",
  "port": {}
}}"#,
                    server_name, port
                );
                println!("\n{}", "--- IntelliJ IDEA Remote JVM Debug ---".dimmed());
                println!("  Run -> Edit Configurations -> '+' -> Remote JVM Debug -> Port: {}\n", port);
            }
            Ok(())
        }
        DevAction::Reload { command } => {
            let props_path = server_path.join("server.properties");
            let cmd = command.unwrap_or_else(|| "reload confirm".to_string());

            let props = craft_core::ServerProperties::load(&props_path).unwrap_or_default();
            let rcon_enabled = props.get_bool("enable-rcon").unwrap_or(false);
            let rcon_port = props.get_u16("rcon.port").unwrap_or(25575);
            let rcon_pass = props.get("rcon.password").unwrap_or("craft");

            if !rcon_enabled {
                return Err(CraftError::Other(
                    "RCON is not enabled in server.properties. Run 'craft prop set <server> enable-rcon true' or use the TUI properties editor to enable it.".to_string(),
                ));
            }

            let mut client = RconClient::connect("127.0.0.1", rcon_port, rcon_pass).await?;
            let resp = client.send_command(&cmd).await?;
            println!("{}: Dispatched '{}' over RCON.", "Craft".cyan().bold(), cmd.white().bold());
            if !resp.trim().is_empty() {
                println!("Response: {}", resp.green());
            }
            Ok(())
        }
    }
}

fn regenerate_server_script(server: &craft_core::ServerConfig, jvm_args: Option<&[String]>) -> Result<()> {
    if let Some(software) = craft_providers::find_software(&server.software) {
        let memory = server.memory.as_deref().unwrap_or("2G");
        software.generate_start_script_with_flags(
            &server.path,
            &server.version,
            server.java_path.as_deref(),
            memory,
            jvm_args,
        )?;
    }
    Ok(())
}
