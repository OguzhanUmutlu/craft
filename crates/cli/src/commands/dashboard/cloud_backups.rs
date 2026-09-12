use colored::Colorize;
use craft_backup::{BackupEngine, GDriveStorageProvider, S3StorageProvider, StorageProvider};
use craft_core::{
    CraftPaths, GDriveBackupConfig, GlobalBackupRegistry, Result,
    S3BackupConfig, ServersRegistry,
};

use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_password_prompt, show_modal_message,
    AltScreenGuard, MenuEntry,
};

pub async fn setup_backup_systems_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);

        let s3_status = if let Some(ref s3) = registry.s3 {
            format!("[CONFIGURED: {}]", s3.bucket).green().bold().to_string()
        } else {
            "[NOT CONFIGURED]".dimmed().to_string()
        };

        let gdrive_status = if let Some(ref gd) = registry.gdrive {
            format!("[CONFIGURED: {}]", gd.folder_id).green().bold().to_string()
        } else {
            "[NOT CONFIGURED]".dimmed().to_string()
        };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Configure local disk and cloud storage systems for server backups.\r\n Local Disk:                [ACTIVE: ~/.craft/backups]\r\n S3 / R2 / MinIO / Wasabi: {}\r\n Google Drive:              {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("BACKUP SYSTEMS CONFIGURATION", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            s3_status,
            gdrive_status,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Configure S3 / Cloudflare R2 / MinIO / Wasabi"),
            MenuEntry::new("2", "Configure Google Drive Cloud Storage"),
            MenuEntry::new("3", "Trigger Immediate Multi-Destination Cloud Backup"),
            MenuEntry::new("0", "Back to Backups Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                configure_s3_menu(paths).await?;
            }
            Some(1) => {
                configure_gdrive_menu(paths).await?;
            }
            Some(2) => {
                trigger_multi_backup_menu(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn configure_s3_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);

        let info_line = if let Some(ref s3) = registry.s3 {
            format!(
                " Bucket: {} | Region: {}\r\n Endpoint: {}\r\n Key ID:   {}",
                s3.bucket.cyan().bold(),
                s3.region,
                s3.endpoint.as_deref().unwrap_or("AWS S3 Standard (Default)"),
                s3.access_key_id
            )
        } else {
            " No S3/R2 storage provider configured.".dimmed().to_string()
        };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n{}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("S3 / R2 / MINIO / WASABI CONFIGURATION", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            info_line,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        entries.push(MenuEntry::new("1", "Setup / Update S3 Credentials"));
        if registry.s3.is_some() {
            entries.push(MenuEntry::new("2", "Test S3 Cloud Connection"));
            entries.push(MenuEntry::new("3", "Remove S3 Configuration"));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let bucket = match run_input_prompt("S3 BUCKET", "Bucket Name:", None)? {
                    Some(b) if !b.trim().is_empty() => b.trim().to_string(),
                    _ => continue,
                };

                let region = match run_input_prompt("S3 REGION", "Region (e.g. us-east-1, eu-central-1, auto):", Some("us-east-1"))? {
                    Some(r) if !r.trim().is_empty() => r.trim().to_string(),
                    _ => "us-east-1".to_string(),
                };

                let endpoint = match run_input_prompt("S3 ENDPOINT", "Custom Endpoint URL (leave empty for AWS S3):", None)? {
                    Some(ep) if !ep.trim().is_empty() => Some(ep.trim().to_string()),
                    _ => None,
                };

                let access_key = match run_input_prompt("ACCESS KEY ID", "AWS / R2 Access Key ID:", None)? {
                    Some(k) if !k.trim().is_empty() => k.trim().to_string(),
                    _ => continue,
                };

                let secret_key = match run_password_prompt("SECRET ACCESS KEY", "AWS / R2 Secret Access Key:")? {
                    Some(k) if !k.trim().is_empty() => k.trim().to_string(),
                    _ => continue,
                };

                let prefix = match run_input_prompt("STORAGE PREFIX", "Key Prefix / Folder (optional):", Some("craft-backups"))? {
                    Some(p) if !p.trim().is_empty() => Some(p.trim().to_string()),
                    _ => None,
                };

                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.s3 = Some(S3BackupConfig {
                    bucket,
                    region,
                    endpoint,
                    access_key_id: access_key,
                    secret_access_key: secret_key,
                    prefix,
                });
                reg.save(paths)?;

                show_modal_message(
                    "S3 CONFIGURED",
                    &["[OK] S3 storage provider credentials saved successfully.".green().bold().to_string()],
                    false,
                )?;
            }
            Some(1) if registry.s3.is_some() => {
                if let Some(ref config) = registry.s3 {
                    print_in_place_status(
                        "TESTING S3 CONNECTION",
                        &[format!("Testing connectivity and bucket access on '{}'...", config.bucket)],
                    )?;

                    let provider = S3StorageProvider::new(config.clone());
                    match provider.list_files("").await {
                        Ok(entries) => {
                            show_modal_message(
                                "S3 CONNECTION SUCCESS",
                                &[
                                    "[OK] Successfully authenticated and connected to S3!".green().bold().to_string(),
                                    format!("Total objects found in bucket: {}", entries.len()),
                                ],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "S3 CONNECTION FAILED",
                                &[format!("[ERROR] {}", e)],
                                true,
                            )?;
                        }
                    }
                }
            }
            Some(2) if registry.s3.is_some() => {
                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.s3 = None;
                reg.save(paths)?;
                show_modal_message(
                    "S3 REMOVED",
                    &["[OK] S3 storage provider removed.".green().bold().to_string()],
                    false,
                )?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn configure_gdrive_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);

        let info_line = if let Some(ref gd) = registry.gdrive {
            format!(
                " Folder ID: {}\r\n Auth Type: {}",
                gd.folder_id.cyan().bold(),
                if gd.api_token.is_some() { "OAuth / API Token" } else { "Service Account Key" }
            )
        } else {
            " No Google Drive storage provider configured.".dimmed().to_string()
        };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n{}\r\n{}",
            box_top(width).cyan().bold(),
            box_title("GOOGLE DRIVE STORAGE CONFIGURATION", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            info_line,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        entries.push(MenuEntry::new("1", "Setup / Update Google Drive Credentials"));
        if registry.gdrive.is_some() {
            entries.push(MenuEntry::new("2", "Test Google Drive Connection"));
            entries.push(MenuEntry::new("3", "Remove Google Drive Configuration"));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let folder_id = match run_input_prompt("GDRIVE FOLDER ID", "Google Drive Folder ID:", None)? {
                    Some(f) if !f.trim().is_empty() => f.trim().to_string(),
                    _ => continue,
                };

                let auth_header = " Choose Google Drive authentication method:";
                let auth_entries = vec![
                    MenuEntry::new("1", "Bearer / OAuth Token"),
                    MenuEntry::new("2", "Service Account JSON File Path"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut a_sel = 0;
                let (api_token, service_account_path) = match run_menu(auth_header, &auth_entries, &mut a_sel)? {
                    Some(0) => {
                        let tok = run_password_prompt("GDRIVE TOKEN", "Enter OAuth / API Token:")?;
                        (tok, None)
                    }
                    Some(1) => {
                        let sa = run_input_prompt("SERVICE ACCOUNT", "Path to service_account.json:", None)?;
                        let p = sa.map(|s| std::path::PathBuf::from(s.trim()));
                        (None, p)
                    }
                    _ => continue,
                };

                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.gdrive = Some(GDriveBackupConfig {
                    folder_id,
                    service_account_path,
                    api_token,
                });
                reg.save(paths)?;

                show_modal_message(
                    "GOOGLE DRIVE CONFIGURED",
                    &["[OK] Google Drive storage provider credentials saved successfully.".green().bold().to_string()],
                    false,
                )?;
            }
            Some(1) if registry.gdrive.is_some() => {
                if let Some(ref config) = registry.gdrive {
                    print_in_place_status(
                        "TESTING GDRIVE CONNECTION",
                        &[format!("Testing connectivity to Google Drive folder '{}'...", config.folder_id)],
                    )?;

                    let provider = GDriveStorageProvider::new(config.clone());
                    match provider.list_files("").await {
                        Ok(entries) => {
                            show_modal_message(
                                "GDRIVE CONNECTION SUCCESS",
                                &[
                                    "[OK] Successfully authenticated and connected to Google Drive!".green().bold().to_string(),
                                    format!("Total backup files found in folder: {}", entries.len()),
                                ],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "GDRIVE CONNECTION FAILED",
                                &[format!("[ERROR] {}", e)],
                                true,
                            )?;
                        }
                    }
                }
            }
            Some(2) if registry.gdrive.is_some() => {
                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.gdrive = None;
                reg.save(paths)?;
                show_modal_message(
                    "GDRIVE REMOVED",
                    &["[OK] Google Drive storage provider removed.".green().bold().to_string()],
                    false,
                )?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn configure_policies_menu(paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let servers_reg = ServersRegistry::load(paths)?;
        if servers_reg.servers.is_empty() {
            show_modal_message(
                "NO SERVERS",
                &["No local servers registered to configure auto-backup policies for."],
                false,
            )?;
            return Ok(());
        }

        let reg = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);


        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a server to configure automated backup schedule & cloud destinations:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("AUTO-BACKUP POLICIES PER SERVER", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, s) in servers_reg.servers.iter().enumerate() {
            let hk = (i + 1).to_string();
            let policy = reg.server_policies.get(&s.name);
            let status = if let Some(p) = policy {
                if p.enabled {
                    format!("[EVERY {}h | KEEP {} | S3:{} GD:{}]", p.interval_hours, p.retention_count, if p.upload_to_s3 { "Y" } else { "N" }, if p.upload_to_gdrive { "Y" } else { "N" }).green().bold().to_string()
                } else {
                    "[DISABLED]".dimmed().to_string()
                }
            } else {
                "[DISABLED]".dimmed().to_string()
            };
            entries.push(MenuEntry::new(hk, format!("{:<20} {}", s.name, status)));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < servers_reg.servers.len() => {
                let server_name = &servers_reg.servers[idx].name;
                configure_single_policy(paths, server_name).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn configure_single_policy(paths: &CraftPaths, server_name: &str) -> Result<()> {
    let mut selected = 0;

    loop {
        let mut reg = GlobalBackupRegistry::load(paths)?;
        let policy = reg.server_policies.entry(server_name.to_string()).or_default().clone();

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:            {}\r\n Automated Backups: {}\r\n Schedule Interval: Every {} Hours\r\n Retention Policy:  Keep {} Latest Archives\r\n Upload to S3/R2:   {}\r\n Upload to GDrive:  {}\r\n World Only Scope:  {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("AUTO-BACKUP POLICY: {}", server_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name.white().bold(),
            if policy.enabled { "[ENABLED]".green().bold() } else { "[DISABLED]".dimmed() },
            policy.interval_hours.to_string().cyan().bold(),
            policy.retention_count.to_string().yellow().bold(),
            if policy.upload_to_s3 { "[YES]".green().bold() } else { "[NO]".dimmed() },
            if policy.upload_to_gdrive { "[YES]".green().bold() } else { "[NO]".dimmed() },
            if policy.world_only { "[YES]".cyan().bold() } else { "[FULL SERVER]".white() },
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", format!("Toggle Status ({})", if policy.enabled { "Disable" } else { "Enable" })),
            MenuEntry::new("2", "Change Interval (Hours)"),
            MenuEntry::new("3", "Change Retention Count"),
            MenuEntry::new("4", format!("Toggle Upload to S3 ({})", if policy.upload_to_s3 { "Off" } else { "On" })),
            MenuEntry::new("5", format!("Toggle Upload to Google Drive ({})", if policy.upload_to_gdrive { "Off" } else { "On" })),
            MenuEntry::new("6", format!("Toggle Scope ({})", if policy.world_only { "Switch to Full" } else { "Switch to World Only" })),
            MenuEntry::new("0", "Save & Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let current = reg.server_policies.entry(server_name.to_string()).or_default();
                current.enabled = !current.enabled;
                reg.save(paths)?;
            }
            Some(1) => {
                let prompt = format!("Current: {} hours. Enter interval in hours:", policy.interval_hours);
                if let Some(val) = run_input_prompt("SCHEDULE INTERVAL", &prompt, Some(&policy.interval_hours.to_string()))? {
                    if let Ok(hours) = val.trim().parse::<u32>() {
                        let current = reg.server_policies.entry(server_name.to_string()).or_default();
                        current.interval_hours = hours.max(1);
                        reg.save(paths)?;
                    }
                }
            }
            Some(2) => {
                let prompt = format!("Current: {} archives. Enter number of archives to retain:", policy.retention_count);
                if let Some(val) = run_input_prompt("RETENTION COUNT", &prompt, Some(&policy.retention_count.to_string()))? {
                    if let Ok(cnt) = val.trim().parse::<usize>() {
                        let current = reg.server_policies.entry(server_name.to_string()).or_default();
                        current.retention_count = cnt.max(1);
                        reg.save(paths)?;
                    }
                }
            }
            Some(3) => {
                let current = reg.server_policies.entry(server_name.to_string()).or_default();
                current.upload_to_s3 = !current.upload_to_s3;
                reg.save(paths)?;
            }
            Some(4) => {
                let current = reg.server_policies.entry(server_name.to_string()).or_default();
                current.upload_to_gdrive = !current.upload_to_gdrive;
                reg.save(paths)?;
            }
            Some(5) => {
                let current = reg.server_policies.entry(server_name.to_string()).or_default();
                current.world_only = !current.world_only;
                reg.save(paths)?;
            }
            _ => return Ok(()),
        }
    }
}

async fn trigger_multi_backup_menu(paths: &CraftPaths) -> Result<()> {
    let servers_reg = ServersRegistry::load(paths)?;
    if servers_reg.servers.is_empty() {
        show_modal_message("NO SERVERS", &["No local servers available."], false)?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in servers_reg.servers.iter().enumerate() {
        entries.push(MenuEntry::new((i + 1).to_string(), s.name.clone()));
    }
    entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

    let mut sel = 0;
    if let Some(idx) = run_menu(" Select server to trigger immediate multi-cloud backup:", &entries, &mut sel)? {
        if idx < servers_reg.servers.len() {
            let server = &servers_reg.servers[idx];
            let backup_reg = GlobalBackupRegistry::load(paths)?;
            let policy = backup_reg.server_policies.get(&server.name).cloned().unwrap_or_default();

            print_in_place_status(
                "CREATING LOCAL ARCHIVE",
                &[format!("Generating compressed snapshot for '{}'...", server.name)],
            )?;

            let engine = BackupEngine::new(paths);
            let archive_path = match engine.create_backup(&server.name, &server.path, None, policy.world_only).await {
                Ok(p) => p,
                Err(e) => {
                    show_modal_message("BACKUP FAILED", &[format!("[ERROR] {}", e)], true)?;
                    return Ok(());
                }
            };

            let fname = archive_path.file_name().and_then(|f| f.to_str()).unwrap_or("backup.tar.gz");
            let mut summary = vec![
                format!("[OK] Local archive created: {}", archive_path.display()).green().bold().to_string(),
            ];

            // S3 upload if configured
            if let Some(ref s3_config) = backup_reg.s3 {
                print_in_place_status(
                    "UPLOADING TO S3",
                    &[format!("Uploading '{}' to S3 bucket '{}'...", fname, s3_config.bucket)],
                )?;

                let provider = S3StorageProvider::new(s3_config.clone());
                let remote_key = if let Some(ref pfx) = s3_config.prefix {
                    format!("{}/{}/{}", pfx.trim_end_matches('/'), server.name, fname)
                } else {
                    format!("{}/{}", server.name, fname)
                };

                match provider.upload_file(&archive_path, &remote_key).await {
                    Ok(_) => summary.push(format!("[OK] S3 Upload complete: s3://{}/{}", s3_config.bucket, remote_key).green().to_string()),
                    Err(e) => summary.push(format!("[WARN] S3 Upload failed: {}", e).yellow().to_string()),
                }
            }

            // GDrive upload if configured
            if let Some(ref gd_config) = backup_reg.gdrive {
                print_in_place_status(
                    "UPLOADING TO GOOGLE DRIVE",
                    &[format!("Uploading '{}' to Google Drive...", fname)],
                )?;

                let provider = GDriveStorageProvider::new(gd_config.clone());
                match provider.upload_file(&archive_path, fname).await {
                    Ok(_) => summary.push(format!("[OK] Google Drive Upload complete: folder {}", gd_config.folder_id).green().to_string()),
                    Err(e) => summary.push(format!("[WARN] Google Drive Upload failed: {}", e).yellow().to_string()),
                }
            }

            show_modal_message("MULTI-DESTINATION BACKUP COMPLETE", &summary, false)?;
        }
    }

    Ok(())
}
