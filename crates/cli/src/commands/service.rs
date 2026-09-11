use colored::Colorize;
use craft_core::{kill_process, read_pid_file, CraftError, CraftPaths, Result};
use craft_daemon::{DaemonClient, DaemonServer};
use crate::cli::ServiceCommands;

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
                println!("{}", "Craft service daemon started successfully in the background.".green());
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
                return Err(CraftError::Other("Could not read daemon PID file".to_string()));
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
                println!("{}", format!("Craft service daemon is RUNNING (PID: {})", pid).green().bold());
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    let running = client.get_running().await.unwrap_or_default();
                    println!("Active servers managed: {}", running.len());
                    for s in running {
                        println!("  - {}", s.display());
                    }
                }
            } else {
                println!("{}", "Craft service daemon is STOPPED.".red().bold());
            }
        }
    }

    Ok(())
}
