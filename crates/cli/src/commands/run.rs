use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use colored::Colorize;
use tokio::process::Command;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

use std::io::IsTerminal;
use dialoguer::{theme::ColorfulTheme, Select};

pub async fn handle_run(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    here: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let mut resolved_name = name_arg.to_string();

    if resolved_name.is_empty() && custom_path.is_none() && std::io::stdin().is_terminal() {
        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            println!("{}", "No servers registered. Use 'craft new' to create one.".yellow());
            return Ok(());
        }

        // Check if current directory is a server
        let in_server_dir = std::env::current_dir().ok().and_then(|cwd| registry.find_by_path(&cwd).map(|s| s.name.clone()));
        if let Some(cur_name) = in_server_dir {
            resolved_name = cur_name;
        } else {
            println!("{}", "=== Select Server to Run ===".cyan().bold());
            let running_paths = if DaemonClient::is_daemon_running(paths) {
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    client.get_running().await.unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let server_items: Vec<String> = registry.servers.iter().map(|s| {
                let is_running = running_paths.contains(&s.path)
                    || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false)
                    || craft_core::is_server_locked(&s.path);
                let status = if is_running {
                    "[ALREADY RUNNING]"
                } else {
                    "[STOPPED]"
                };
                format!("{:<20} {:<18} ({} {})", s.name, status, s.software, s.version)
            }).collect();

            let idx = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select server")
                .items(&server_items)
                .default(0)
                .interact()?;

            resolved_name = registry.servers[idx].name.clone();
        }
    }

    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if resolved_name.is_empty() { None } else { Some(&resolved_name) },
        true,
    )?;

    if !server_path.exists() {
        return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
    }

    let registry = ServersRegistry::load(paths)?;
    if registry.find_by_path(&server_path).is_none() {
        return Err(CraftError::ServerNotFound(format!(
            "Server at '{}' is not registered. Use 'craft load' or 'craft new' first.",
            server_path.display()
        )));
    }

    if here {
        run_foreground_server(&server_path).await
    } else {
        let registry = ServersRegistry::load(paths).ok();
        let expected_file = registry.as_ref()
            .and_then(|r| r.find_by_path(&server_path))
            .and_then(|s| craft_providers::find_software(&s.software))
            .map(|sw| sw.default_server_file())
            .unwrap_or("server.jar");
        let _ = craft_core::auto_heal_server_file(&server_path, expected_file);

        DaemonClient::ensure_daemon_started(paths).await?;
        let mut client = DaemonClient::connect(paths).await?;
        client.start_server(&server_path).await?;
        println!("{}", format!("Server at '{}' started successfully in the background!", server_path.display()).green());
        println!("{}", format!("Use 'craft view {}' to attach to its live console.", server_path.file_name().unwrap_or_default().to_string_lossy()).dimmed());
        Ok(())
    }
}

pub async fn run_foreground_server(server_path: &Path) -> Result<()> {
    let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.to_path_buf());
    let lock_guard = craft_core::ServerLockGuard::acquire(&canonical)?;

    // Self-healing: ensure server jar / binary is in place
    let paths = CraftPaths::new();
    let expected_file = paths.as_ref().ok().and_then(|p| {
        ServersRegistry::load(p).ok().and_then(|r| {
            r.find_by_path(server_path).and_then(|s| {
                craft_providers::find_software(&s.software).map(|sw| sw.default_server_file())
            })
        })
    }).unwrap_or("server.jar");

    if let Some(source) = craft_core::auto_heal_server_file(server_path, expected_file) {
        println!("{}", format!("Self-healing: Restored {} from '{}'", expected_file, source).yellow());
    }
    if expected_file != "server.jar" && !server_path.join(expected_file).exists() {
        if let Some(source) = craft_core::auto_heal_server_jar(server_path) {
            println!("{}", format!("Self-healing: Restored server.jar from '{}'", source).yellow());
        }
    }

    let script = if cfg!(windows) {
        server_path.join("start.cmd")
    } else {
        server_path.join("start.sh")
    };

    if !script.exists() {
        return Err(CraftError::Other(format!(
            "Start script not found at '{}'",
            script.display()
        )));
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&script) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&script, perms);
            }
        }
        let bedrock_bin = server_path.join("bedrock_server");
        if bedrock_bin.exists() {
            if let Ok(meta) = fs::metadata(&bedrock_bin) {
                let mut perms = meta.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&bedrock_bin, perms);
                }
            }
        }
    }

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd.exe");
        c.arg("/c").arg(&script);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(&script);
        c
    };

    cmd.current_dir(server_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn()
        .map_err(|e| CraftError::Process(format!("Failed to start server: {}", e)))?;

    if let Some(pid) = child.id() {
        let _ = lock_guard.record_pid(pid);
    }

    #[cfg(unix)]
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).ok();

    let status = tokio::select! {
        res = child.wait() => {
            res.map_err(|e| CraftError::Process(format!("Error waiting for server process: {}", e)))?
        }
        _ = async {
            #[cfg(unix)]
            if let Some(ref mut s) = sigint {
                s.recv().await;
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
        } => {
            println!("\r\n{}", "[Craft] Stop signal received. Waiting for server shutdown sequence to finish...".yellow().bold());
            tokio::select! {
                res = child.wait() => {
                    res.map_err(|e| CraftError::Process(format!("Error waiting for server process: {}", e)))?
                }
                _ = async {
                    #[cfg(unix)]
                    if let Some(ref mut s) = sigint {
                        s.recv().await;
                    } else {
                        let _ = tokio::signal::ctrl_c().await;
                    }
                    #[cfg(not(unix))]
                    let _ = tokio::signal::ctrl_c().await;
                } => {
                    println!("{}", "[Craft] Second interrupt received. Forcing immediate termination...".red().bold());
                    let _ = child.kill().await;
                    child.wait().await.map_err(|e| CraftError::Process(format!("Error after killing server: {}", e)))?
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                    println!("{}", "[Craft] Shutdown timed out. Forcing process kill...".red().bold());
                    let _ = child.kill().await;
                    child.wait().await.map_err(|e| CraftError::Process(format!("Error after killing server: {}", e)))?
                }
            }
        }
    };

    // Check EULA after exit if it failed because EULA was not accepted
    let eula_file = server_path.join("eula.txt");
    if eula_file.exists() {
        if let Ok(content) = fs::read_to_string(&eula_file) {
            if content.contains("eula=false") {
                println!("\n{}", "You need to accept the Mojang EULA to run this server.".yellow().bold());
                print!("{}", "Type 'agree' to accept the EULA and restart the server: ".yellow());
                let _ = io::stdout().flush();

                let mut input = String::new();
                let _ = io::stdin().read_line(&mut input);

                if input.trim().eq_ignore_ascii_case("agree") {
                    let updated = content.replace("eula=false", "eula=true");
                    let _ = fs::write(&eula_file, updated);
                    println!("{}", "EULA accepted. Restarting server...".green());
                    drop(lock_guard);
                    return Box::pin(run_foreground_server(server_path)).await;
                } else {
                    println!("{}", "EULA was not accepted. Server will not run.".red());
                    return Ok(());
                }
            }
        }
    }

    println!("{}", format!("\nServer process exited with code {:?}", status.code()).dimmed());
    println!("{}", "[Craft] Returning to Server Control menu...".cyan().dimmed());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    Ok(())
}
