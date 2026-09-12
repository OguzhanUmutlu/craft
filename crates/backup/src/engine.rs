use std::fs::{self, File};
use std::path::{Path, PathBuf};
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use craft_core::{CraftError, CraftPaths, Result};
use craft_net::RconClient;

#[derive(Debug, Clone)]
pub struct BackupMetadata {
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: String,
}

pub struct BackupEngine {
    backups_dir: PathBuf,
}

impl BackupEngine {
    pub fn new(paths: &CraftPaths) -> Self {
        let dir = if let Ok(reg) = craft_core::GlobalBackupRegistry::load(paths) {
            reg.default_local_path(paths)
        } else {
            paths.backups_dir.clone()
        };
        Self {
            backups_dir: dir,
        }
    }

    pub fn with_dir(backups_dir: PathBuf) -> Self {
        Self { backups_dir }
    }

    pub fn server_backup_dir(&self, server_name: &str) -> PathBuf {
        self.backups_dir.join(server_name)
    }

    /// Performs an atomic backup of a server, optionally flushing with RCON
    pub async fn create_backup(
        &self,
        server_name: &str,
        server_path: &Path,
        rcon: Option<(&str, u16, &str)>, // (host, port, password)
        world_only: bool,
    ) -> Result<PathBuf> {
        if !server_path.exists() {
            return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
        }

        let mut rcon_client = None;
        if let Some((host, port, pass)) = rcon {
            if let Ok(mut client) = RconClient::connect(host, port, pass).await {
                let _ = client.send_command("save-off").await;
                let _ = client.send_command("save-all flush").await;
                rcon_client = Some(client);
            }
        }

        let target_dir = self.server_backup_dir(server_name);
        fs::create_dir_all(&target_dir)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let suffix = if world_only { "_world" } else { "" };
        let archive_name = format!("{}_{}{}.tar.gz", server_name, timestamp, suffix);
        let archive_path = target_dir.join(&archive_name);

        let res = compress_server_directory(server_path, &archive_path, world_only);

        // Resume auto-saving if RCON was connected
        if let Some(mut client) = rcon_client {
            let _ = client.send_command("save-on").await;
        }

        res?;
        Ok(archive_path)
    }

    pub fn list_backups(&self, server_name: &str) -> Vec<BackupMetadata> {
        let dir = self.server_backup_dir(server_name);
        let mut list = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map(|e| e == "gz").unwrap_or(false) {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let created_at = entry
                        .metadata()
                        .and_then(|m| m.created())
                        .map(|c| {
                            let dt: chrono::DateTime<Utc> = c.into();
                            dt.format("%Y-%m-%d %H:%M:%S").to_string()
                        })
                        .unwrap_or_else(|_| "Unknown".to_string());

                    list.push(BackupMetadata {
                        filename,
                        path,
                        size_bytes,
                        created_at,
                    });
                }
            }
        }

        list.sort_by(|a, b| b.filename.cmp(&a.filename));
        list
    }

    pub fn restore_backup(&self, backup_file: &Path, server_path: &Path) -> Result<()> {
        if !backup_file.exists() {
            return Err(CraftError::InvalidPath(backup_file.to_string_lossy().to_string()));
        }

        // Strict safety check: Never restore a backup while server is running!
        if craft_core::process::is_server_locked(server_path)
            || craft_core::process::get_server_running_pid(server_path).is_some()
        {
            return Err(CraftError::Other(format!(
                "Cannot restore backup to server at '{}': The server is currently running. You MUST stop the server before restoring a backup to prevent world corruption.",
                server_path.display()
            )));
        }

        let file = File::open(backup_file)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        fs::create_dir_all(server_path)?;
        archive.unpack(server_path)
            .map_err(|e| CraftError::Other(format!("Failed to unpack backup archive: {}", e)))?;

        Ok(())
    }

    /// Downloads a backup from a storage provider and restores it safely
    pub async fn restore_from_provider<P: crate::providers::StorageProvider>(
        &self,
        provider: &P,
        remote_key: &str,
        server_path: &Path,
    ) -> Result<()> {
        // Strict safety check upfront before downloading
        if craft_core::process::is_server_locked(server_path)
            || craft_core::process::get_server_running_pid(server_path).is_some()
        {
            return Err(CraftError::Other(format!(
                "Cannot restore backup to server at '{}': The server is currently running. You MUST stop the server before restoring a backup to prevent world corruption.",
                server_path.display()
            )));
        }

        let temp_dir = tempfile::tempdir()
            .map_err(|e| CraftError::Other(format!("Failed to create temporary directory for restore: {}", e)))?;
        let temp_file = temp_dir.path().join("downloaded_backup.tar.gz");

        provider.download_file(remote_key, &temp_file).await?;
        self.restore_backup(&temp_file, server_path)?;

        Ok(())
    }
}


fn should_exclude(rel_path: &Path) -> bool {
    let components: Vec<_> = rel_path.iter().map(|c| c.to_string_lossy().to_string()).collect();
    for comp in &components {
        let lower = comp.to_lowercase();
        if lower == "logs" || lower == "crash-reports" || lower == "cache" || lower == ".craft" {
            return true;
        }
    }

    if let Some(file_name) = rel_path.file_name().and_then(|f| f.to_str()) {
        let lower = file_name.to_lowercase();
        if lower.ends_with(".tmp") || lower.ends_with(".sock") || lower.ends_with(".pid") || lower == "session.lock" {
            return true;
        }
    }

    false
}

fn is_world_or_config(rel_path: &Path, is_dir: bool) -> bool {
    let comp = if let Some(first) = rel_path.iter().next() {
        first.to_string_lossy().to_lowercase()
    } else {
        return false;
    };

    // World root folders
    if comp.starts_with("world") || comp == "dim-1" || comp == "dim1" || comp == "worlds" || comp == "db" {
        return true;
    }

    // Config directories
    if comp == "config" || comp == "defaultconfigs" || comp == "plugins" {
        return true;
    }

    // Root config / script files
    if !is_dir && rel_path.parent().map(|p| p.as_os_str().is_empty()).unwrap_or(true) {
        let lower = comp;
        if lower.ends_with(".properties")
            || lower.ends_with(".yml")
            || lower.ends_with(".yaml")
            || lower.ends_with(".toml")
            || lower.ends_with(".json")
            || lower.ends_with(".txt")
            || lower.ends_with(".sh")
            || lower.ends_with(".cmd")
            || lower.ends_with(".bat")
        {
            return true;
        }
    }

    false
}

fn compress_server_directory(source_dir: &Path, output_tar_gz: &Path, world_only: bool) -> Result<()> {
    let file = File::create(output_tar_gz)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let mut stack = vec![source_dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel_path = match path.strip_prefix(source_dir) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if should_exclude(rel_path) {
                    continue;
                }

                let is_dir = path.is_dir();

                if world_only && !is_world_or_config(rel_path, is_dir) {
                    continue;
                }

                if is_dir {
                    tar.append_dir(rel_path, &path)
                        .map_err(|e| CraftError::Other(format!("Failed to append dir {}: {}", rel_path.display(), e)))?;
                    stack.push(path);
                } else {
                    let mut f = File::open(&path)?;
                    tar.append_file(rel_path, &mut f)
                        .map_err(|e| CraftError::Other(format!("Failed to append file {}: {}", rel_path.display(), e)))?;
                }
            }
        }
    }

    tar.finish()
        .map_err(|e| CraftError::Other(format!("Failed to finalize archive: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_exclusions_and_world_only() {
        let tmp = tempdir().unwrap();
        let server_dir = tmp.path().join("server");
        fs::create_dir_all(server_dir.join("world/data")).unwrap();
        fs::create_dir_all(server_dir.join("logs")).unwrap();
        fs::create_dir_all(server_dir.join("cache")).unwrap();

        fs::write(server_dir.join("world/level.dat"), b"level_data").unwrap();
        fs::write(server_dir.join("logs/latest.log"), b"log_data").unwrap();
        fs::write(server_dir.join("cache/some.cache"), b"cache_data").unwrap();
        fs::write(server_dir.join("server.properties"), b"motd=Test").unwrap();
        fs::write(server_dir.join("session.lock"), b"lock").unwrap();
        fs::write(server_dir.join("server.jar"), b"fake_jar").unwrap();

        // 1. Full backup (should exclude logs, cache, session.lock, but include server.jar)
        let full_archive = tmp.path().join("full.tar.gz");
        compress_server_directory(&server_dir, &full_archive, false).unwrap();

        let restore_dir = tmp.path().join("restore_full");
        let file = File::open(&full_archive).unwrap();
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        archive.unpack(&restore_dir).unwrap();

        assert!(restore_dir.join("world/level.dat").exists());
        assert!(restore_dir.join("server.properties").exists());
        assert!(restore_dir.join("server.jar").exists());
        assert!(!restore_dir.join("logs/latest.log").exists());
        assert!(!restore_dir.join("cache/some.cache").exists());
        assert!(!restore_dir.join("session.lock").exists());

        // 2. World-only backup (should only include world and config files, not server.jar)
        let world_archive = tmp.path().join("world.tar.gz");
        compress_server_directory(&server_dir, &world_archive, true).unwrap();

        let restore_world = tmp.path().join("restore_world");
        let file_w = File::open(&world_archive).unwrap();
        let gz_w = flate2::read::GzDecoder::new(file_w);
        let mut archive_w = tar::Archive::new(gz_w);
        archive_w.unpack(&restore_world).unwrap();

        assert!(restore_world.join("world/level.dat").exists());
        assert!(restore_world.join("server.properties").exists());
        assert!(!restore_world.join("server.jar").exists());
        assert!(!restore_world.join("logs/latest.log").exists());
    }

    #[test]
    fn test_restore_fails_when_server_is_running() {
        let tmp = tempdir().unwrap();
        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();
        fs::write(server_dir.join("world.txt"), b"original").unwrap();

        // Create a backup archive
        let archive_path = tmp.path().join("backup.tar.gz");
        compress_server_directory(&server_dir, &archive_path, false).unwrap();

        // Simulate server running by locking it with ServerLockGuard
        let guard = craft_core::process::ServerLockGuard::acquire(&server_dir).unwrap();

        let engine = BackupEngine {
            backups_dir: tmp.path().join("backups"),
        };

        // Restoring should fail because server is running/locked
        let result = engine.restore_backup(&archive_path, &server_dir);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("server is currently running"));

        drop(guard);

        // Once dropped, restore should succeed
        let result_ok = engine.restore_backup(&archive_path, &server_dir);
        assert!(result_ok.is_ok());
    }
}

