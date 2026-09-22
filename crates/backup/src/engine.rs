use chrono::Utc;
use craft_core::{CraftError, CraftPaths, Result};
use craft_net::RconClient;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackupFormat {
    TarZstd,
    TarGz,
}

impl BackupFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            BackupFormat::TarZstd => "tar.zst",
            BackupFormat::TarGz => "tar.gz",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            BackupFormat::TarZstd => "Zstd",
            BackupFormat::TarGz => "Gzip",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        if lower == "zstd" || lower == "zst" || lower == "tar.zst" || lower == "tzst" {
            Some(BackupFormat::TarZstd)
        } else if lower == "gzip" || lower == "gz" || lower == "tar.gz" || lower == "tgz" {
            Some(BackupFormat::TarGz)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupMetadata {
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: String,
    pub format: BackupFormat,
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
        Self { backups_dir: dir }
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
        format: Option<BackupFormat>,
    ) -> Result<PathBuf> {
        if !server_path.exists() {
            return Err(CraftError::InvalidPath(
                server_path.to_string_lossy().to_string(),
            ));
        }

        let fmt = format.unwrap_or(BackupFormat::TarZstd);

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
        let archive_name = format!("{}_{}{}.{}", server_name, timestamp, suffix, fmt.extension());
        let archive_path = target_dir.join(&archive_name);

        let res = compress_server_directory(server_path, &archive_path, world_only, fmt);

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
                if path.is_file() {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let lower = filename.to_lowercase();
                    let format_opt = if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
                        Some(BackupFormat::TarZstd)
                    } else if lower.ends_with(".tar.gz")
                        || lower.ends_with(".tgz")
                        || path.extension().map(|e| e == "gz").unwrap_or(false)
                    {
                        Some(BackupFormat::TarGz)
                    } else {
                        None
                    };

                    if let Some(fmt) = format_opt {
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
                            format: fmt,
                        });
                    }
                }
            }
        }

        list.sort_by(|a, b| b.filename.cmp(&a.filename));
        list
    }

    pub fn restore_backup(&self, backup_file: &Path, server_path: &Path) -> Result<()> {
        if !backup_file.exists() {
            return Err(CraftError::InvalidPath(
                backup_file.to_string_lossy().to_string(),
            ));
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

        // Detect archive compression via magic bytes
        let mut f = File::open(backup_file)?;
        let mut magic = [0u8; 4];
        let bytes_read = f.read(&mut magic).unwrap_or(0);

        let is_zstd = bytes_read >= 4 && magic == [0x28, 0xB5, 0x2F, 0xFD];
        let is_gzip = bytes_read >= 2 && magic[0] == 0x1F && magic[1] == 0x8B;

        fs::create_dir_all(server_path)?;
        let file = File::open(backup_file)?;

        if is_zstd {
            let decoder = zstd::stream::read::Decoder::new(file)
                .map_err(|e| CraftError::Other(format!("Failed to initialize zstd decoder: {}", e)))?;
            let mut archive = tar::Archive::new(decoder);
            archive
                .unpack(server_path)
                .map_err(|e| CraftError::Other(format!("Failed to unpack zstd backup archive: {}", e)))?;
        } else if is_gzip {
            let gz = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(gz);
            archive
                .unpack(server_path)
                .map_err(|e| CraftError::Other(format!("Failed to unpack gzip backup archive: {}", e)))?;
        } else {
            // Fallback: check file extension
            let fname_lower = backup_file
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("")
                .to_lowercase();
            if fname_lower.ends_with(".zst") || fname_lower.ends_with(".tzst") {
                let decoder = zstd::stream::read::Decoder::new(file)
                    .map_err(|e| CraftError::Other(format!("Failed to initialize zstd decoder: {}", e)))?;
                let mut archive = tar::Archive::new(decoder);
                archive
                    .unpack(server_path)
                    .map_err(|e| CraftError::Other(format!("Failed to unpack backup archive: {}", e)))?;
            } else {
                let gz = flate2::read::GzDecoder::new(file);
                let mut archive = tar::Archive::new(gz);
                archive
                    .unpack(server_path)
                    .map_err(|e| CraftError::Other(format!("Failed to unpack backup archive: {}", e)))?;
            }
        }

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

        let temp_dir = tempfile::tempdir().map_err(|e| {
            CraftError::Other(format!(
                "Failed to create temporary directory for restore: {}",
                e
            ))
        })?;
        let temp_file = temp_dir.path().join("downloaded_backup.tar.gz");

        provider.download_file(remote_key, &temp_file).await?;
        self.restore_backup(&temp_file, server_path)?;

        Ok(())
    }
}

fn should_exclude(rel_path: &Path) -> bool {
    let components: Vec<_> = rel_path
        .iter()
        .map(|c| c.to_string_lossy().to_string())
        .collect();
    for comp in &components {
        let lower = comp.to_lowercase();
        if lower == "logs" || lower == "crash-reports" || lower == "cache" || lower == ".craft" {
            return true;
        }
    }

    if let Some(file_name) = rel_path.file_name().and_then(|f| f.to_str()) {
        let lower = file_name.to_lowercase();
        if lower.ends_with(".tmp")
            || lower.ends_with(".sock")
            || lower.ends_with(".pid")
            || lower == "session.lock"
        {
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
    if comp.starts_with("world")
        || comp == "dim-1"
        || comp == "dim1"
        || comp == "worlds"
        || comp == "db"
    {
        return true;
    }

    // Config directories
    if comp == "config" || comp == "defaultconfigs" || comp == "plugins" {
        return true;
    }

    // Root config / script files
    if !is_dir
        && rel_path
            .parent()
            .map(|p| p.as_os_str().is_empty())
            .unwrap_or(true)
    {
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

fn compress_server_directory(
    source_dir: &Path,
    output_archive: &Path,
    world_only: bool,
    format: BackupFormat,
) -> Result<()> {
    let file = File::create(output_archive)?;
    match format {
        BackupFormat::TarZstd => {
            let enc = zstd::stream::write::Encoder::new(file, 3)
                .map_err(|e| CraftError::Other(format!("Failed to initialize zstd encoder: {}", e)))?;
            let mut tar = tar::Builder::new(enc);
            append_dir_to_tar(&mut tar, source_dir, world_only)?;
            let enc = tar
                .into_inner()
                .map_err(|e| CraftError::Other(format!("Failed to finalize tar archive: {}", e)))?;
            enc.finish()
                .map_err(|e| CraftError::Other(format!("Failed to finalize zstd stream: {}", e)))?;
        }
        BackupFormat::TarGz => {
            let enc = GzEncoder::new(file, Compression::default());
            let mut tar = tar::Builder::new(enc);
            append_dir_to_tar(&mut tar, source_dir, world_only)?;
            let enc = tar
                .into_inner()
                .map_err(|e| CraftError::Other(format!("Failed to finalize tar archive: {}", e)))?;
            enc.finish()
                .map_err(|e| CraftError::Other(format!("Failed to finalize gzip stream: {}", e)))?;
        }
    }
    Ok(())
}

fn append_dir_to_tar<W: std::io::Write>(
    tar: &mut tar::Builder<W>,
    source_dir: &Path,
    world_only: bool,
) -> Result<()> {
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
                    tar.append_dir(rel_path, &path).map_err(|e| {
                        CraftError::Other(format!(
                            "Failed to append dir {}: {}",
                            rel_path.display(),
                            e
                        ))
                    })?;
                    stack.push(path);
                } else {
                    let mut f = File::open(&path)?;
                    tar.append_file(rel_path, &mut f).map_err(|e| {
                        CraftError::Other(format!(
                            "Failed to append file {}: {}",
                            rel_path.display(),
                            e
                        ))
                    })?;
                }
            }
        }
    }

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
        compress_server_directory(&server_dir, &full_archive, false, BackupFormat::TarGz).unwrap();

        let restore_dir = tmp.path().join("restore_full");
        let engine = BackupEngine {
            backups_dir: tmp.path().join("backups"),
        };
        engine.restore_backup(&full_archive, &restore_dir).unwrap();

        assert!(restore_dir.join("world/level.dat").exists());
        assert!(restore_dir.join("server.properties").exists());
        assert!(restore_dir.join("server.jar").exists());
        assert!(!restore_dir.join("logs/latest.log").exists());
        assert!(!restore_dir.join("cache/some.cache").exists());
        assert!(!restore_dir.join("session.lock").exists());

        // 2. World-only backup (should only include world and config files, not server.jar)
        let world_archive = tmp.path().join("world.tar.gz");
        compress_server_directory(&server_dir, &world_archive, true, BackupFormat::TarGz).unwrap();

        let restore_world = tmp.path().join("restore_world");
        engine.restore_backup(&world_archive, &restore_world).unwrap();

        assert!(restore_world.join("world/level.dat").exists());
        assert!(restore_world.join("server.properties").exists());
        assert!(!restore_world.join("server.jar").exists());
        assert!(!restore_world.join("logs/latest.log").exists());
    }

    #[test]
    fn test_zstd_compression_and_auto_detection() {
        let tmp = tempdir().unwrap();
        let server_dir = tmp.path().join("server");
        fs::create_dir_all(server_dir.join("world")).unwrap();
        fs::write(server_dir.join("world/level.dat"), b"zstd_level_data").unwrap();
        fs::write(server_dir.join("server.properties"), b"motd=ZstdServer").unwrap();

        let backups_dir = tmp.path().join("backups");
        let engine = BackupEngine {
            backups_dir: backups_dir.clone(),
        };

        // Create zstd backup
        let zstd_archive = backups_dir.join("test_server").join("test_server_20260101_120000.tar.zst");
        fs::create_dir_all(zstd_archive.parent().unwrap()).unwrap();
        compress_server_directory(&server_dir, &zstd_archive, false, BackupFormat::TarZstd).unwrap();

        // Check list_backups
        let list = engine.list_backups("test_server");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].format, BackupFormat::TarZstd);
        assert!(list[0].size_bytes > 0);

        // Restore zstd backup
        let restore_dir = tmp.path().join("restore_zstd");
        engine.restore_backup(&zstd_archive, &restore_dir).unwrap();

        assert!(restore_dir.join("world/level.dat").exists());
        assert_eq!(
            fs::read_to_string(restore_dir.join("server.properties")).unwrap(),
            "motd=ZstdServer"
        );
    }

    #[test]
    fn test_restore_fails_when_server_is_running() {
        let tmp = tempdir().unwrap();
        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();
        fs::write(server_dir.join("world.txt"), b"original").unwrap();

        // Create a backup archive
        let archive_path = tmp.path().join("backup.tar.gz");
        compress_server_directory(&server_dir, &archive_path, false, BackupFormat::TarGz).unwrap();

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
