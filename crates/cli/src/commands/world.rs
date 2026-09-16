use std::path::PathBuf;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{
    set_default_world, set_end_world, set_nether_world, CraftError, CraftPaths, Result,
};
use craft_plugins::world::{
    inspect_world_metadata, install_world_from_url, install_world_from_zip, list_installed_worlds,
    list_world_player_data,
};

#[derive(clap::Subcommand, Debug, Clone)]
pub enum WorldAction {
    /// List all installed worlds for this server
    #[command(alias = "list")]
    Ls,
    /// View detailed metadata for a world (seed, gamemode, difficulty, spawn)
    Info {
        /// World directory name (defaults to active default world)
        world: Option<String>,
    },
    /// List player positions, health, and stats in world's playerdata
    Players {
        /// World directory name (defaults to active default world)
        world: Option<String>,
    },
    /// Set a world as the active default Overworld (level-name in server.properties)
    SetDefault {
        /// World directory name
        world: String,
    },
    /// Set a world as the active Nether dimension world
    SetNether {
        /// World directory name
        world: String,
    },
    /// Set a world as the active The End dimension world
    SetEnd {
        /// World directory name
        world: String,
    },
    /// Import a world from a local .zip archive or remote URL
    Import {
        /// Path to local .zip file or remote HTTP/HTTPS download link
        path_or_url: String,
        /// Custom destination folder name (optional)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Delete a world directory permanently
    #[command(alias = "delete")]
    Rm {
        /// World directory name
        world: String,
        /// Bypass confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn handle_world(
    server_name: &str,
    action: Option<WorldAction>,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server_name), true)?;

    match action.unwrap_or(WorldAction::Ls) {
        WorldAction::Ls => {
            let worlds = list_installed_worlds(&server_path);
            if worlds.is_empty() {
                println!("{}", format!("No worlds found in '{}'.", server_path.display()).yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("World Name").fg(Color::Cyan),
                    Cell::new("Role / Status").fg(Color::Green),
                    Cell::new("Size (MB)").fg(Color::Yellow),
                    Cell::new("Path").fg(Color::DarkGrey),
                ]);

            for w in &worlds {
                let mut badges = Vec::new();
                if w.is_default {
                    badges.push("[DEFAULT OVERWORLD]".to_string());
                }
                if w.is_nether {
                    badges.push("[NETHER]".to_string());
                }
                if w.is_end {
                    badges.push("[THE END]".to_string());
                }
                let status_str = if badges.is_empty() {
                    "[AVAILABLE]".dimmed().to_string()
                } else {
                    badges.join(" ").green().bold().to_string()
                };

                let mb = format!("{:.1}", (w.size_bytes as f64) / (1024.0 * 1024.0));
                table.add_row(vec![
                    Cell::new(&w.name).fg(Color::White),
                    Cell::new(status_str),
                    Cell::new(mb).fg(Color::Yellow),
                    Cell::new(w.path.display().to_string()),
                ]);
            }

            println!("\n{}", format!("=== Installed Worlds: {} ({} worlds) ===", server_name, worlds.len()).cyan().bold());
            println!("{table}\n");
            Ok(())
        }
        WorldAction::Info { world } => {
            let worlds = list_installed_worlds(&server_path);
            let target_name = world.unwrap_or_else(|| {
                worlds.iter().find(|w| w.is_default).map(|w| w.name.clone()).unwrap_or_else(|| "world".to_string())
            });

            let world_path = server_path.join(&target_name);
            let meta = inspect_world_metadata(&world_path)?;

            println!("\n{}", format!("=== World Metadata: {}/{} ===", server_name, target_name).cyan().bold());
            println!("  Level Name:     {}", meta.level_name.white().bold());
            println!("  Game Mode:      {}", meta.game_type.green());
            println!("  Difficulty:     {}", meta.difficulty.yellow());
            println!("  Hardcore:       {}", if meta.hardcore { "Yes".red().bold() } else { "No".dimmed() });
            println!("  Spawn Location: X={}, Y={}, Z={}", meta.spawn_x, meta.spawn_y, meta.spawn_z);
            println!("  Seed:           {}", meta.seed.map(|s| s.to_string()).unwrap_or_else(|| "Unknown".to_string()).cyan());
            if let Some(ref ver) = meta.version_name {
                println!("  Version:        {}", ver.dimmed());
            }
            println!("  World Time:     {} ticks (Day: {})\n", meta.time, meta.day_time / 24000);
            Ok(())
        }
        WorldAction::Players { world } => {
            let worlds = list_installed_worlds(&server_path);
            let target_name = world.unwrap_or_else(|| {
                worlds.iter().find(|w| w.is_default).map(|w| w.name.clone()).unwrap_or_else(|| "world".to_string())
            });

            let world_path = server_path.join(&target_name);
            let players = list_world_player_data(&world_path, &server_path)?;

            if players.is_empty() {
                println!("{}", format!("No player data files found in '{}/playerdata'.", target_name).yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Player Name / UUID").fg(Color::Cyan),
                    Cell::new("Coordinates (X, Y, Z)").fg(Color::Green),
                    Cell::new("Health").fg(Color::Red),
                    Cell::new("XP").fg(Color::Yellow),
                    Cell::new("Mode").fg(Color::White),
                    Cell::new("Dimension").fg(Color::DarkGrey),
                ]);

            for p in &players {
                let coords = format!("{:.1}, {:.1}, {:.1}", p.pos.0, p.pos.1, p.pos.2);
                let label = if p.name == p.uuid {
                    p.uuid.dimmed().to_string()
                } else {
                    format!("{} ({})", p.name.white().bold(), p.uuid.dimmed())
                };

                table.add_row(vec![
                    Cell::new(label),
                    Cell::new(coords),
                    Cell::new(format!("{:.1} / 20", p.health)),
                    Cell::new(format!("Lvl {}", p.xp_level)),
                    Cell::new(&p.gamemode),
                    Cell::new(&p.dimension),
                ]);
            }

            println!("\n{}", format!("=== Player Data: {}/{} ({} players) ===", server_name, target_name, players.len()).cyan().bold());
            println!("{table}\n");
            Ok(())
        }
        WorldAction::SetDefault { world } => {
            let world_path = server_path.join(&world);
            if !world_path.exists() {
                return Err(CraftError::Other(format!("World directory '{}' does not exist.", world_path.display())));
            }
            set_default_world(&server_path, &world)?;
            println!("{}: Set '{}' as active default Overworld in server.properties!", "Success".green().bold(), world.white().bold());
            Ok(())
        }
        WorldAction::SetNether { world } => {
            let world_path = server_path.join(&world);
            if !world_path.exists() {
                return Err(CraftError::Other(format!("World directory '{}' does not exist.", world_path.display())));
            }
            set_nether_world(&server_path, &world)?;
            println!("{}: Set '{}' as active Nether dimension world!", "Success".green().bold(), world.white().bold());
            Ok(())
        }
        WorldAction::SetEnd { world } => {
            let world_path = server_path.join(&world);
            if !world_path.exists() {
                return Err(CraftError::Other(format!("World directory '{}' does not exist.", world_path.display())));
            }
            set_end_world(&server_path, &world)?;
            println!("{}: Set '{}' as active The End dimension world!", "Success".green().bold(), world.white().bold());
            Ok(())
        }
        WorldAction::Import { path_or_url, name } => {
            println!("{}: Importing world into '{}'...", "Craft".cyan().bold(), server_name);
            let (dest, installed_name) = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
                install_world_from_url(&server_path, &path_or_url, name.as_deref()).await?
            } else {
                let zip_path = PathBuf::from(&path_or_url);
                if !zip_path.exists() {
                    return Err(CraftError::Other(format!("File '{}' does not exist.", path_or_url)));
                }
                install_world_from_zip(&server_path, &zip_path, name.as_deref())?
            };

            println!("{}: Imported world as '{}' at {}", "Success".green().bold(), installed_name.white().bold(), dest.display());
            Ok(())
        }
        WorldAction::Rm { world, force } => {
            let world_path = server_path.join(&world);
            if !world_path.exists() {
                return Err(CraftError::Other(format!("World directory '{}' not found.", world_path.display())));
            }

            if !force {
                let prompt = format!("Are you sure you want to permanently delete world '{}'?", world);
                if !dialoguer::Confirm::new().with_prompt(prompt).default(false).interact()? {
                    println!("{}", "Deletion cancelled.".yellow());
                    return Ok(());
                }
            }

            std::fs::remove_dir_all(&world_path)?;
            println!("{}: Removed world directory '{}'.", "Success".green().bold(), world_path.display());
            Ok(())
        }
    }
}
