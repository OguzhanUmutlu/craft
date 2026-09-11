use std::path::Path;
use colored::Colorize;
use craft_core::{CraftError, Result};
use crate::session::RemoteSession;
use crate::sftp_ops::SftpOps;

pub fn sync_local_to_remote(
    session: &RemoteSession,
    local_dir: &Path,
    remote_dir: &Path,
) -> Result<()> {
    if !local_dir.exists() {
        return Err(CraftError::InvalidPath(local_dir.to_string_lossy().to_string()));
    }

    let sftp_ops = SftpOps::new(session);
    sftp_ops.create_remote_dir_all(remote_dir)?;

    println!("{}", format!("Synchronizing '{}' -> remote '{}'...", local_dir.display(), remote_dir.display()).cyan());

    for entry in walkdir(local_dir)? {
        let rel = entry.strip_prefix(local_dir).map_err(|e| CraftError::Other(e.to_string()))?;
        let remote_dest = remote_dir.join(rel);

        if entry.is_dir() {
            let _ = sftp_ops.create_remote_dir_all(&remote_dest);
        } else if entry.is_file() {
            if let Some(parent) = remote_dest.parent() {
                let _ = sftp_ops.create_remote_dir_all(parent);
            }
            sftp_ops.upload_file(&entry, &remote_dest)?;
        }
    }

    println!("{}", "[OK] Synchronization complete!".green().bold());
    Ok(())
}

fn walkdir(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.push(path.clone());
                files.extend(walkdir(&path)?);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}
