use std::io::IsTerminal;
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_stop(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    force: bool,
    all: bool,
    paths: &CraftPaths,
) -> Result<()> {
    if !DaemonClient::is_daemon_running(paths) {
        let server_path_opt = if !name_arg.is_empty() || custom_path.is_some() {
            paths.resolve_server_path(
                custom_path.as_deref(),
                if name_arg.is_empty() { None } else { Some(name_arg) },
                true,
            ).ok()
        } else {
            None
        };

        if let Some(ref sp) = server_path_opt {
            if let Some(pid) = craft_core::get_server_running_pid(sp) {
                println!("{}", format!("Stopping server at '{}' (PID: {})...", sp.display(), pid).yellow());
                craft_core::kill_process(pid, force)?;
                for _ in 0..10 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    if !craft_core::is_process_running(pid) {
                        break;
                    }
                }
                if !craft_core::is_process_running(pid) {
                    craft_core::remove_pid_file(sp.join(".server.pid"));
                }
                println!("{}", format!("Server '{}' stopped successfully.", sp.display()).green());
                return Ok(());
            }
        }

        println!("{}", "Craft daemon is not running. No background servers active.".yellow());
        return Ok(());
    }

    let mut client = DaemonClient::connect(paths).await?;

    if all {
        let running = client.get_running().await.unwrap_or_default();
        if running.is_empty() {
            println!("{}", "No running servers found to stop.".yellow());
            return Ok(());
        }
        for p in running {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            println!("{}", format!("Stopping server '{}'...", name).yellow());
            let _ = client.stop_server(&p, force).await;
            println!("{}", format!("Server '{}' stopped.", name).green());
        }
        return Ok(());
    }

    let mut resolved_name = name_arg.to_string();

    if resolved_name.is_empty() && custom_path.is_none() && std::io::stdin().is_terminal() {
        let registry = ServersRegistry::load(paths)?;
        let running_paths = client.get_running().await.unwrap_or_default();
        let running_servers: Vec<_> = registry.servers.iter()
            .filter(|s| running_paths.contains(&s.path) || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false))
            .collect();

        if running_servers.is_empty() {
            println!("{}", "No running servers currently detected.".yellow());
            return Ok(());
        }

        let mut options: Vec<String> = running_servers.iter()
            .map(|s| format!("{:<20} [RUNNING - {} {}]", s.name, s.software, s.version))
            .collect();
        options.push("[Stop All Running Servers]".to_string());

        let idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select server to stop")
            .items(&options)
            .default(0)
            .interact()?;

        if idx == options.len() - 1 {
            for s in running_servers {
                println!("{}", format!("Stopping server '{}'...", s.name).yellow());
                let _ = client.stop_server(&s.path, force).await;
                println!("{}", format!("Server '{}' stopped.", s.name).green());
            }
            return Ok(());
        } else {
            resolved_name = running_servers[idx].name.clone();
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
            "Server '{}' is not registered. Use 'craft ls' to see servers.",
            server_path.display()
        )));
    }

    println!("{}", format!("Stopping server '{}'...", server_path.display()).yellow());
    client.stop_server(&server_path, force).await?;
    println!("{}", format!("Server '{}' stopped successfully.", server_path.display()).green());
    Ok(())
}
