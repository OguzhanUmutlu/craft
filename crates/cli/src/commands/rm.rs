use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_rm(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    mut rf: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let is_tty = std::io::stdin().is_terminal();
    let theme = ColorfulTheme::default();

    let mut resolved_name = name_arg.to_string();

    if resolved_name.is_empty() && custom_path.is_none() {
        if !is_tty {
            return Err(CraftError::Other(
                "Please specify a server name to remove, or run interactively in a terminal.".to_string(),
            ));
        }

        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            println!("{}", "No registered servers found to remove.".yellow());
            return Ok(());
        }

        let items: Vec<String> = registry
            .servers
            .iter()
            .map(|s| format!("{:<20} [{} {}] ({})", s.name, s.software, s.version, s.path.display()))
            .collect();

        let idx = Select::with_theme(&theme)
            .with_prompt("Select server to remove")
            .items(&items)
            .default(0)
            .interact()?;

        resolved_name = registry.servers[idx].name.clone();

        if !rf {
            let actions = &[
                "[1] Unregister only (keep all world and server files on disk)",
                "[2] Permanently delete all files from disk",
                "[0] Cancel",
            ];
            let action_idx = Select::with_theme(&theme)
                .with_prompt("Action")
                .items(actions)
                .default(0)
                .interact()?;

            match action_idx {
                0 => {
                    rf = false;
                }
                1 => {
                    rf = true;
                }
                _ => {
                    println!("{}", "Operation cancelled.".cyan());
                    return Ok(());
                }
            }
        }
    }

    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if resolved_name.is_empty() { None } else { Some(&resolved_name) },
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
        if is_tty {
            let confirmed = Confirm::with_theme(&theme)
                .with_prompt(format!(
                    "WARNING: Permanently delete directory '{}' and all contents?",
                    server_path.display()
                ))
                .default(false)
                .interact()?;

            if !confirmed {
                println!("{}", "Operation cancelled. Files have NOT been deleted.".cyan());
                return Ok(());
            }

            let double_confirmed = Confirm::with_theme(&theme)
                .with_prompt(format!(
                    "FINAL CONFIRMATION: Are you ABSOLUTELY sure? All world data and configs in '{}' will be lost forever!",
                    server_path.display()
                ))
                .default(false)
                .interact()?;

            if !double_confirmed {
                println!("{}", "Operation cancelled on final confirmation. Files have NOT been deleted.".cyan());
                return Ok(());
            }
        }

        fs::remove_dir_all(&server_path)?;
        println!("{}", format!("All files in '{}' have been deleted.", server_path.display()).yellow());
    }

    registry.remove(&server_path);
    registry.save(paths)?;

    println!("{}", format!("Server '{}' unregistered successfully.", server_path.display()).green());
    Ok(())
}

