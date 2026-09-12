use std::io::IsTerminal;
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_view(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    paths: &CraftPaths,
) -> Result<()> {
    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other(
            "Craft daemon is not running. Start the server first with 'craft run <name>'.".to_string(),
        ));
    }

    let mut client = DaemonClient::connect(paths).await?;
    let mut resolved_name = name_arg.to_string();

    if resolved_name.is_empty() && custom_path.is_none() && std::io::stdin().is_terminal() {
        let registry = ServersRegistry::load(paths)?;
        let running_paths = client.get_running().await.unwrap_or_default();
        let running_servers: Vec<_> = registry.servers.iter()
            .filter(|s| running_paths.contains(&s.path) || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false))
            .collect();

        if running_servers.is_empty() {
            println!("{}", "No running servers currently detected to attach to.".yellow());
            return Ok(());
        }

        let options: Vec<String> = running_servers.iter()
            .map(|s| format!("{:<20} [RUNNING - {} {}]", s.name, s.software, s.version))
            .collect();

        let idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select running server to attach console")
            .items(&options)
            .default(0)
            .interact()?;

        resolved_name = running_servers[idx].name.clone();
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
    let entry = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!(
            "Server '{}' is not registered.",
            server_path.display()
        ))
    })?;

    let s_name = entry.name.clone();
    if std::io::stdout().is_terminal() {
        crate::commands::dashboard::screen::run_virtual_console(&s_name, &server_path, paths).await?;
    } else {
        println!("{}", format!("Attaching to console for '{}'...", server_path.display()).cyan());
        client.attach_console(&server_path).await?;
    }
    Ok(())
}
