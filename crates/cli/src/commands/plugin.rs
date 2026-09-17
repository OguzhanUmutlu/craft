use crate::cli::PluginCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_plugins::PluginManager;

pub async fn handle_plugin(action: PluginCommands, paths: &CraftPaths) -> Result<()> {
    let pm = PluginManager::new();

    match action {
        PluginCommands::Search { query } => {
            println!(
                "{}",
                format!(
                    "Searching plugins for '{}' across Modrinth, Hangar, and Poggit...",
                    query
                )
                .cyan()
            );
            let results = pm.search(&query).await;

            if results.is_empty() {
                println!("{}", "No plugins found.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
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

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.plugins {
                if caps.mods {
                    return Err(CraftError::Other(format!(
                        "Server '{}' (software: {}) does not support plugins.\nThis is a modded server; use 'craft mod install' instead.",
                        s.name, s.software
                    )));
                } else {
                    return Err(CraftError::Other(format!(
                        "Server '{}' (software: {}) does not support plugins.\nPlugins only exist in non-vanilla server softwares (e.g. Paper, Purpur, Spigot).",
                        s.name, s.software
                    )));
                }
            }

            println!(
                "{}",
                format!(
                    "Installing plugin '{}' to '{}'...",
                    project_id,
                    server_path.display()
                )
                .cyan()
            );
            let dest = pm.install_from_modrinth(&server_path, &project_id).await?;
            println!(
                "{}",
                format!(
                    "[OK] Successfully installed plugin to '{}'!",
                    dest.display()
                )
                .green()
                .bold()
            );
        }
        PluginCommands::List { server } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.plugins {
                println!(
                    "{}",
                    format!(
                        "[NOTE] Server '{}' (software: {}) does not support plugins.",
                        s.name, s.software
                    )
                    .yellow()
                );
                return Ok(());
            }

            let plugins_dir = server_path.join("plugins");
            let mut installed = Vec::new();
            if plugins_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("jar")
                        {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                            installed.push((name, size));
                        }
                    }
                }
            }

            installed.sort_by(|a, b| a.0.cmp(&b.0));

            if installed.is_empty() {
                println!(
                    "{}",
                    format!("No plugins installed in '{}'.", plugins_dir.display()).yellow()
                );
                return Ok(());
            }

            println!(
                "{}",
                format!("Installed plugins in '{}' ({}):", s.name, installed.len())
                    .cyan()
                    .bold()
            );
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Plugin File").fg(Color::Cyan),
                Cell::new("Size").fg(Color::Cyan),
            ]);

            for (name, size) in installed {
                table.add_row(Row::from(vec![
                    Cell::new(name).fg(Color::Green),
                    Cell::new(craft_core::format_size(size)).fg(Color::Yellow),
                ]));
            }
            println!("{table}");
        }
        PluginCommands::Remove { server, filename } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let plugins_dir = server_path.join("plugins");
            let target = plugins_dir.join(&filename);

            let file_to_delete = if target.exists() && target.is_file() {
                target
            } else {
                let target_with_ext = plugins_dir.join(format!("{}.jar", filename));
                if target_with_ext.exists() && target_with_ext.is_file() {
                    target_with_ext
                } else {
                    return Err(CraftError::Other(format!(
                        "Plugin file '{}' not found in '{}'.",
                        filename,
                        plugins_dir.display()
                    )));
                }
            };

            std::fs::remove_file(&file_to_delete)?;
            println!(
                "{}",
                format!("[OK] Removed plugin '{}'.", file_to_delete.display())
                    .green()
                    .bold()
            );
        }
    }

    Ok(())
}
