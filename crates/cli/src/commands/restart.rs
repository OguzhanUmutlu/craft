use std::io::IsTerminal;
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_restart(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    force: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let mut resolved_name = name_arg.to_string();

    if resolved_name.is_empty() && custom_path.is_none() && std::io::stdin().is_terminal() {
        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            println!("{}", "No servers registered. Use 'craft new' to create one.".yellow());
            return Ok(());
        }

        let running_paths = if DaemonClient::is_daemon_running(paths) {
            if let Ok(mut client) = DaemonClient::connect(paths).await {
                client.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let running_servers: Vec<_> = registry.servers.iter()
            .filter(|s| running_paths.contains(&s.path)
                || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false)
                || craft_core::is_server_locked(&s.path))
            .collect();

        if running_servers.is_empty() {
            println!("{}", "No running servers found to restart.".yellow());
            println!("Select a registered server to start:");
            let server_names: Vec<String> = registry.servers.iter()
                .map(|s| format!("{} [STOPPED - {} {}]", s.name, s.software, s.version))
                .collect();
            let idx = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select server")
                .items(&server_names)
                .default(0)
                .interact()?;
            resolved_name = registry.servers[idx].name.clone();
        } else {
            let server_names: Vec<String> = running_servers.iter()
                .map(|s| format!("{} [RUNNING - {} {}]", s.name, s.software, s.version))
                .collect();
            let idx = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select server to restart")
                .items(&server_names)
                .default(0)
                .interact()?;
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
    let server_entry = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!(
            "Server at '{}' is not registered. Use 'craft ls' to see servers.",
            server_path.display()
        ))
    })?;

    DaemonClient::ensure_daemon_started(paths).await?;
    let mut client = DaemonClient::connect(paths).await?;

    println!("{}", format!("Stopping server '{}'...", server_entry.name).yellow());
    let _ = client.stop_server(&server_path, force).await;

    // Small pause to allow socket and port release
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    println!("{}", format!("Starting server '{}'...", server_entry.name).cyan());
    client.start_server(&server_path).await?;

    println!("{}", format!("Server '{}' restarted successfully in the background.", server_entry.name).green().bold());
    println!("{}", format!("Use 'craft view {}' to attach to its live console.", server_entry.name).dimmed());

    Ok(())
}
