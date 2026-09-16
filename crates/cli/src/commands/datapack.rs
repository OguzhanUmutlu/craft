use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_plugins::PluginManager;
use crate::cli::DatapackCommands;

pub async fn handle_datapack(action: DatapackCommands, paths: &CraftPaths) -> Result<()> {
    let pm = PluginManager::new();

    match action {
        DatapackCommands::Search { query } => {
            println!("{}", format!("Searching datapacks for '{}' on Modrinth...", query).cyan());
            let results = pm.search_datapacks(&query).await;

            if results.is_empty() {
                println!("{}", "No datapacks found.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("ID / Slug").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            for hit in results {
                let desc = craft_core::truncate_ellipsis(&hit.description, 60);

                table.add_row(Row::from(vec![
                    Cell::new(hit.name).fg(Color::Green),
                    Cell::new(hit.id_or_slug),
                    Cell::new(desc),
                ]));
            }

            println!("{table}");
            println!("\nInstall via: craft datapack install <id> <server_name> [--world <world_name>]");
        }
        DatapackCommands::Install { project_id, server, world } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => return Err(CraftError::ServerNotFound(format!("Server '{}' is not registered.", server))),
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.datapacks {
                return Err(CraftError::Other(format!(
                    "Server '{}' (software: {}) does not support datapacks. Datapacks only exist in Minecraft Java world saves.",
                    s.name, s.software
                )));
            }

            let target_world = world.unwrap_or_else(|| craft_core::get_default_world(&server_path));
            let datapacks_dir = server_path.join(&target_world).join("datapacks");
            std::fs::create_dir_all(&datapacks_dir)?;

            println!(
                "{}",
                format!(
                    "Installing datapack '{}' to server '{}' (world: '{}')...",
                    project_id, s.name, target_world
                )
                .cyan()
            );

            let dest = pm.install_datapack_from_modrinth(&server_path, &project_id, &target_world).await?;
            println!("{}", format!("[OK] Successfully installed datapack to '{}'!", dest.display()).green().bold());
        }
        DatapackCommands::List { server, world } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => return Err(CraftError::ServerNotFound(format!("Server '{}' is not registered.", server))),
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.datapacks {
                println!("{}", format!("[NOTE] Server '{}' (software: {}) does not support datapacks.", s.name, s.software).yellow());
                return Ok(());
            }

            let target_world = world.unwrap_or_else(|| craft_core::get_default_world(&server_path));
            let datapacks_dir = server_path.join(&target_world).join("datapacks");
            let mut installed = Vec::new();
            if datapacks_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&datapacks_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = if path.is_file() {
                            entry.metadata().map(|m| m.len()).unwrap_or(0)
                        } else {
                            0
                        };
                        let is_dir = path.is_dir();
                        installed.push((name, size, is_dir));
                    }
                }
            }

            installed.sort_by(|a, b| a.0.cmp(&b.0));

            if installed.is_empty() {
                println!("{}", format!("No datapacks installed in '{}/datapacks'.", target_world).yellow());
                return Ok(());
            }

            println!(
                "{}",
                format!("Installed datapacks in '{}' (world: {}, total: {}):", s.name, target_world, installed.len())
                    .cyan()
                    .bold()
            );
            let mut table = Table::new();
            table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Datapack").fg(Color::Cyan),
                Cell::new("Type").fg(Color::Cyan),
                Cell::new("Size").fg(Color::Cyan),
            ]);

            for (name, size, is_dir) in installed {
                let kind = if is_dir { "Folder" } else { "Archive (.zip)" };
                let size_str = if is_dir { "-".to_string() } else { craft_core::format_size(size) };
                table.add_row(Row::from(vec![
                    Cell::new(name).fg(Color::Green),
                    Cell::new(kind).fg(Color::Yellow),
                    Cell::new(size_str),
                ]));
            }
            println!("{table}");
        }
        DatapackCommands::Remove { server, filename, world } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let target_world = world.unwrap_or_else(|| craft_core::get_default_world(&server_path));
            let datapacks_dir = server_path.join(&target_world).join("datapacks");
            let target = datapacks_dir.join(&filename);

            let file_to_delete = if target.exists() {
                target
            } else {
                let target_with_zip = datapacks_dir.join(format!("{}.zip", filename));
                if target_with_zip.exists() {
                    target_with_zip
                } else {
                    return Err(CraftError::Other(format!(
                        "Datapack '{}' not found in '{}/datapacks'.",
                        filename, target_world
                    )));
                }
            };

            if file_to_delete.is_dir() {
                std::fs::remove_dir_all(&file_to_delete)?;
            } else {
                std::fs::remove_file(&file_to_delete)?;
            }

            println!("{}", format!("[OK] Removed datapack '{}'.", file_to_delete.display()).green().bold());
        }
    }

    Ok(())
}
