use std::fs;
use crate::engine::BackupEngine;

pub fn enforce_retention(engine: &BackupEngine, server_name: &str, max_backups_to_keep: usize) {
    let mut backups = engine.list_backups(server_name);
    if backups.len() > max_backups_to_keep {
        // Backups are sorted newest first; drain everything after max_backups_to_keep
        for excess in backups.drain(max_backups_to_keep..) {
            let _ = fs::remove_file(excess.path);
        }
    }
}
