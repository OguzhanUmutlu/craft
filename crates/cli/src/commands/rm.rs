use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_rm(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    rf: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if name_arg.is_empty() { None } else { Some(name_arg) },
        false,
    )?;

    let mut registry = ServersRegistry::load(paths)?;
    if registry.find_by_path(&server_path).is_none() {
        return Err(CraftError::ServerNotFound(format!(
            "Server at '{}' is not registered.",
            server_path.display()
        )));
    }

    if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            let running = client.get_running().await.unwrap_or_default();
            if running.iter().any(|p| p == &server_path || p.canonicalize().ok() == server_path.canonicalize().ok()) {
                return Err(CraftError::Other(
                    "Server is currently running! Please stop it with 'craft stop' before removing.".to_string(),
                ));
            }
        }
    }

    if rf && server_path.exists() {
        println!("{}", "WARNING: This action will permanently delete all files in the server directory!".red().bold());
        print!("{}", "Type 'delete files' to confirm: ".red());
        let _ = io::stdout().flush();

        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);

        if input.trim().eq_ignore_ascii_case("delete files") {
            fs::remove_dir_all(&server_path)?;
            println!("{}", format!("All files in '{}' have been deleted.", server_path.display()).yellow());
        } else {
            println!("{}", "Operation cancelled. Files have NOT been deleted.".cyan());
            return Ok(());
        }
    }

    registry.remove(&server_path);
    registry.save(paths)?;

    println!("{}", format!("Server '{}' unregistered successfully.", server_path.display()).green());
    Ok(())
}
