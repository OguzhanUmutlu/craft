use crate::cli::ServiceCommands;
use colored::Colorize;
use craft_core::{kill_process, read_pid_file, CraftError, CraftPaths, Result};
use craft_daemon::{DaemonClient, DaemonServer};

pub async fn handle_service(action: ServiceCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        ServiceCommands::Start { foreground } => {
            if DaemonClient::is_daemon_running(paths) {
                println!("{}", "Craft service daemon is already running.".yellow());
                return Ok(());
            }

            if foreground {
                println!("{}", "Starting Craft daemon in foreground...".cyan());
                let server = DaemonServer::new(paths.clone());
                server.run().await?;
            } else {
                DaemonClient::ensure_daemon_started(paths).await?;
                println!(
                    "{}",
                    "Craft service daemon started successfully in the background.".green()
                );
            }
        }
        ServiceCommands::Stop => {
            if !DaemonClient::is_daemon_running(paths) {
                println!("{}", "Craft service daemon is not running.".yellow());
                return Ok(());
            }

            if let Some(pid) = read_pid_file(&paths.pid_file) {
                kill_process(pid, false)?;
                println!("{}", "Craft service daemon stopped successfully.".green());
            } else {
                return Err(CraftError::Other(
                    "Could not read daemon PID file".to_string(),
                ));
            }
        }
        ServiceCommands::Restart => {
            if DaemonClient::is_daemon_running(paths) {
                if let Some(pid) = read_pid_file(&paths.pid_file) {
                    let _ = kill_process(pid, false);
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            DaemonClient::ensure_daemon_started(paths).await?;
            println!("{}", "Craft service daemon restarted successfully.".green());
        }
        ServiceCommands::Status => {
            if DaemonClient::is_daemon_running(paths) {
                let pid = read_pid_file(&paths.pid_file).unwrap_or(0);
                println!(
                    "{}",
                    format!("Craft service daemon is RUNNING (PID: {})", pid)
                        .green()
                        .bold()
                );
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    let running = client.get_running().await.unwrap_or_default();
                    println!("Active servers managed: {}", running.len());
                    for s in running {
                        println!("  - {}", s.display());
                    }

                    // Query circuit breakers
                    if let Ok(breakers) = client.get_circuit_breakers().await {
                        let active_breakers: Vec<_> = breakers
                            .iter()
                            .filter(|b| b.state.contains("Open") || b.state.contains("HalfOpen"))
                            .collect();
                        if !active_breakers.is_empty() {
                            println!("\n{}", "=== Circuit Breaker Activity ===".yellow().bold());
                            for b in active_breakers {
                                println!(
                                    "  - {} | State: {} | Consecutive Crashes: {}",
                                    b.server_name.yellow().bold(),
                                    b.state.red(),
                                    b.consecutive_crashes
                                );
                            }
                        }
                    }

                    // Query backup schedules
                    if let Ok(schedules) = client.get_backup_schedules().await {
                        let active: Vec<_> = schedules.iter().filter(|s| s.enabled).collect();
                        if !active.is_empty() {
                            println!("\n{}", "=== Automated Backup Schedules ===".cyan().bold());
                            for s in active {
                                let last = s.last_backup.as_deref().unwrap_or("Never");
                                println!(
                                    "  - {} | Schedule: {} | Retention: {} | Format: {} | Last: {}",
                                    s.server_name.green().bold(),
                                    s.schedule.cyan(),
                                    s.retention_count,
                                    s.format,
                                    last.dimmed()
                                );
                            }
                        }
                    }
                }
            } else {
                println!("{}", "Craft service daemon is STOPPED.".red().bold());
            }
        }
        ServiceCommands::Install => {
            let current_exe = std::env::current_exe().map_err(|e| {
                CraftError::Other(format!(
                    "Failed to determine current executable path: {}",
                    e
                ))
            })?;
            let exe_path = current_exe.to_string_lossy().to_string();

            #[cfg(target_os = "linux")]
            {
                let user_dirs = directories::UserDirs::new().ok_or_else(|| {
                    CraftError::Config("Unable to locate user home directory".to_string())
                })?;
                let service_dir = user_dirs
                    .home_dir()
                    .join(".config")
                    .join("systemd")
                    .join("user");
                std::fs::create_dir_all(&service_dir)?;

                let service_path = service_dir.join("craft.service");
                let unit = format!(
                    "[Unit]\n\
                    Description=Craft Minecraft Server Management Daemon\n\
                    After=network.target\n\n\
                    [Service]\n\
                    Type=simple\n\
                    ExecStart=\"{}\" service start --foreground\n\
                    Restart=always\n\
                    RestartSec=5\n\n\
                    [Install]\n\
                    WantedBy=default.target\n",
                    exe_path
                );
                std::fs::write(&service_path, unit)?;

                println!(
                    "{}",
                    format!("Wrote systemd user service to {}", service_path.display()).cyan()
                );

                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .status();
                let enable = std::process::Command::new("systemctl")
                    .args(["--user", "enable", "--now", "craft.service"])
                    .status();

                if let Ok(user) = std::env::var("USER") {
                    let _ = std::process::Command::new("loginctl")
                        .args(["enable-linger", &user])
                        .status();
                }

                if enable.map(|s| s.success()).unwrap_or(false) {
                    println!(
                        "{}",
                        "Craft service successfully installed and started via systemd user unit."
                            .green()
                            .bold()
                    );
                } else {
                    println!("{}", "Created systemd unit. Run 'systemctl --user enable --now craft.service' to start.".yellow());
                }
            }

            #[cfg(target_os = "macos")]
            {
                let user_dirs = directories::UserDirs::new().ok_or_else(|| {
                    CraftError::Config("Unable to locate user home directory".to_string())
                })?;
                let agents_dir = user_dirs.home_dir().join("Library").join("LaunchAgents");
                std::fs::create_dir_all(&agents_dir)?;

                let plist_path = agents_dir.join("com.craft.daemon.plist");
                let plist = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                    <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                    <plist version=\"1.0\">\n\
                    <dict>\n\
                        <key>Label</key>\n\
                        <string>com.craft.daemon</string>\n\
                        <key>ProgramArguments</key>\n\
                        <array>\n\
                            <string>{}</string>\n\
                            <string>service</string>\n\
                            <string>start</string>\n\
                            <string>--foreground</string>\n\
                        </array>\n\
                        <key>RunAtLoad</key>\n\
                        <true/>\n\
                        <key>KeepAlive</key>\n\
                        <true/>\n\
                    </dict>\n\
                    </plist>\n",
                    exe_path
                );
                std::fs::write(&plist_path, plist)?;
                println!(
                    "{}",
                    format!("Wrote LaunchAgent to {}", plist_path.display()).cyan()
                );

                let load = std::process::Command::new("launchctl")
                    .args(["load", "-w", &plist_path.to_string_lossy()])
                    .status();

                if load.map(|s| s.success()).unwrap_or(false) {
                    println!(
                        "{}",
                        "Craft service successfully registered with macOS launchd."
                            .green()
                            .bold()
                    );
                } else {
                    println!("{}", "Created LaunchAgent. Run 'launchctl load -w ~/Library/LaunchAgents/com.craft.daemon.plist' to start.".yellow());
                }
            }

            #[cfg(target_os = "windows")]
            {
                let task_cmd = format!("\"{}\" service start --foreground", exe_path);
                let status = std::process::Command::new("schtasks")
                    .args([
                        "/Create",
                        "/SC",
                        "ONLOGON",
                        "/TN",
                        "CraftDaemon",
                        "/TR",
                        &task_cmd,
                        "/F",
                    ])
                    .status();

                if status.map(|s| s.success()).unwrap_or(false) {
                    println!("{}", "Craft service scheduled task 'CraftDaemon' successfully registered for logon.".green().bold());
                } else {
                    return Err(CraftError::Other("Failed to create Windows Scheduled Task. Run terminal as Administrator or check permissions.".to_string()));
                }
            }
        }
        ServiceCommands::Uninstall => {
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "disable", "--now", "craft.service"])
                    .status();

                if let Some(user_dirs) = directories::UserDirs::new() {
                    let service_path = user_dirs
                        .home_dir()
                        .join(".config")
                        .join("systemd")
                        .join("user")
                        .join("craft.service");
                    if service_path.exists() {
                        let _ = std::fs::remove_file(&service_path);
                    }
                }
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .status();
                println!(
                    "{}",
                    "Craft systemd user service uninstalled successfully."
                        .green()
                        .bold()
                );
            }

            #[cfg(target_os = "macos")]
            {
                if let Some(user_dirs) = directories::UserDirs::new() {
                    let plist_path = user_dirs
                        .home_dir()
                        .join("Library")
                        .join("LaunchAgents")
                        .join("com.craft.daemon.plist");
                    if plist_path.exists() {
                        let _ = std::process::Command::new("launchctl")
                            .args(["unload", "-w", &plist_path.to_string_lossy()])
                            .status();
                        let _ = std::fs::remove_file(&plist_path);
                    }
                }
                println!(
                    "{}",
                    "Craft macOS LaunchAgent uninstalled successfully."
                        .green()
                        .bold()
                );
            }

            #[cfg(target_os = "windows")]
            {
                let status = std::process::Command::new("schtasks")
                    .args(["/Delete", "/TN", "CraftDaemon", "/F"])
                    .status();

                if status.map(|s| s.success()).unwrap_or(false) {
                    println!(
                        "{}",
                        "Craft service scheduled task 'CraftDaemon' uninstalled successfully."
                            .green()
                            .bold()
                    );
                } else {
                    println!(
                        "{}",
                        "Scheduled task was either not found or failed to delete.".yellow()
                    );
                }
            }
        }
        ServiceCommands::CircuitBreakers { reset } => {
            if !DaemonClient::is_daemon_running(paths) {
                println!("{}", "Craft service daemon is not running.".yellow());
                return Ok(());
            }

            let mut client = DaemonClient::connect(paths).await?;

            if let Some(target) = reset {
                let target_path = paths.resolve_server_path(None, Some(&target), true)?;
                client.reset_circuit_breaker(&target_path).await?;
                println!(
                    "{}",
                    format!(
                        "[OK] Reset crash circuit breaker for server '{}' ({})",
                        target,
                        target_path.display()
                    )
                    .green()
                    .bold()
                );
            } else {
                let breakers = client.get_circuit_breakers().await?;
                if breakers.is_empty() {
                    println!("{}", "No circuit breakers registered or active.".green());
                    return Ok(());
                }

                println!(
                    "{}",
                    "=== Server Crash Circuit Breakers ===".cyan().bold()
                );
                for b in breakers {
                    let tripped_info = b.last_tripped.as_deref().unwrap_or("None");
                    let state_color = if b.state.contains("Open") {
                        b.state.red().bold()
                    } else if b.state.contains("HalfOpen") {
                        b.state.yellow().bold()
                    } else {
                        b.state.green()
                    };
                    println!(
                        "  Server:      {}\n  Path:        {}\n  State:       {}\n  Crashes:     {}\n  Last Trip:   {}\n",
                        b.server_name.cyan().bold(),
                        b.path.display(),
                        state_color,
                        b.consecutive_crashes,
                        tripped_info.dimmed()
                    );
                }
                println!("Tip: Run 'craft service breakers --reset <server>' to clear a tripped breaker.");
            }
        }
    }

    Ok(())
}
