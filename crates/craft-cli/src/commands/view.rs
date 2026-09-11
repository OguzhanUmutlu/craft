use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_view(
    name_arg: &str,
    custom_path: Option<PathBuf>,
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
            "Server '{}' is not registered.",
            server_path.display()
        )));
    }

    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other(
            "Craft daemon is not running. Start the server first with 'craft run <name>'.".to_string(),
        ));
    }

    let client = DaemonClient::connect(paths).await?;
    println!("{}", format!("Attaching to console for '{}'...", server_path.display()).cyan());
    client.attach_console(&server_path).await?;
    Ok(())
}
