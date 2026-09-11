use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_stop(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    force: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if name_arg.is_empty() { None } else { Some(name_arg) },
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

    if !DaemonClient::is_daemon_running(paths) {
        println!("{}", "Craft daemon is not running. No background servers active.".yellow());
        return Ok(());
    }

    let mut client = DaemonClient::connect(paths).await?;
    println!("{}", format!("Stopping server '{}'...", server_path.display()).yellow());
    client.stop_server(&server_path, force).await?;
    println!("{}", format!("Server '{}' stopped successfully.", server_path.display()).green());
    Ok(())
}
