use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{CraftError, CraftPaths, Result, TrashManager};

#[derive(clap::Subcommand, Debug, Clone)]
pub enum TrashAction {
    /// List all recoverable items in the trash bin
    #[command(alias = "list")]
    Ls,
    /// Restore an item from the trash bin back to its original path
    Restore {
        /// Trash item ID (or prefix)
        id: String,
    },
    /// Permanently purge and empty all items in the trash bin
    Empty {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn handle_trash(action: Option<TrashAction>, paths: &CraftPaths) -> Result<()> {
    let trash = TrashManager::new(paths);

    match action.unwrap_or(TrashAction::Ls) {
        TrashAction::Ls => {
            let items = trash.list_items()?;
            if items.is_empty() {
                println!("{}", "Trash bin is empty.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Item ID").fg(Color::Cyan),
                    Cell::new("Original Name").fg(Color::Green),
                    Cell::new("Original Path").fg(Color::White),
                    Cell::new("Size (MB)").fg(Color::Yellow),
                    Cell::new("Trashed Date").fg(Color::DarkGrey),
                ]);

            for item in &items {
                let mb = format!("{:.2}", (item.size_bytes as f64) / (1024.0 * 1024.0));
                table.add_row(vec![
                    Cell::new(&item.id).fg(Color::Cyan),
                    Cell::new(&item.original_name).fg(Color::White),
                    Cell::new(item.original_path.display().to_string()),
                    Cell::new(mb).fg(Color::Yellow),
                    Cell::new(&item.trashed_at),
                ]);
            }

            println!("\n{}", format!("=== Trash Bin ({} items) ===", items.len()).cyan().bold());
            println!("{table}\n");
            Ok(())
        }
        TrashAction::Restore { id } => {
            let items = trash.list_items()?;
            let target_item = items
                .iter()
                .find(|i| i.id.eq_ignore_ascii_case(&id) || i.id.starts_with(&id))
                .ok_or_else(|| CraftError::Other(format!("No trash item found matching ID '{}'", id)))?;

            println!(
                "{}: Restoring '{}' to '{}'...",
                "Craft".cyan().bold(),
                target_item.original_name.white().bold(),
                target_item.original_path.display().to_string().cyan()
            );

            let restored_path = trash.restore_item(&target_item.id)?;
            println!(
                "{}: Restored '{}' successfully!",
                "Success".green().bold(),
                restored_path.display()
            );
            Ok(())
        }
        TrashAction::Empty { force } => {
            let items = trash.list_items()?;
            if items.is_empty() {
                println!("{}", "Trash bin is already empty.".yellow());
                return Ok(());
            }

            if !force {
                let prompt = format!(
                    "Are you sure you want to permanently delete all {} items in the trash bin?",
                    items.len()
                );
                if !dialoguer::Confirm::new().with_prompt(prompt).default(false).interact()? {
                    println!("{}", "Operation cancelled.".yellow());
                    return Ok(());
                }
            }

            let count = trash.empty_trash()?;
            println!("{}: Permanently purged {} items from the trash bin.", "Success".green().bold(), count);
            Ok(())
        }
    }
}
