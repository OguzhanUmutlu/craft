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
        Self {
            backups_dir: paths.backups_dir.clone(),
        }
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
        let archive_name = format!("{}_{}.tar.gz", server_name, timestamp);
        let archive_path = target_dir.join(&archive_name);

        let res = compress_server_directory(server_path, &archive_path);

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

        let file = File::open(backup_file)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        fs::create_dir_all(server_path)?;
        archive.unpack(server_path)
            .map_err(|e| CraftError::Other(format!("Failed to unpack backup archive: {}", e)))?;

        Ok(())
    }
}

fn compress_server_directory(source_dir: &Path, output_tar_gz: &Path) -> Result<()> {
    let file = File::create(output_tar_gz)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(enc);

    tar.append_dir_all(".", source_dir)
        .map_err(|e| CraftError::Other(format!("Failed to create tar archive: {}", e)))?;

    tar.finish()
        .map_err(|e| CraftError::Other(format!("Failed to finalize archive: {}", e)))?;

    Ok(())
}
