use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_plugins::PluginManager;
use crate::cli::PluginCommands;

pub async fn handle_plugin(action: PluginCommands, paths: &CraftPaths) -> Result<()> {
    let pm = PluginManager::new();

    match action {
        PluginCommands::Search { query } => {
            println!("{}", format!("Searching plugins for '{}' across Modrinth, Hangar, and Poggit...", query).cyan());
            let results = pm.search(&query).await;

            if results.is_empty() {
                println!("{}", "No plugins found.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Source").fg(Color::Cyan),
                Cell::new("ID / Slug").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            for hit in results {
                let desc = craft_core::truncate_ellipsis(&hit.description, 60);

                table.add_row(Row::from(vec![
                    Cell::new(hit.name).fg(Color::Green),
                    Cell::new(hit.source).fg(Color::Yellow),
                    Cell::new(hit.id_or_slug),
                    Cell::new(desc),
                ]));
            }

            println!("{table}");
            println!("\nInstall via: craft plugin install <id> <server_name>");
        }
        PluginCommands::Install { project_id, server } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            if registry.find_by_path(&server_path).is_none() {
                return Err(CraftError::ServerNotFound(format!("Server '{}' is not registered.", server)));
            }

            println!("{}", format!("Installing plugin '{}' to '{}'...", project_id, server_path.display()).cyan());
            let dest = pm.install_from_modrinth(&server_path, &project_id).await?;
            println!("{}", format!("[OK] Successfully installed plugin to '{}'!", dest.display()).green().bold());
        }
    }

    Ok(())
}
