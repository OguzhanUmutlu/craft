use std::path::PathBuf;
use colored::Colorize;
use craft_backup::{GDriveStorageProvider, S3StorageProvider, StorageProvider};
use craft_core::{
    CraftPaths, GDriveBackupTarget, GlobalBackupRegistry, LocalBackupTarget,
    Result, S3BackupTarget, ServersRegistry,
};

use crate::commands::dashboard::screen::{
    box_divider, box_title, box_title_simple, box_top, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_paged_list_menu, run_password_prompt, show_modal_message,
    AltScreenGuard, MenuEntry, NavGuard, PagedMenuAction,
};

pub async fn setup_backup_systems_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Backup Systems");
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);

        let local_count = registry.local_targets.len();
        let s3_count = registry.s3_targets.len();
        let gd_count = registry.gdrive_targets.len();

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Configure local disk and cloud storage systems for server backups.\r\n Local Storage: {} registered\r\n S3 Storage:    {} registered\r\n Google Drive:  {} registered\r\n{}",
            box_top(width).cyan().bold(),
            box_title("BACKUP SYSTEMS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            local_count.to_string().cyan().bold(),
            s3_count.to_string().cyan().bold(),
            gd_count.to_string().cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Local Storage"),
            MenuEntry::new("2", "S3 Storage"),
            MenuEntry::new("3", "Google Drive"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                local_storage_targets_menu(paths).await?;
            }
            Some(1) => {
                s3_storage_targets_menu(paths).await?;
            }
            Some(2) => {
                gdrive_storage_targets_menu(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn local_storage_targets_menu(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Local Storage");
    let mut current_page = 0;
    let page_size = 6;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);
        let action_entries = vec![
            MenuEntry::new("a", "Add Local Storage").with_aliases(&["add", "n"]),
        ];

        let action = run_paged_list_menu(
            &registry.local_targets,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };
                format!(
                    "{}\r\n{}\r\n{}\r\n Registered local directories and mounted drives for server backups (Total: {}){}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("LOCAL STORAGE TARGETS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    total_count,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, t| format!("{:<20} {}", t.name, t.path.display()),
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(global_idx) if global_idx < registry.local_targets.len() => {
                let target_id = registry.local_targets[global_idx].id.clone();
                manage_single_local_target_menu(paths, &target_id).await?;
            }
            PagedMenuAction::Action(act) if act == "a" => {
                // Add Local Storage
                let name = match run_input_prompt("TARGET NAME", "Storage Name / Label (e.g. External Drive):", None)? {
                    Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                    _ => continue,
                };

                let default_dir = paths.backups_dir.display().to_string();
                let path_str = match run_input_prompt("STORAGE PATH", "Directory Path for Backups:", Some(&default_dir))? {
                    Some(p) if !p.trim().is_empty() => p.trim().to_string(),
                    _ => continue,
                };

                let p = PathBuf::from(&path_str);
                let _ = std::fs::create_dir_all(&p);

                let id = slugify(&name, &registry.local_targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.local_targets.push(LocalBackupTarget {
                    id,
                    name: name.clone(),
                    path: p,
                });
                reg.save(paths)?;

                show_modal_message(
                    "LOCAL STORAGE ADDED",
                    &[format!("[OK] Added local storage target '{}'.", name).green().bold().to_string()],
                    false,
                )?;
            }
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}

async fn manage_single_local_target_menu(paths: &CraftPaths, target_id: &str) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let target = match registry.local_targets.iter().find(|t| t.id == target_id) {
            Some(t) => t.clone(),
            None => return Ok(()),
        };
        let _nav = NavGuard::enter(&target.name);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Target Name: {}\r\n Target ID:   {}\r\n Directory:   {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("LOCAL STORAGE: {}", target.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            target.name.white().bold(),
            target.id.dimmed(),
            target.path.display(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Change Directory Path"),
            MenuEntry::new("2", "Rename Target"),
            MenuEntry::new("3", "Delete Target").with_aliases(&["d", "del", "rm"]),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let cur = target.path.display().to_string();
                if let Some(new_p_str) = run_input_prompt("STORAGE PATH", "Enter new directory path:", Some(&cur))? {
                    let trimmed = new_p_str.trim();
                    if !trimmed.is_empty() {
                        let new_p = PathBuf::from(trimmed);
                        let _ = std::fs::create_dir_all(&new_p);
                        let mut reg = GlobalBackupRegistry::load(paths)?;
                        if let Some(t) = reg.local_targets.iter_mut().find(|t| t.id == target_id) {
                            t.path = new_p;
                            reg.save(paths)?;
                        }
                    }
                }
            }
            Some(1) => {
                if let Some(new_name) = run_input_prompt("TARGET NAME", "Enter new name:", Some(&target.name))? {
                    let trimmed = new_name.trim();
                    if !trimmed.is_empty() {
                        let mut reg = GlobalBackupRegistry::load(paths)?;
                        if let Some(t) = reg.local_targets.iter_mut().find(|t| t.id == target_id) {
                            t.name = trimmed.to_string();
                            reg.save(paths)?;
                        }
                    }
                }
            }
            Some(2) => {
                let warn_text = if registry.local_targets.len() <= 1 {
                    " Warning: This is currently your only registered local storage target.\r\n"
                } else {
                    ""
                };
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Remove storage target '{}'?\r\n Target ID: {}\r\n Directory: {}\r\n{}\r\n{}\r\n Are you sure you want to proceed?\r\n{}",
                    box_top(width).yellow().bold(),
                    box_title_simple("CONFIRM DELETE TARGET", width, false).yellow().bold(),
                    box_divider(width).yellow().bold(),
                    target.name.white().bold(),
                    target.id,
                    target.path.display(),
                    warn_text,
                    box_divider(width).dimmed(),
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                    MenuEntry::new("2", format!("Confirm Delete of '{}'", target.name)),
                ];
                let mut c_sel = 0;
                if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    let mut reg = GlobalBackupRegistry::load(paths)?;
                    reg.local_targets.retain(|t| t.id != target_id);
                    reg.save(paths)?;
                    show_modal_message(
                        "TARGET REMOVED",
                        &[format!("[OK] Local storage target '{}' removed.", target.name).green().bold().to_string()],
                        false,
                    )?;
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn s3_storage_targets_menu(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("S3 Storage");
    let mut current_page = 0;
    let page_size = 6;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);
        let action_entries = vec![
            MenuEntry::new("a", "Add S3 Storage").with_aliases(&["add", "n"]),
        ];

        let action = run_paged_list_menu(
            &registry.s3_targets,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };
                format!(
                    "{}\r\n{}\r\n{}\r\n Registered S3-compatible cloud providers (Total: {}){}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("S3 STORAGE PROVIDERS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    total_count,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, t| format!("{:<20} s3://{} ({})", t.name, t.bucket, t.region),
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(global_idx) if global_idx < registry.s3_targets.len() => {
                let target_id = registry.s3_targets[global_idx].id.clone();
                manage_single_s3_target_menu(paths, &target_id).await?;
            }
            PagedMenuAction::Action(act) if act == "a" => {
                let _ = add_s3_target_wizard(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn add_s3_target_wizard(paths: &CraftPaths) -> Result<Option<String>> {
    let name = match run_input_prompt("S3 PROVIDER NAME", "Provider Label (e.g. Wasabi EU, Cloudflare R2, AWS Prod):", None)? {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => return Ok(None),
    };

    let bucket = match run_input_prompt("S3 BUCKET", "Bucket Name:", None)? {
        Some(b) if !b.trim().is_empty() => b.trim().to_string(),
        _ => return Ok(None),
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
        _ => return Ok(None),
    };

    let secret_key = match run_password_prompt("SECRET ACCESS KEY", "AWS / R2 Secret Access Key:")? {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => return Ok(None),
    };

    let prefix = match run_input_prompt("STORAGE PREFIX", "Key Prefix / Folder (optional):", Some("craft-backups"))? {
        Some(p) if !p.trim().is_empty() => Some(p.trim().to_string()),
        _ => None,
    };

    let mut reg = GlobalBackupRegistry::load(paths)?;
    let id = slugify(&name, &reg.s3_targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
    let target = S3BackupTarget {
        id: id.clone(),
        name: name.clone(),
        bucket: bucket.clone(),
        region,
        endpoint,
        access_key_id: access_key,
        secret_access_key: secret_key,
        prefix,
    };

    reg.s3_targets.push(target.clone());
    reg.save(paths)?;

    show_modal_message(
        "S3 STORAGE ADDED",
        &[format!("[OK] S3 storage provider '{}' (bucket: {}) configured.", name, bucket).green().bold().to_string()],
        false,
    )?;

    Ok(Some(id))
}

async fn manage_single_s3_target_menu(paths: &CraftPaths, target_id: &str) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let target = match registry.s3_targets.iter().find(|t| t.id == target_id) {
            Some(t) => t.clone(),
            None => return Ok(()),
        };
        let _nav = NavGuard::enter(&target.name);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Name:     {}\r\n Bucket:   {}\r\n Region:   {}\r\n Endpoint: {}\r\n Key ID:   {}\r\n Prefix:   {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("S3 STORAGE: {}", target.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            target.name.white().bold(),
            target.bucket.cyan(),
            target.region,
            target.endpoint.as_deref().unwrap_or("AWS S3 Standard (Default)"),
            target.access_key_id,
            target.prefix.as_deref().unwrap_or("<none>"),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Test Connection"),
            MenuEntry::new("2", "Edit Configuration"),
            MenuEntry::new("3", "Delete Provider"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = print_in_place_status(
                    "TESTING S3 CONNECTION",
                    &[format!("Testing connectivity to s3://{}...", target.bucket)],
                );

                let provider = S3StorageProvider::new(target.clone().into());
                match provider.list_files("").await {
                    Ok(items) => {
                        show_modal_message(
                            "CONNECTION SUCCESS",
                            &[
                                "[OK] Successfully connected to S3 bucket!".green().bold().to_string(),
                                format!("Found {} existing files.", items.len()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "CONNECTION FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(1) => {
                if let Some(new_name) = run_input_prompt("NAME", "Provider Name:", Some(&target.name))? {
                    if let Some(new_b) = run_input_prompt("BUCKET", "Bucket Name:", Some(&target.bucket))? {
                        if let Some(new_r) = run_input_prompt("REGION", "Region:", Some(&target.region))? {
                            let mut reg = GlobalBackupRegistry::load(paths)?;
                            if let Some(t) = reg.s3_targets.iter_mut().find(|t| t.id == target_id) {
                                t.name = new_name.trim().to_string();
                                t.bucket = new_b.trim().to_string();
                                t.region = new_r.trim().to_string();
                                reg.save(paths)?;
                            }
                        }
                    }
                }
            }
            Some(2) => {
                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.s3_targets.retain(|t| t.id != target_id);
                reg.save(paths)?;
                show_modal_message(
                    "PROVIDER REMOVED",
                    &[format!("[OK] Removed S3 provider '{}'.", target.name).green().bold().to_string()],
                    false,
                )?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn gdrive_storage_targets_menu(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Google Drive");
    let mut current_page = 0;
    let page_size = 6;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let width = get_content_width(80);
        let action_entries = vec![
            MenuEntry::new("a", "Add Google Drive").with_aliases(&["add", "n"]),
        ];

        let action = run_paged_list_menu(
            &registry.gdrive_targets,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };
                format!(
                    "{}\r\n{}\r\n{}\r\n Registered Google Drive storage folders for backups (Total: {}){}:\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("GOOGLE DRIVE PROVIDERS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    total_count,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, t| format!("{:<20} Folder: {}", t.name, t.folder_id),
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(global_idx) if global_idx < registry.gdrive_targets.len() => {
                let target_id = registry.gdrive_targets[global_idx].id.clone();
                manage_single_gdrive_target_menu(paths, &target_id).await?;
            }
            PagedMenuAction::Action(act) if act == "a" => {
                let _ = add_gdrive_target_wizard(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn add_gdrive_target_wizard(paths: &CraftPaths) -> Result<Option<String>> {
    let name = match run_input_prompt("GDRIVE NAME", "Provider Label (e.g. Personal Drive, Team Drive):", None)? {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => return Ok(None),
    };

    let folder_id = match run_input_prompt("GDRIVE FOLDER ID", "Google Drive Folder ID:", None)? {
        Some(f) if !f.trim().is_empty() => f.trim().to_string(),
        _ => return Ok(None),
    };

    let auth_header = " Choose Google Drive authentication method:";
    let auth_entries = vec![
        MenuEntry::new("1", "OAuth / API Token"),
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
            let p = sa.map(|s| PathBuf::from(s.trim()));
            (None, p)
        }
        _ => return Ok(None),
    };

    let mut reg = GlobalBackupRegistry::load(paths)?;
    let id = slugify(&name, &reg.gdrive_targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
    let target = GDriveBackupTarget {
        id: id.clone(),
        name: name.clone(),
        folder_id,
        service_account_path,
        api_token,
    };

    reg.gdrive_targets.push(target);
    reg.save(paths)?;

    show_modal_message(
        "GOOGLE DRIVE ADDED",
        &[format!("[OK] Google Drive provider '{}' configured.", name).green().bold().to_string()],
        false,
    )?;

    Ok(Some(id))
}

async fn manage_single_gdrive_target_menu(paths: &CraftPaths, target_id: &str) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = GlobalBackupRegistry::load(paths)?;
        let target = match registry.gdrive_targets.iter().find(|t| t.id == target_id) {
            Some(t) => t.clone(),
            None => return Ok(()),
        };
        let _nav = NavGuard::enter(&target.name);

        let width = get_content_width(80);
        let auth_type_str = if target.api_token.is_some() { "OAuth / API Token" } else { "Service Account Key" };
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Name:      {}\r\n Folder ID: {}\r\n Auth Type: {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("GOOGLE DRIVE: {}", target.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            target.name.white().bold(),
            target.folder_id.cyan(),
            auth_type_str,
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "Test Connection"),
            MenuEntry::new("2", "Edit Configuration"),
            MenuEntry::new("3", "Delete Provider"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let _ = print_in_place_status(
                    "TESTING GDRIVE CONNECTION",
                    &[format!("Testing connectivity to Google Drive folder '{}'...", target.folder_id)],
                );

                let provider = GDriveStorageProvider::new(target.clone().into());
                match provider.list_files("").await {
                    Ok(items) => {
                        show_modal_message(
                            "CONNECTION SUCCESS",
                            &[
                                "[OK] Successfully connected to Google Drive!".green().bold().to_string(),
                                format!("Found {} existing files.", items.len()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "CONNECTION FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            Some(1) => {
                if let Some(new_name) = run_input_prompt("NAME", "Provider Name:", Some(&target.name))? {
                    if let Some(new_f) = run_input_prompt("FOLDER ID", "Folder ID:", Some(&target.folder_id))? {
                        let mut reg = GlobalBackupRegistry::load(paths)?;
                        if let Some(t) = reg.gdrive_targets.iter_mut().find(|t| t.id == target_id) {
                            t.name = new_name.trim().to_string();
                            t.folder_id = new_f.trim().to_string();
                            reg.save(paths)?;
                        }
                    }
                }
            }
            Some(2) => {
                let mut reg = GlobalBackupRegistry::load(paths)?;
                reg.gdrive_targets.retain(|t| t.id != target_id);
                reg.save(paths)?;
                show_modal_message(
                    "PROVIDER REMOVED",
                    &[format!("[OK] Removed Google Drive provider '{}'.", target.name).green().bold().to_string()],
                    false,
                )?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

pub(crate) async fn pick_server_backup_method(paths: &CraftPaths, server_name: &str) -> Result<Option<String>> {
    let mut reg = GlobalBackupRegistry::load(paths)?;
    reg.ensure_defaults(paths);

    let type_header = format!(" Select backup system type for server '{}':", server_name);
    let type_entries = vec![
        MenuEntry::new("1", "Local Storage"),
        MenuEntry::new("2", "S3 Storage"),
        MenuEntry::new("3", "Google Drive"),
        MenuEntry::new("4", "Multi-Destination"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut t_sel = 0;
    let type_choice = run_menu(&type_header, &type_entries, &mut t_sel)?;

    match type_choice {
        Some(0) => {
            // Local Storage targets
            let mut l_sel = 0;
            loop {
                let r = GlobalBackupRegistry::load(paths)?;
                let mut entries = Vec::new();
                for (i, t) in r.local_targets.iter().enumerate() {
                    let hk = (i + 1).to_string();
                    entries.push(MenuEntry::new(hk, format!("{:<20} ({})", t.name, t.path.display())));
                }
                entries.push(MenuEntry::new("a", "Add Local Storage"));
                entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let header = format!(" Select local storage target for '{}':", server_name);
                let num_targets = r.local_targets.len();
                match run_menu(&header, &entries, &mut l_sel)? {
                    Some(idx) if idx < num_targets => {
                        return Ok(Some(format!("local:{}", r.local_targets[idx].id)));
                    }
                    Some(idx) if idx == num_targets => {
                        // Add local
                        if let Some(name) = run_input_prompt("TARGET NAME", "Name:", None)? {
                            if let Some(p_str) = run_input_prompt("DIRECTORY", "Path:", Some(&paths.backups_dir.display().to_string()))? {
                                let p = PathBuf::from(p_str.trim());
                                let _ = std::fs::create_dir_all(&p);
                                let mut updated = GlobalBackupRegistry::load(paths)?;
                                let id = slugify(name.trim(), &updated.local_targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
                                updated.local_targets.push(LocalBackupTarget {
                                    id,
                                    name: name.trim().to_string(),
                                    path: p,
                                });
                                updated.save(paths)?;
                            }
                        }
                    }
                    _ => return Ok(None),
                }
            }
        }
        Some(1) => {
            // S3 Storage targets
            let mut s_sel = 0;
            loop {
                let r = GlobalBackupRegistry::load(paths)?;
                if r.s3_targets.is_empty() {
                    show_modal_message(
                        "NO S3 PROVIDERS",
                        &["No S3 storage providers are registered yet.", "Opening setup wizard now..."],
                        false,
                    )?;
                    if let Some(id) = add_s3_target_wizard(paths).await? {
                        return Ok(Some(format!("s3:{}", id)));
                    } else {
                        return Ok(None);
                    }
                }

                let mut entries = Vec::new();
                for (i, t) in r.s3_targets.iter().enumerate() {
                    let hk = (i + 1).to_string();
                    entries.push(MenuEntry::new(hk, format!("{:<20} (s3://{})", t.name, t.bucket)));
                }
                entries.push(MenuEntry::new("a", "Add S3 Storage"));
                entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let header = format!(" Select S3 storage provider for '{}':", server_name);
                let num_targets = r.s3_targets.len();
                match run_menu(&header, &entries, &mut s_sel)? {
                    Some(idx) if idx < num_targets => {
                        return Ok(Some(format!("s3:{}", r.s3_targets[idx].id)));
                    }
                    Some(idx) if idx == num_targets => {
                        if let Some(id) = add_s3_target_wizard(paths).await? {
                            return Ok(Some(format!("s3:{}", id)));
                        }
                    }
                    _ => return Ok(None),
                }
            }
        }
        Some(2) => {
            // Google Drive targets
            let mut g_sel = 0;
            loop {
                let r = GlobalBackupRegistry::load(paths)?;
                if r.gdrive_targets.is_empty() {
                    show_modal_message(
                        "NO GDRIVE PROVIDERS",
                        &["No Google Drive providers are registered yet.", "Opening setup wizard now..."],
                        false,
                    )?;
                    if let Some(id) = add_gdrive_target_wizard(paths).await? {
                        return Ok(Some(format!("gdrive:{}", id)));
                    } else {
                        return Ok(None);
                    }
                }

                let mut entries = Vec::new();
                for (i, t) in r.gdrive_targets.iter().enumerate() {
                    let hk = (i + 1).to_string();
                    entries.push(MenuEntry::new(hk, format!("{:<20} (folder: {})", t.name, t.folder_id)));
                }
                entries.push(MenuEntry::new("a", "Add Google Drive"));
                entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let header = format!(" Select Google Drive provider for '{}':", server_name);
                let num_targets = r.gdrive_targets.len();
                match run_menu(&header, &entries, &mut g_sel)? {
                    Some(idx) if idx < num_targets => {
                        return Ok(Some(format!("gdrive:{}", r.gdrive_targets[idx].id)));
                    }
                    Some(idx) if idx == num_targets => {
                        if let Some(id) = add_gdrive_target_wizard(paths).await? {
                            return Ok(Some(format!("gdrive:{}", id)));
                        }
                    }
                    _ => return Ok(None),
                }
            }
        }
        Some(3) => Ok(Some("multi".to_string())),
        _ => Ok(None),
    }
}

pub(crate) async fn configure_policies_menu(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Policies");
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
            "{}\r\n{}\r\n{}\r\n Select a server to configure automated backup schedule & destinations:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("AUTO-BACKUP POLICIES", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, s) in servers_reg.servers.iter().enumerate() {
            let hk = (i + 1).to_string();
            let policy = reg.server_policies.get(&s.name);
            let status = if let Some(p) = policy {
                if p.enabled {
                    let method_name = reg.format_method_display(p.backup_method.as_deref().or(s.backup_method.as_deref()));
                    format!("[EVERY {}h | KEEP {} | {}]", p.interval_hours, p.retention_count, method_name).green().bold().to_string()
                } else {
                    "[DISABLED]".dimmed().to_string()
                }
            } else {
                "[DISABLED]".dimmed().to_string()
            };
            entries.push(MenuEntry::new(hk, format!("{:<20} {}", s.name, status)));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match super::screen::run_menu_with_space(&header, &entries, &mut selected)? {
            super::screen::MenuAction::Space(idx) => {
                if idx < servers_reg.servers.len() {
                    let server_name = &servers_reg.servers[idx].name;
                    let mut reg = GlobalBackupRegistry::load(paths)?;
                    let current = reg.server_policies.entry(server_name.to_string()).or_default();
                    current.enabled = !current.enabled;
                    let _ = reg.save(paths);
                }
                continue;
            }
            super::screen::MenuAction::Select(idx) if idx < servers_reg.servers.len() => {
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

        let s_reg = ServersRegistry::load(paths)?;
        let s_method = s_reg.servers.iter().find(|s| s.name == server_name).and_then(|s| s.backup_method.clone());
        let effective_method = policy.backup_method.as_deref().or(s_method.as_deref());
        let method_display = reg.format_method_display(effective_method);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:            {}\r\n Automated Backups: {}\r\n Schedule Interval: Every {} Hours\r\n Retention Policy:  Keep {} Latest Archives\r\n Backup Destination: {}\r\n World Only Scope:  {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("AUTO-BACKUP: {}", server_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name.white().bold(),
            if policy.enabled { "[ENABLED]".green().bold() } else { "[DISABLED]".dimmed() },
            policy.interval_hours.to_string().cyan().bold(),
            policy.retention_count.to_string().yellow().bold(),
            method_display.cyan().bold(),
            if policy.world_only { "[YES]".cyan().bold() } else { "[FULL SERVER]".white() },
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", format!("Toggle Status ({})", if policy.enabled { "Disable" } else { "Enable" })),
            MenuEntry::new("2", "Change Interval (Hours)"),
            MenuEntry::new("3", "Change Retention Count"),
            MenuEntry::new("4", "Change Backup Destination"),
            MenuEntry::new("5", format!("Toggle Scope ({})", if policy.world_only { "Switch to Full" } else { "Switch to World Only" })),
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
                if let Some(new_method) = pick_server_backup_method(paths, server_name).await? {
                    let current = reg.server_policies.entry(server_name.to_string()).or_default();
                    current.backup_method = Some(new_method.clone());
                    reg.save(paths)?;

                    let mut s_reg = ServersRegistry::load(paths)?;
                    if let Some(s) = s_reg.servers.iter_mut().find(|s| s.name == server_name) {
                        s.backup_method = Some(new_method);
                        let _ = s_reg.save(paths);
                    }
                }
            }
            Some(4) => {
                let current = reg.server_policies.entry(server_name.to_string()).or_default();
                current.world_only = !current.world_only;
                reg.save(paths)?;
            }
            _ => return Ok(()),
        }
    }
}


fn slugify(name: &str, existing: &[&str]) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let mut candidate = base.trim_matches('-').to_string();
    if candidate.is_empty() {
        candidate = "target".to_string();
    }
    if !existing.contains(&candidate.as_str()) {
        return candidate;
    }
    let mut i = 2;
    loop {
        let next = format!("{}-{}", candidate, i);
        if !existing.contains(&next.as_str()) {
            return next;
        }
        i += 1;
    }
}
