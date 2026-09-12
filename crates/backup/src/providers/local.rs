use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;
use craft_core::{CraftError, Result};
use super::{CloudBackupEntry, StorageProvider};

pub struct LocalStorageProvider {
    base_dir: PathBuf,
}

impl LocalStorageProvider {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl StorageProvider for LocalStorageProvider {
    fn provider_name(&self) -> &'static str {
        "local"
    }

    async fn upload_file(&self, local_path: &Path, remote_key: &str) -> Result<()> {
        let dest = self.base_dir.join(remote_key);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(local_path, &dest)?;
        Ok(())
    }

    async fn list_files(&self, prefix: &str) -> Result<Vec<CloudBackupEntry>> {
        let dir = self.base_dir.join(prefix);
        let mut list = Vec::new();
        if dir.exists() && dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let filename = entry.file_name().to_string_lossy().to_string();
                        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let last_modified = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .map(|c| {
                                let dt: chrono::DateTime<Utc> = c.into();
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            })
                            .unwrap_or_else(|_| "Unknown".to_string());

                        let key = if prefix.is_empty() {
                            filename.clone()
                        } else {
                            format!("{}/{}", prefix.trim_end_matches('/'), filename)
                        };

                        list.push(CloudBackupEntry {
                            key,
                            filename,
                            size_bytes,
                            last_modified,
                            provider: "local".to_string(),
                        });
                    }
                }
            }
        }
        list.sort_by(|a, b| b.filename.cmp(&a.filename));
        Ok(list)
    }

    async fn download_file(&self, remote_key: &str, target_path: &Path) -> Result<()> {
        let src = self.base_dir.join(remote_key);
        if !src.exists() {
            return Err(CraftError::Other(format!("Local file not found: {}", src.display())));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, target_path)?;
        Ok(())
    }

    async fn delete_file(&self, remote_key: &str) -> Result<()> {
        let path = self.base_dir.join(remote_key);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }
}
