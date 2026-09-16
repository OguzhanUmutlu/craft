use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{CraftError, CraftPaths, PropertyCategory, Result, ServerProperties};

#[derive(clap::Subcommand, Debug, Clone)]
pub enum PropAction {
    /// List all properties (optionally filtered by category or search term)
    #[command(alias = "list")]
    Ls {
        /// Filter by category (network, gameplay, world, security, performance, rcon, general)
        #[arg(short, long)]
        category: Option<String>,
        /// Search keyword in key, value, or description
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Get the value of a specific property
    Get {
        /// Property key
        key: String,
    },
    /// Set or update the value of a property
    Set {
        /// Property key
        key: String,
        /// New property value
        value: String,
    },
    /// Reset or remove a property from server.properties
    Reset {
        /// Property key
        key: String,
    },
}

pub async fn handle_prop(
    server_name: &str,
    action: Option<PropAction>,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server_name), true)?;
    let props_path = server_path.join("server.properties");

    if !props_path.exists() {
        return Err(CraftError::Other(format!(
            "server.properties not found in '{}'",
            server_path.display()
        )));
    }

    let mut props = ServerProperties::load(&props_path)?;

    match action.unwrap_or(PropAction::Ls { category: None, search: None }) {
        PropAction::Get { key } => {
            if let Some(val) = props.get(&key) {
                println!("{}: {} = {}", "Property".cyan().bold(), key.white().bold(), val.green());
                println!("  Description: {}", ServerProperties::property_description(&key).dimmed());
            } else {
                println!("{}: Property '{}' is not set.", "Notice".yellow().bold(), key);
            }
            Ok(())
        }
        PropAction::Set { key, value } => {
            props.set(&key, &value);
            props.save(&props_path)?;
            println!(
                "{}: Set '{}' = '{}' in {}",
                "Success".green().bold(),
                key.white().bold(),
                value.green().bold(),
                props_path.display().to_string().dimmed()
            );
            Ok(())
        }
        PropAction::Reset { key } => {
            if props.remove(&key) {
                props.save(&props_path)?;
                println!(
                    "{}: Removed '{}' from {}",
                    "Success".green().bold(),
                    key.white().bold(),
                    props_path.display().to_string().dimmed()
                );
            } else {
                println!("{}: Property '{}' was not found.", "Notice".yellow().bold(), key);
            }
            Ok(())
        }
        PropAction::Ls { category, search } => {
            let cat_filter = category.as_deref().and_then(|c| match c.to_lowercase().as_str() {
                "net" | "network" => Some(PropertyCategory::Network),
                "game" | "gameplay" => Some(PropertyCategory::Gameplay),
                "world" => Some(PropertyCategory::World),
                "sec" | "security" => Some(PropertyCategory::Security),
                "perf" | "performance" => Some(PropertyCategory::Performance),
                "rcon" => Some(PropertyCategory::Rcon),
                "gen" | "general" => Some(PropertyCategory::General),
                _ => None,
            });

            let search_term = search.map(|s| s.to_lowercase());

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Category").fg(Color::Cyan),
                    Cell::new("Key").fg(Color::Green),
                    Cell::new("Value").fg(Color::Yellow),
                    Cell::new("Description").fg(Color::DarkGrey),
                ]);

            let mut count = 0;
            for (key, val) in props.list_entries() {
                let cat = ServerProperties::category_of(key);
                if let Some(cf) = cat_filter {
                    if cat != cf {
                        continue;
                    }
                }

                let desc = ServerProperties::property_description(key);

                if let Some(ref q) = search_term {
                    let matches = key.to_lowercase().contains(q)
                        || val.to_lowercase().contains(q)
                        || desc.to_lowercase().contains(q);
                    if !matches {
                        continue;
                    }
                }

                let cat_label = format!("{} {}", cat.icon(), cat.name());
                table.add_row(vec![
                    Cell::new(cat_label),
                    Cell::new(key).fg(Color::White),
                    Cell::new(val).fg(Color::Green),
                    Cell::new(desc),
                ]);
                count += 1;
            }

            println!("\n{}", format!("=== Server Properties: {} ({} entries) ===", server_name, count).cyan().bold());
            println!("{table}\n");
            Ok(())
        }
    }
}
