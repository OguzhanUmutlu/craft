use crate::cli::BackupCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_backup::{BackupEngine, BackupFormat};
use craft_core::{
    AutoBackupPolicy, CraftError, CraftPaths, GlobalBackupRegistry, Result, ServersRegistry,
};

pub async fn handle_backup(action: BackupCommands, paths: &CraftPaths) -> Result<()> {
    let engine = BackupEngine::new(paths);

    match action {
        BackupCommands::Create {
            server,
            world_only,
            format,
        } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            if registry.find_by_path(&server_path).is_none() {
                return Err(CraftError::ServerNotFound(format!(
                    "Server '{}' is not registered.",
                    server
                )));
            }

            let fmt = BackupFormat::from_str_opt(&format).unwrap_or(BackupFormat::TarZstd);

            let mode_str = if world_only {
                "world snapshot (excluding logs/cache)"
            } else {
                "full snapshot (excluding logs/cache)"
            };
            println!(
                "{}",
                format!(
                    "Creating compressed {} [{}] for server '{}'...",
                    mode_str,
                    fmt.display_name(),
                    server
                )
                .cyan()
            );
            let backup_file = engine
                .create_backup(&server, &server_path, None, world_only, Some(fmt))
                .await?;
            let meta = std::fs::metadata(&backup_file)?;
            let mb = (meta.len() as f64) / (1024.0 * 1024.0);

            println!(
                "{}",
                format!(
                    "[OK] Backup created successfully: {} ({:.2} MB)",
                    backup_file.display(),
                    mb
                )
                .green()
                .bold()
            );
        }
        BackupCommands::List { server } => {
            let list = engine.list_backups(&server);
            if list.is_empty() {
                println!(
                    "{}",
                    format!("No backups found for server '{}'.", server).yellow()
                );
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Filename").fg(Color::Cyan),
                Cell::new("Format").fg(Color::Cyan),
                Cell::new("Size (MB)").fg(Color::Cyan),
                Cell::new("Created At").fg(Color::Cyan),
            ]);

            for b in list {
                let mb = format!("{:.2} MB", (b.size_bytes as f64) / (1024.0 * 1024.0));
                table.add_row(Row::from(vec![
                    Cell::new(b.filename).fg(Color::Green),
                    Cell::new(b.format.display_name()).fg(Color::Yellow),
                    Cell::new(mb),
                    Cell::new(b.created_at),
                ]));
            }

            println!("{table}");
        }
        BackupCommands::Restore {
            server,
            backup_file,
        } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            println!(
                "{}",
                format!(
                    "Restoring backup '{}' to '{}'...",
                    backup_file.display(),
                    server_path.display()
                )
                .yellow()
            );
            engine.restore_backup(&backup_file, &server_path)?;
            println!("{}", "[OK] Server restored successfully!".green().bold());
        }
        BackupCommands::Policy {
            server,
            enable,
            interval,
            cron,
            retention,
            format,
            s3,
            gdrive,
            world_only,
        } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let s_reg = ServersRegistry::load(paths)?;
            let server_entry = match s_reg.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )));
                }
            };
            let server_name = &server_entry.name;

            let mut b_reg = GlobalBackupRegistry::load(paths)?;

            let has_updates = enable.is_some()
                || interval.is_some()
                || cron.is_some()
                || retention.is_some()
                || format.is_some()
                || s3.is_some()
                || gdrive.is_some()
                || world_only.is_some();

            if !has_updates {
                let policy = b_reg
                    .server_policies
                    .get(server_name)
                    .cloned()
                    .unwrap_or_default();

                println!(
                    "{}",
                    format!("=== Automated Backup Policy: '{}' ===", server_name)
                        .cyan()
                        .bold()
                );
                let status_str = if policy.enabled {
                    "[OK] Enabled".green()
                } else {
                    "[ ] Disabled".yellow()
                };
                println!("  Status:          {}", status_str);
                println!("  Schedule:        {}", policy.schedule_display().cyan());
                println!("  Retention:       {} backups", policy.retention_count);
                println!("  Format:          {}", policy.compression_format());
                println!("  World Only:      {}", policy.world_only);
                println!("  Upload to S3:    {}", policy.upload_to_s3);
                println!("  Upload to Drive: {}", policy.upload_to_gdrive);
                let last_str = policy
                    .last_backup_timestamp
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "Never".to_string());
                println!("  Last Backup:     {}", last_str.dimmed());
                println!(
                    "\nTip: Configure with 'craft backup policy {} --enable --interval 6 --retention 7 --format zstd'",
                    server_name
                );
                return Ok(());
            }

            let policy = b_reg
                .server_policies
                .entry(server_name.clone())
                .or_insert_with(AutoBackupPolicy::default);

            if let Some(en) = enable {
                policy.enabled = en;
            }
            if let Some(iv) = interval {
                policy.interval_hours = iv;
                policy.cron_expression = None;
            }
            if let Some(cr) = cron {
                policy.cron_expression = Some(cr);
            }
            if let Some(ret) = retention {
                policy.retention_count = ret;
            }
            if let Some(fmt) = format {
                policy.format = Some(fmt);
            }
            if let Some(s3_en) = s3 {
                policy.upload_to_s3 = s3_en;
            }
            if let Some(gd_en) = gdrive {
                policy.upload_to_gdrive = gd_en;
            }
            if let Some(wo) = world_only {
                policy.world_only = wo;
            }

            let summary_schedule = policy.schedule_display();
            let summary_retention = policy.retention_count;
            let summary_format = policy.compression_format().to_string();

            b_reg.save(paths)?;
            println!(
                "{}",
                format!(
                    "[OK] Automated backup policy updated for server '{}'!",
                    server_name
                )
                .green()
                .bold()
            );
            println!("  Schedule:  {}", summary_schedule.cyan());
            println!("  Retention: {} backups", summary_retention);
            println!("  Format:    {}", summary_format);
        }
    }

    Ok(())
}
