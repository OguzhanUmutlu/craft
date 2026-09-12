use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftPaths, Result};
use craft_remote::{RemoteCraftClient, RemoteServerInfo};
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status,
    run_input_prompt, run_menu, run_paged_list_menu, show_modal_message,
    MenuEntry, NavGuard, PagedMenuAction,
};

pub async fn manage_remote_backups(
    paths: &CraftPaths,
    client: &RemoteCraftClient,
    server: &RemoteServerInfo,
) -> Result<()> {
    let _nav = NavGuard::enter("Backups");
    let mut current_page = 0;
    let page_size = 7;

    loop {
        let (is_running, pid) = client.check_server_running(&server.name, &server.path);
        let backups = match client.list_backups(&server.name) {
            Ok(b) => b,
            Err(e) => {
                show_modal_message(
                    "REMOTE BACKUPS ERROR",
                    &[format!("Failed to list remote backups: {}", e)],
                    true,
                )?;
                return Ok(());
            }
        };

        let status_str = if is_running {
            if let Some(p) = pid {
                format!("[RUNNING (PID: {})]", p).green().bold().to_string()
            } else {
                "[RUNNING]".green().bold().to_string()
            }
        } else {
            "[STOPPED]".dimmed().to_string()
        };

        let width = get_content_width(80);
        let action_entries = vec![
            MenuEntry::new("c", "Create Backup").with_aliases(&["n"]),
        ];

        let action = run_paged_list_menu(
            &backups,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "No backups found for this server.".dimmed().to_string()
                } else {
                    format!("Total Archives: {}", total_count).white().bold().to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Server: {:<20} | Status: {}\r\n Backups Directory: ~/.craft/backups/{}/\r\n {}{}\r\n Note: Server must be STOPPED before restoring to prevent corruption.\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title(&format!("REMOTE BACKUPS: {}", server.name), width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    server.name.white().bold(),
                    status_str,
                    server.name,
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, b| {
                let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                let size_label = if b.size_bytes > 0 {
                    format!("{:.1} MB", mb)
                } else {
                    "archive".to_string()
                };
                format!("{:<36} ({}, {})", b.filename, size_label, b.created_at)
            },
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Action(act) if act == "c" => {
                // Create backup
                let scope_header = " Choose remote backup scope:";
                let scope_entries = vec![
                    MenuEntry::new("1", "Full Backup"),
                    MenuEntry::new("2", "World Only"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];
                let mut scope_sel = 0;
                if let Some(s_idx) = run_menu(scope_header, &scope_entries, &mut scope_sel)? {
                    let world_only = match s_idx {
                        0 => false,
                        1 => true,
                        _ => continue,
                    };

                    print_in_place_status(
                        "CREATING REMOTE BACKUP",
                        &[format!("Creating snapshot for '{}' on remote host...", server.name)],
                    )?;

                    match client.create_backup(&server.name, world_only) {
                        Ok(_) => {
                            show_modal_message(
                                "BACKUP CREATED",
                                &[format!(
                                    "[OK] Remote backup successfully generated for '{}'.",
                                    server.name
                                )
                                .green()
                                .bold()
                                .to_string()],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "BACKUP FAILED",
                                &[format!("[ERROR] {}", e)],
                                true,
                            )?;
                        }
                    }
                }
            }
            PagedMenuAction::Select(global_idx) if global_idx < backups.len() => {
                let backup = &backups[global_idx];

                let action_header = format!(
                    " Backup Archive: {}\r\n Size: {:.2} MB | Created: {}\r\n Select action:",
                    backup.filename.white().bold(),
                    (backup.size_bytes as f64) / (1024.0 * 1024.0),
                    backup.created_at
                );

                let action_entries = vec![
                    MenuEntry::new("1", "Restore Backup"),
                    MenuEntry::new("2", "Download Backup"),
                    MenuEntry::new("3", "Move to Trash"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];

                let mut action_sel = 0;
                if let Some(act) = run_menu(&action_header, &action_entries, &mut action_sel)? {
                    match act {
                        0 => {
                            // Restore
                            let (running_now, cur_pid) = client.check_server_running(&server.name, &server.path);
                            if running_now {
                                let pid_info = cur_pid.map(|p| format!(" (PID: {})", p)).unwrap_or_default();
                                show_modal_message(
                                    "RESTORE BLOCKED: SERVER IS RUNNING",
                                    &[
                                        format!(
                                            "Cannot restore backup to server '{}': The remote server is currently RUNNING{}.",
                                            server.name, pid_info
                                        ),
                                        "You MUST stop the server before restoring a backup to prevent world corruption.".to_string(),
                                        "".to_string(),
                                        "Please stop the server first from the Remote Server Control Panel.".to_string(),
                                    ],
                                    true,
                                )?;
                                continue;
                            }

                            // Confirmation prompt
                            let confirm_header = format!(
                                "{}\r\n{}\r\n{}\r\n WARNING: Restoring will overwrite server files with archive '{}'!\r\n Server: {}\r\n Path:   {}\r\n{}\r\n Are you sure you want to proceed with restore?\r\n{}",
                                box_top(width).yellow().bold(),
                                box_title("CONFIRM BACKUP RESTORE", width, false).yellow().bold(),
                                box_divider(width).yellow().bold(),
                                backup.filename.white().bold(),
                                server.name.white().bold(),
                                server.path,
                                box_divider(width).dimmed(),
                                box_divider(width).dimmed(),
                            );

                            let confirm_entries = vec![
                                MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                                MenuEntry::new("2", format!("Confirm Restore of '{}'", backup.filename)),
                            ];

                            let mut c_sel = 0;
                            if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                                let _ = print_in_place_status(
                                    "RESTORING REMOTE BACKUP",
                                    &[format!("Restoring '{}' on remote server...", backup.filename)],
                                );

                                match client.restore_backup(&server.name, &server.path, &backup.filename) {
                                    Ok(_) => {
                                        show_modal_message(
                                            "RESTORE COMPLETE",
                                            &[format!(
                                                "[OK] Successfully restored backup '{}' to server '{}'.",
                                                backup.filename, server.name
                                            )
                                            .green()
                                            .bold()
                                            .to_string()],
                                            false,
                                        )?;
                                    }
                                    Err(e) => {
                                        show_modal_message(
                                            "RESTORE FAILED",
                                            &[format!("[ERROR] {}", e)],
                                            true,
                                        )?;
                                    }
                                }
                            }
                        }
                        1 => {
                            // Download
                            let dest_header = " Select local destination for downloaded backup:";
                            let dest_entries = vec![
                                MenuEntry::new("1", "Current Directory (.)"),
                                MenuEntry::new("2", "Downloads Directory (~/.craft/downloads)"),
                                MenuEntry::new("3", "Custom Directory"),
                                MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                            ];

                            let mut dest_sel = 0;
                            let target_dir = match run_menu(dest_header, &dest_entries, &mut dest_sel)? {
                                Some(0) => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                                Some(1) => {
                                    let p = paths.home.join("downloads");
                                    let _ = std::fs::create_dir_all(&p);
                                    p
                                }
                                Some(2) => {
                                    match run_input_prompt(
                                        "CUSTOM DESTINATION",
                                        "Enter local directory to save backup:",
                                        None,
                                    )? {
                                        Some(d) if !d.trim().is_empty() => PathBuf::from(d.trim()),
                                        _ => continue,
                                    }
                                }
                                _ => continue,
                            };

                            print_in_place_status(
                                "DOWNLOADING BACKUP",
                                &[
                                    format!("Downloading '{}' from remote host via SFTP...", backup.filename),
                                    format!("Destination directory: {}", target_dir.display()),
                                ],
                            )?;

                            match client.download_backup(&server.name, &backup.filename, &target_dir) {
                                Ok(dest_file) => {
                                    show_modal_message(
                                        "DOWNLOAD COMPLETE",
                                        &[
                                            "[OK] Backup archive downloaded successfully!".green().bold().to_string(),
                                            format!("Saved to: {}", dest_file.display()),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "DOWNLOAD FAILED",
                                        &[format!("[ERROR] {}", e)],
                                        true,
                                    )?;
                                }
                            }
                        }
                        2 => {
                            // Move to Trash
                            let _ = print_in_place_status(
                                "TRASHING BACKUP",
                                &[format!("Moving '{}' to remote trash...", backup.filename)],
                            );
                            match client.trash_backup(&server.name, &backup.filename) {
                                Ok(_) => {
                                    show_modal_message(
                                        "BACKUP TRASHED",
                                        &[format!("[OK] Moved '{}' to remote trash bin (~/.craft/trash/).", backup.filename).green().bold().to_string()],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message("ERROR", &[format!("[ERROR] Failed to trash backup: {}", e)], true)?;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}
