use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use fs2::FileExt;
use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteAuthType {
    Key,
    Agent,
    Password,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteOsType {
    Linux,
    MacOS,
    Windows,
}

impl std::fmt::Display for RemoteOsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteOsType::Linux => write!(f, "Linux"),
            RemoteOsType::MacOS => write!(f, "macOS"),
            RemoteOsType::Windows => write!(f, "Windows"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteHostConfig {
    pub alias: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    #[serde(default = "default_auth_type")]
    pub auth_type: RemoteAuthType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_type: Option<RemoteOsType>,
}

fn default_ssh_port() -> u16 {
    22
}
fn default_auth_type() -> RemoteAuthType {
    RemoteAuthType::Key
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemotesRegistry {
    #[serde(default)]
    pub remotes: Vec<RemoteHostConfig>,
}

impl RemotesRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.remotes_file.exists() {
            let content = fs::read_to_string(&paths.remotes_file)?;
            let registry: RemotesRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse remotes.toml: {}", e)))?;
            return Ok(registry);
        }
        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize remotes.toml: {}", e)))?;

        let lock_file_path = paths.locks_dir.join("remotes.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;

        let temp_path = paths.remotes_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.remotes_file)?;

        lock_file.unlock()?;
        Ok(())
    }

    pub fn find(&self, alias: &str) -> Option<&RemoteHostConfig> {
        self.remotes.iter().find(|r| r.alias.eq_ignore_ascii_case(alias))
    }

    pub fn add(&mut self, remote: RemoteHostConfig) -> Result<()> {
        if self.find(&remote.alias).is_some() {
            return Err(CraftError::Other(format!("Remote host with alias '{}' already exists", remote.alias)));
        }
        self.remotes.push(remote);
        Ok(())
    }

    pub fn remove(&mut self, alias: &str) -> Option<RemoteHostConfig> {
        if let Some(pos) = self.remotes.iter().position(|r| r.alias.eq_ignore_ascii_case(alias)) {
            Some(self.remotes.remove(pos))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remotes_registry_add_remove_find() {
        let mut registry = RemotesRegistry::default();
        let remote = RemoteHostConfig {
            alias: "vps-prod".to_string(),
            host: "192.168.1.50".to_string(),
            port: 2222,
            user: "root".to_string(),
            auth_type: RemoteAuthType::Key,
            key_path: Some(PathBuf::from("~/.ssh/id_ed25519")),
            password: None,
            remote_dir: Some(PathBuf::from("/opt/craft")),
            os_type: Some(RemoteOsType::Linux),
        };

        assert!(registry.add(remote.clone()).is_ok());
        // Duplicate alias should fail
        assert!(registry.add(remote.clone()).is_err());

        // Find (case-insensitive)
        assert!(registry.find("vps-prod").is_some());
        assert!(registry.find("VPS-PROD").is_some());
        assert_eq!(registry.find("vps-prod").unwrap().port, 2222);
        assert_eq!(registry.find("vps-prod").unwrap().user, "root");

        // Remove
        let removed = registry.remove("VPS-PROD");
        assert!(removed.is_some());
        assert!(registry.find("vps-prod").is_none());
    }

    #[test]
    fn test_remotes_registry_toml_roundtrip() {
        let mut registry = RemotesRegistry::default();
        registry.remotes.push(RemoteHostConfig {
            alias: "macos-mini".to_string(),
            host: "10.0.0.15".to_string(),
            port: 22,
            user: "admin".to_string(),
            auth_type: RemoteAuthType::Password,
            key_path: None,
            password: Some("secret123".to_string()),
            remote_dir: Some(PathBuf::from("/Users/admin/minecraft")),
            os_type: Some(RemoteOsType::MacOS),
        });

        let serialized = toml::to_string(&registry).expect("Failed to serialize remotes.toml");
        let deserialized: RemotesRegistry = toml::from_str(&serialized).expect("Failed to deserialize remotes.toml");

        assert_eq!(registry.remotes.len(), deserialized.remotes.len());
        assert_eq!(registry.remotes[0].alias, deserialized.remotes[0].alias);
        assert_eq!(registry.remotes[0].os_type, Some(RemoteOsType::MacOS));
        assert_eq!(registry.remotes[0].auth_type, RemoteAuthType::Password);
    }
}

