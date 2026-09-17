use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

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
pub struct LocalBackupTarget {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3BackupTarget {
    pub id: String,
    pub name: String,
    pub bucket: String,
    #[serde(default = "default_s3_region")]
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

impl From<S3BackupTarget> for S3BackupConfig {
    fn from(t: S3BackupTarget) -> Self {
        Self {
            bucket: t.bucket,
            region: t.region,
            endpoint: t.endpoint,
            access_key_id: t.access_key_id,
            secret_access_key: t.secret_access_key,
            prefix: t.prefix,
        }
    }
}

impl From<&S3BackupTarget> for S3BackupConfig {
    fn from(t: &S3BackupTarget) -> Self {
        Self {
            bucket: t.bucket.clone(),
            region: t.region.clone(),
            endpoint: t.endpoint.clone(),
            access_key_id: t.access_key_id.clone(),
            secret_access_key: t.secret_access_key.clone(),
            prefix: t.prefix.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GDriveBackupTarget {
    pub id: String,
    pub name: String,
    pub folder_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_account_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

impl From<GDriveBackupTarget> for GDriveBackupConfig {
    fn from(t: GDriveBackupTarget) -> Self {
        Self {
            folder_id: t.folder_id,
            service_account_path: t.service_account_path,
            api_token: t.api_token,
        }
    }
}

impl From<&GDriveBackupTarget> for GDriveBackupConfig {
    fn from(t: &GDriveBackupTarget) -> Self {
        Self {
            folder_id: t.folder_id.clone(),
            service_account_path: t.service_account_path.clone(),
            api_token: t.api_token.clone(),
        }
    }
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
    pub backup_method: Option<String>,
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
            backup_method: None,
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
    #[serde(default)]
    pub local_targets: Vec<LocalBackupTarget>,
    #[serde(default)]
    pub s3_targets: Vec<S3BackupTarget>,
    #[serde(default)]
    pub gdrive_targets: Vec<GDriveBackupTarget>,
    #[serde(default)]
    pub server_policies: std::collections::HashMap<String, AutoBackupPolicy>,

    // Legacy fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3BackupConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gdrive: Option<GDriveBackupConfig>,
}

impl GlobalBackupRegistry {
    pub fn config_path(paths: &CraftPaths) -> PathBuf {
        paths.home.join("backup_config.toml")
    }

    pub fn ensure_defaults(&mut self, paths: &CraftPaths) {
        if self.local_targets.is_empty() {
            let p = self
                .local_path
                .clone()
                .unwrap_or_else(|| paths.backups_dir.clone());
            self.local_targets.push(LocalBackupTarget {
                id: "default".to_string(),
                name: "Default Storage".to_string(),
                path: p,
            });
        }
        if self.s3_targets.is_empty() {
            if let Some(ref s3) = self.s3 {
                self.s3_targets.push(S3BackupTarget {
                    id: "default-s3".to_string(),
                    name: s3.bucket.clone(),
                    bucket: s3.bucket.clone(),
                    region: s3.region.clone(),
                    endpoint: s3.endpoint.clone(),
                    access_key_id: s3.access_key_id.clone(),
                    secret_access_key: s3.secret_access_key.clone(),
                    prefix: s3.prefix.clone(),
                });
            }
        }
        if self.gdrive_targets.is_empty() {
            if let Some(ref gd) = self.gdrive {
                self.gdrive_targets.push(GDriveBackupTarget {
                    id: "default-gdrive".to_string(),
                    name: "Default Google Drive".to_string(),
                    folder_id: gd.folder_id.clone(),
                    service_account_path: gd.service_account_path.clone(),
                    api_token: gd.api_token.clone(),
                });
            }
        }
    }

    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let path = Self::config_path(paths);
        let exists = path.exists();
        let mut reg = if exists {
            let content = fs::read_to_string(&path)?;
            toml::from_str(&content).map_err(|e| {
                CraftError::Config(format!("Failed to parse backup_config.toml: {}", e))
            })?
        } else {
            Self::default()
        };
        if !exists {
            reg.ensure_defaults(paths);
        } else {
            // Only migrate legacy single-target fields if new arrays are empty and legacy fields exist
            if reg.local_targets.is_empty() && reg.local_path.is_some() {
                let p = reg
                    .local_path
                    .clone()
                    .unwrap_or_else(|| paths.backups_dir.clone());
                reg.local_targets.push(LocalBackupTarget {
                    id: "default".to_string(),
                    name: "Default Storage".to_string(),
                    path: p,
                });
            }
            if reg.s3_targets.is_empty() && reg.s3.is_some() {
                if let Some(ref s3) = reg.s3 {
                    reg.s3_targets.push(S3BackupTarget {
                        id: "default-s3".to_string(),
                        name: s3.bucket.clone(),
                        bucket: s3.bucket.clone(),
                        region: s3.region.clone(),
                        endpoint: s3.endpoint.clone(),
                        access_key_id: s3.access_key_id.clone(),
                        secret_access_key: s3.secret_access_key.clone(),
                        prefix: s3.prefix.clone(),
                    });
                }
            }
            if reg.gdrive_targets.is_empty() && reg.gdrive.is_some() {
                if let Some(ref gd) = reg.gdrive {
                    reg.gdrive_targets.push(GDriveBackupTarget {
                        id: "default-gdrive".to_string(),
                        name: "Default Google Drive".to_string(),
                        folder_id: gd.folder_id.clone(),
                        service_account_path: gd.service_account_path.clone(),
                        api_token: gd.api_token.clone(),
                    });
                }
            }
        }
        Ok(reg)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let path = Self::config_path(paths);
        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize backup_config.toml: {}", e))
        })?;

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

    pub fn default_local_path(&self, paths: &CraftPaths) -> PathBuf {
        self.local_targets
            .first()
            .map(|t| t.path.clone())
            .or_else(|| self.local_path.clone())
            .unwrap_or_else(|| paths.backups_dir.clone())
    }

    pub fn find_local(&self, id: &str) -> Option<&LocalBackupTarget> {
        self.local_targets
            .iter()
            .find(|t| t.id == id || t.name == id)
            .or_else(|| self.local_targets.first())
    }

    pub fn find_s3(&self, id: &str) -> Option<&S3BackupTarget> {
        self.s3_targets
            .iter()
            .find(|t| t.id == id || t.name == id)
            .or_else(|| self.s3_targets.first())
    }

    pub fn find_gdrive(&self, id: &str) -> Option<&GDriveBackupTarget> {
        self.gdrive_targets
            .iter()
            .find(|t| t.id == id || t.name == id)
            .or_else(|| self.gdrive_targets.first())
    }

    pub fn format_method_display(&self, method: Option<&str>) -> String {
        match method {
            None => {
                if let Some(first) = self.local_targets.first() {
                    format!("Local: {}", first.name)
                } else {
                    "Local: Default Storage".to_string()
                }
            }
            Some("multi") => "Multi-Destination".to_string(),
            Some("local") => {
                if let Some(first) = self.local_targets.first() {
                    format!("Local: {}", first.name)
                } else {
                    "Local: Default Storage".to_string()
                }
            }
            Some(m) if m.starts_with("local:") => {
                let id = &m["local:".len()..];
                if let Some(t) = self.local_targets.iter().find(|t| t.id == id) {
                    format!("Local: {}", t.name)
                } else {
                    format!("Local: {}", id)
                }
            }
            Some(m) if m.starts_with("s3:") || m == "s3" => {
                let id = if m == "s3" {
                    "default-s3"
                } else {
                    &m["s3:".len()..]
                };
                if let Some(t) = self.s3_targets.iter().find(|t| t.id == id) {
                    format!("S3: {}", t.name)
                } else if let Some(first) = self.s3_targets.first() {
                    format!("S3: {}", first.name)
                } else {
                    "S3: [Configure First]".to_string()
                }
            }
            Some(m) if m.starts_with("gdrive:") || m == "gdrive" => {
                let id = if m == "gdrive" {
                    "default-gdrive"
                } else {
                    &m["gdrive:".len()..]
                };
                if let Some(t) = self.gdrive_targets.iter().find(|t| t.id == id) {
                    format!("Google Drive: {}", t.name)
                } else if let Some(first) = self.gdrive_targets.first() {
                    format!("Google Drive: {}", first.name)
                } else {
                    "Google Drive: [Configure First]".to_string()
                }
            }
            Some(other) => other.to_string(),
        }
    }

    pub fn get_policy(&self, server_name: &str) -> AutoBackupPolicy {
        self.server_policies
            .get(server_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_policy(&mut self, server_name: &str, policy: AutoBackupPolicy) {
        self.server_policies.insert(server_name.to_string(), policy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_defaults_and_formatting() {
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut reg = GlobalBackupRegistry::default();
        assert!(reg.local_targets.is_empty());
        reg.ensure_defaults(&paths);

        assert_eq!(reg.local_targets.len(), 1);
        assert_eq!(reg.local_targets[0].id, "default");
        assert_eq!(reg.local_targets[0].name, "Default Storage");
        assert_eq!(reg.local_targets[0].path, paths.backups_dir);

        // Test display formatting
        assert_eq!(reg.format_method_display(None), "Local: Default Storage");
        assert_eq!(
            reg.format_method_display(Some("local")),
            "Local: Default Storage"
        );
        assert_eq!(
            reg.format_method_display(Some("local:default")),
            "Local: Default Storage"
        );
        assert_eq!(
            reg.format_method_display(Some("multi")),
            "Multi-Destination"
        );

        // Add S3 target
        reg.s3_targets.push(S3BackupTarget {
            id: "s3-prod".to_string(),
            name: "AWS Production Bucket".to_string(),
            endpoint: None,
            bucket: "mc-backups".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            prefix: Some("server1/".to_string()),
        });

        assert_eq!(
            reg.format_method_display(Some("s3:s3-prod")),
            "S3: AWS Production Bucket"
        );
        assert_eq!(
            reg.format_method_display(Some("s3")),
            "S3: AWS Production Bucket"
        );
        assert_eq!(reg.find_s3("s3-prod").unwrap().bucket, "mc-backups");

        // Test TOML roundtrip
        let toml_str = toml::to_string_pretty(&reg).unwrap();
        let loaded: GlobalBackupRegistry = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.s3_targets.len(), 1);
        assert_eq!(loaded.local_targets.len(), 1);
    }

    #[test]
    fn test_legacy_migration() {
        let legacy_toml = r#"
            local_path = "/tmp/my-backups"

            [s3]
            bucket = "legacy-bucket"
            region = "eu-west-1"
            access_key_id = "abc"
            secret_access_key = "xyz"

            [gdrive]
            folder_id = "folder-123"
            credentials_json = "{}"
        "#;

        let mut reg: GlobalBackupRegistry = toml::from_str(legacy_toml).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        reg.ensure_defaults(&paths);

        assert_eq!(reg.local_targets.len(), 1);
        assert_eq!(reg.local_targets[0].path, PathBuf::from("/tmp/my-backups"));
        assert_eq!(reg.s3_targets.len(), 1);
        assert_eq!(reg.s3_targets[0].bucket, "legacy-bucket");
        assert_eq!(reg.gdrive_targets.len(), 1);
        assert_eq!(reg.gdrive_targets[0].folder_id, "folder-123");
    }
}
