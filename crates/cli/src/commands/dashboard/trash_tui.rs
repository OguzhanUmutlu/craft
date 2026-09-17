use colored::Colorize;
use craft_core::{CraftPaths, Result, TrashManager};

use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_menu,
    run_paged_list_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard, PagedMenuAction,
};

pub async fn trash_bin_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Trash Bin");
    let manager = TrashManager::new(paths);
    let mut current_page = 0;
    let page_size = 7;

    loop {
        let items = match manager.list_items() {
            Ok(i) => i,
            Err(e) => {
                show_modal_message(
                    "TRASH ERROR",
                    &[format!("Failed to load trash manifest: {}", e)],
                    true,
                )?;
                return Ok(());
            }
        };

        let total_bytes: u64 = items.iter().map(|i| i.size_bytes).sum();
        let total_mb = (total_bytes as f64) / (1024.0 * 1024.0);
        let width = get_content_width(80);

        let action_entries = if items.is_empty() {
            vec![]
        } else {
            vec![MenuEntry::new("e", "Empty Trash Bin")]
        };

        let action = run_paged_list_menu(
            &items,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "Trash is empty.".dimmed().to_string()
                } else {
                    format!("Total Trashed: {} ({:.2} MB total)", total_count, total_mb)
                        .white()
                        .bold()
                        .to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages)
                        .cyan()
                        .to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Directory: {}\r\n {}{}\r\n Backups moved to trash are securely preserved with SHA-256 integrity hashes.\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("TRASH BIN (LOCAL BACKUPS)", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    paths.trash_dir.display().to_string().dimmed(),
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, item| {
                let mb = (item.size_bytes as f64) / (1024.0 * 1024.0);
                let srv = item.server_name.as_deref().unwrap_or("standalone");
                let short_hash = if item.content_hash.len() >= 8 {
                    &item.content_hash[..8]
                } else {
                    &item.content_hash
                };
                format!(
                    "{:<32} {:<12} {:>7.1} MB  hash:{}  {}",
                    item.original_name,
                    format!("[{}]", srv).cyan(),
                    mb,
                    short_hash.dimmed(),
                    item.trashed_at
                )
            },
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(global_idx) => {
                if global_idx >= items.len() {
                    continue;
                }
                let target_id = items[global_idx].id.clone();
                let fresh_items = manager.list_items().unwrap_or_default();
                let item = match fresh_items.into_iter().find(|i| i.id == target_id) {
                    Some(i) => i,
                    None => {
                        show_modal_message(
                            "TRASH ITEM NOT FOUND",
                            &[
                                "This trash item was restored or removed by another process."
                                    .to_string(),
                            ],
                            true,
                        )?;
                        continue;
                    }
                };
                let item_mb = (item.size_bytes as f64) / (1024.0 * 1024.0);

                let detail_header = format!(
                    "{}\r\n{}\r\n{}\r\n Archive:       {}\r\n Server:        {}\r\n Original Path: {}\r\n Size:          {:.2} MB\r\n Trashed Date:  {}\r\n SHA-256 Hash:  {}\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("TRASHED ARCHIVE DETAILS", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    item.original_name.white().bold(),
                    item.server_name.as_deref().unwrap_or("none").cyan(),
                    item.original_path.display(),
                    item_mb,
                    item.trashed_at,
                    item.content_hash.dimmed(),
                    box_divider(width).dimmed(),
                );

                let detail_entries = vec![
                    MenuEntry::new("1", "Restore to Original Location"),
                    MenuEntry::new("2", "Delete Permanently"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
                ];

                let mut sel = 0;
                if let Some(act) = run_menu(&detail_header, &detail_entries, &mut sel)? {
                    match act {
                        0 => {
                            // Restore
                            let _ = print_in_place_status(
                                "VERIFYING & RESTORING ARCHIVE",
                                &[
                                    format!(
                                        "Verifying SHA-256 hash for '{}'...",
                                        item.original_name
                                    ),
                                    format!("Restoring to '{}'...", item.original_path.display()),
                                ],
                            );
                            match manager.restore_item(&item.id) {
                                Ok(dest) => {
                                    show_modal_message(
                                        "RESTORE SUCCESSFUL",
                                        &[
                                            format!(
                                                "[OK] Successfully verified and restored '{}'!",
                                                item.original_name
                                            )
                                            .green()
                                            .bold()
                                            .to_string(),
                                            format!("Restored to: {}", dest.display()),
                                            format!(
                                                "Integrity hash verified: {}",
                                                item.content_hash
                                            )
                                            .dimmed()
                                            .to_string(),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message(
                                        "RESTORE FAILED",
                                        &[format!("[ERROR] Failed to restore archive: {}", e)],
                                        true,
                                    )?;
                                }
                            }
                        }
                        1 => {
                            // Permanent delete confirmation
                            let confirm_header = format!(
                                "{}\r\n{}\r\n{}\r\n Are you sure you want to PERMANENTLY delete '{}'?\r\n This cannot be recovered.\r\n{}",
                                box_top(width).red().bold(),
                                box_title("CONFIRM PERMANENT DELETION", width, false).red().bold(),
                                box_divider(width).red().bold(),
                                item.original_name.white().bold(),
                                box_divider(width).dimmed(),
                            );
                            let confirm_entries = vec![
                                MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                                MenuEntry::new(
                                    "2",
                                    format!("Confirm Delete of '{}'", item.original_name),
                                ),
                            ];
                            let mut c_sel = 0;
                            if let Some(1) =
                                run_menu(&confirm_header, &confirm_entries, &mut c_sel)?
                            {
                                match manager.delete_permanently(&item.id) {
                                    Ok(_) => {
                                        show_modal_message(
                                            "ITEM DELETED",
                                            &[format!(
                                                "[OK] Permanently deleted '{}'.",
                                                item.original_name
                                            )
                                            .green()
                                            .bold()
                                            .to_string()],
                                            false,
                                        )?;
                                    }
                                    Err(e) => {
                                        show_modal_message(
                                            "DELETE FAILED",
                                            &[format!("[ERROR] {}", e)],
                                            true,
                                        )?;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            PagedMenuAction::Action(act) if act == "e" => {
                // Empty trash confirmation
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Are you sure you want to permanently delete ALL {} items in the trash bin?\r\n Total Size: {:.2} MB\r\n This action cannot be undone.\r\n{}",
                    box_top(width).red().bold(),
                    box_title("CONFIRM EMPTY TRASH", width, false).red().bold(),
                    box_divider(width).red().bold(),
                    items.len(),
                    total_mb,
                    box_divider(width).dimmed(),
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Cancel").with_aliases(&["0", "b"]),
                    MenuEntry::new("2", "Confirm Empty Trash"),
                ];
                let mut c_sel = 0;
                if let Some(1) = run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    match manager.empty_trash() {
                        Ok(count) => {
                            show_modal_message(
                                "TRASH EMPTIED",
                                &[format!(
                                    "[OK] Permanently deleted {} items from the trash bin.",
                                    count
                                )
                                .green()
                                .bold()
                                .to_string()],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message(
                                "ERROR",
                                &[format!("[ERROR] Failed to empty trash: {}", e)],
                                true,
                            )?;
                        }
                    }
                }
            }
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}
