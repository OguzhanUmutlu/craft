use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use fs2::FileExt;

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3BackupConfig {
    pub bucket: String,
    #[serde(default = "default_s3_region")]
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>, // Custom endpoint for Cloudflare R2, MinIO, Wasabi, Backblaze B2
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

fn default_s3_region() -> String {
    "us-east-1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GDriveBackupConfig {
    pub folder_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_account_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoBackupPolicy {
    pub enabled: bool,
    #[serde(default = "default_interval_hours")]
    pub interval_hours: u32,
    #[serde(default = "default_retention_count")]
    pub retention_count: usize,
    #[serde(default)]
    pub upload_to_s3: bool,
    #[serde(default)]
    pub upload_to_gdrive: bool,
    #[serde(default)]
    pub world_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_backup_timestamp: Option<i64>,
}

impl Default for AutoBackupPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_hours: 6,
            retention_count: 7,
            upload_to_s3: false,
            upload_to_gdrive: false,
            world_only: false,
            last_backup_timestamp: None,
        }
    }
}

fn default_interval_hours() -> u32 {
    6
}

fn default_retention_count() -> usize {
    7
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalBackupRegistry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3BackupConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gdrive: Option<GDriveBackupConfig>,
    #[serde(default)]
    pub server_policies: std::collections::HashMap<String, AutoBackupPolicy>,
}

impl GlobalBackupRegistry {
    pub fn config_path(paths: &CraftPaths) -> PathBuf {
        paths.home.join("backup_config.toml")
    }


    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let path = Self::config_path(paths);
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let reg: GlobalBackupRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse backup_config.toml: {}", e)))?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let path = Self::config_path(paths);
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize backup_config.toml: {}", e)))?;

        let lock_file_path = paths.locks_dir.join("backup_config.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &path)?;

        lock_file.unlock()?;
        Ok(())
    }

    pub fn get_policy(&self, server_name: &str) -> AutoBackupPolicy {
        self.server_policies.get(server_name).cloned().unwrap_or_default()
    }

    pub fn set_policy(&mut self, server_name: &str, policy: AutoBackupPolicy) {
        self.server_policies.insert(server_name.to_string(), policy);
    }
}
