use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshTargetKind {
    S3,
    CloudflareR2,
    GDrive,
    Sftp,
    Local,
}

impl std::fmt::Display for MeshTargetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeshTargetKind::S3 => write!(f, "AWS S3"),
            MeshTargetKind::CloudflareR2 => write!(f, "Cloudflare R2"),
            MeshTargetKind::GDrive => write!(f, "Google Drive"),
            MeshTargetKind::Sftp => write!(f, "SFTP Node"),
            MeshTargetKind::Local => write!(f, "Local Filesystem"),
        }
    }
}

impl std::str::FromStr for MeshTargetKind {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "s3" | "aws" | "aws_s3" => Ok(MeshTargetKind::S3),
            "r2" | "cloudflare" | "cloudflare_r2" => Ok(MeshTargetKind::CloudflareR2),
            "gdrive" | "google" | "gcs" => Ok(MeshTargetKind::GDrive),
            "sftp" | "ssh" => Ok(MeshTargetKind::Sftp),
            "local" | "fs" => Ok(MeshTargetKind::Local),
            other => Err(CraftError::Other(format!(
                "Unknown mesh target kind '{}'. Supported: s3, r2, gdrive, sftp, local",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationQuorum {
    All,
    Majority,
    Any,
}

impl Default for ReplicationQuorum {
    fn default() -> Self {
        ReplicationQuorum::Majority
    }
}

impl std::str::FromStr for ReplicationQuorum {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "all" => Ok(ReplicationQuorum::All),
            "majority" | "quorum" => Ok(ReplicationQuorum::Majority),
            "any" | "one" => Ok(ReplicationQuorum::Any),
            other => Err(CraftError::Other(format!(
                "Invalid replication quorum '{}'. Supported: all, majority, any",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshTarget {
    pub id: String,
    pub name: String,
    pub kind: MeshTargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub bucket_or_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_priority")]
    pub priority: u32,
}

fn default_true() -> bool {
    true
}

fn default_priority() -> u32 {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshPolicy {
    #[serde(default)]
    pub quorum: ReplicationQuorum,
    #[serde(default = "default_true")]
    pub encryption_enabled: bool,
    #[serde(default = "default_concurrency")]
    pub max_concurrent_uploads: usize,
    #[serde(default = "default_compression")]
    pub compression_level: i32,
}

fn default_concurrency() -> usize {
    8
}

fn default_compression() -> i32 {
    3
}

impl Default for MeshPolicy {
    fn default() -> Self {
        Self {
            quorum: ReplicationQuorum::Majority,
            encryption_enabled: true,
            max_concurrent_uploads: 8,
            compression_level: 3,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshRegistry {
    #[serde(default)]
    pub policy: MeshPolicy,
    #[serde(default)]
    pub targets: Vec<MeshTarget>,
}

impl MeshRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.mesh_file.exists() {
            let content = fs::read_to_string(&paths.mesh_file)?;
            let reg: MeshRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse mesh.toml: {}", e)))?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    pub fn load_or_init(paths: &CraftPaths) -> Result<Self> {
        let mut reg = Self::load(paths)?;
        if reg.targets.is_empty() {
            // Default local backup target
            reg.targets.push(MeshTarget {
                id: "local-primary".to_string(),
                name: "Local Storage".to_string(),
                kind: MeshTargetKind::Local,
                endpoint: None,
                bucket_or_path: paths.backups_dir.to_string_lossy().to_string(),
                region: None,
                access_key: None,
                secret_key: None,
                enabled: true,
                priority: 1,
            });
            reg.save(paths)?;
        }
        Ok(reg)
    }

    fn save_internal(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize mesh.toml: {}", e)))?;
        let temp_path = paths.mesh_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(temp_path, &paths.mesh_file)?;
        Ok(())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_file_path = paths.locks_dir.join("mesh.lock");
        if let Some(parent) = lock_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;
        let res = self.save_internal(paths);
        let _ = lock_file.unlock();
        res
    }

    pub fn add_target(&mut self, target: MeshTarget) -> Result<()> {
        if self.get_target(&target.id).is_some() {
            return Err(CraftError::Other(format!(
                "Mesh target '{}' already exists",
                target.id
            )));
        }
        self.targets.push(target);
        Ok(())
    }

    pub fn remove_target(&mut self, id: &str) -> Result<()> {
        let initial_len = self.targets.len();
        self.targets.retain(|t| !t.id.eq_ignore_ascii_case(id));
        if self.targets.len() == initial_len {
            return Err(CraftError::Other(format!(
                "Mesh target '{}' not found",
                id
            )));
        }
        Ok(())
    }

    pub fn get_target(&self, id: &str) -> Option<&MeshTarget> {
        self.targets.iter().find(|t| t.id.eq_ignore_ascii_case(id))
    }

    pub fn active_targets(&self) -> Vec<&MeshTarget> {
        let mut list: Vec<&MeshTarget> = self.targets.iter().filter(|t| t.enabled).collect();
        list.sort_by_key(|t| t.priority);
        list
    }

    pub fn is_quorum_met(&self, successful_uploads: usize, total_active: usize) -> bool {
        if total_active == 0 {
            return false;
        }
        match self.policy.quorum {
            ReplicationQuorum::All => successful_uploads == total_active,
            ReplicationQuorum::Majority => successful_uploads >= (total_active / 2) + 1,
            ReplicationQuorum::Any => successful_uploads >= 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mesh_registry_quorum() {
        let mut reg = MeshRegistry::default();
        reg.policy.quorum = ReplicationQuorum::Majority;
        assert!(!reg.is_quorum_met(1, 3));
        assert!(reg.is_quorum_met(2, 3));
        assert!(reg.is_quorum_met(3, 3));

        reg.policy.quorum = ReplicationQuorum::All;
        assert!(!reg.is_quorum_met(2, 3));
        assert!(reg.is_quorum_met(3, 3));

        reg.policy.quorum = ReplicationQuorum::Any;
        assert!(reg.is_quorum_met(1, 3));
        assert!(!reg.is_quorum_met(0, 3));
    }

    #[test]
    fn test_mesh_registry_persistence() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut reg = MeshRegistry::load_or_init(&paths).unwrap();
        assert_eq!(reg.targets.len(), 1);

        reg.add_target(MeshTarget {
            id: "r2-backup".to_string(),
            name: "Cloudflare R2 Storage".to_string(),
            kind: MeshTargetKind::CloudflareR2,
            endpoint: Some("https://account.r2.cloudflarestorage.com".to_string()),
            bucket_or_path: "craft-backups".to_string(),
            region: Some("auto".to_string()),
            access_key: Some("key123".to_string()),
            secret_key: Some("secret123".to_string()),
            enabled: true,
            priority: 5,
        })
        .unwrap();
        reg.save(&paths).unwrap();

        let loaded = MeshRegistry::load(&paths).unwrap();
        assert_eq!(loaded.targets.len(), 2);
        assert_eq!(loaded.active_targets().len(), 2);
    }
}
