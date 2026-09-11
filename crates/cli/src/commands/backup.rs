use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_backup::BackupEngine;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use crate::cli::BackupCommands;

pub async fn handle_backup(action: BackupCommands, paths: &CraftPaths) -> Result<()> {
    let engine = BackupEngine::new(paths);

    match action {
        BackupCommands::Create { server } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            if registry.find_by_path(&server_path).is_none() {
                return Err(CraftError::ServerNotFound(format!("Server '{}' is not registered.", server)));
            }

            println!("{}", format!("Creating compressed snapshot for server '{}'...", server).cyan());
            let backup_file = engine.create_backup(&server, &server_path, None).await?;
            let meta = std::fs::metadata(&backup_file)?;
            let mb = (meta.len() as f64) / (1024.0 * 1024.0);

            println!("{}", format!("✓ Backup created successfully: {} ({:.2} MB)", backup_file.display(), mb).green().bold());
        }
        BackupCommands::List { server } => {
            let list = engine.list_backups(&server);
            if list.is_empty() {
                println!("{}", format!("No backups found for server '{}'.", server).yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Filename").fg(Color::Cyan),
                Cell::new("Size (MB)").fg(Color::Cyan),
                Cell::new("Created At").fg(Color::Cyan),
            ]);

            for b in list {
                let mb = format!("{:.2} MB", (b.size_bytes as f64) / (1024.0 * 1024.0));
                table.add_row(Row::from(vec![
                    Cell::new(b.filename).fg(Color::Green),
                    Cell::new(mb),
                    Cell::new(b.created_at),
                ]));
            }

            println!("{table}");
        }
        BackupCommands::Restore { server, backup_file } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            println!("{}", format!("Restoring backup '{}' to '{}'...", backup_file.display(), server_path.display()).yellow());
            engine.restore_backup(&backup_file, &server_path)?;
            println!("{}", "✓ Server restored successfully!".green().bold());
        }
    }

    Ok(())
}
