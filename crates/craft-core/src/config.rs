use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    pub name: String,
    pub path: PathBuf,
    pub software: String,
    pub version: String,
    #[serde(default)]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServersRegistry {
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    #[serde(default = "default_daemon_port")]
    pub daemon_port: u16,
    #[serde(default = "default_max_log_lines")]
    pub max_log_lines: usize,
    #[serde(default = "default_auto_agree_eula")]
    pub auto_agree_eula: bool,
    #[serde(default = "default_download_concurrency")]
    pub download_concurrency: usize,
}

fn default_daemon_port() -> u16 {
    8123
}
fn default_max_log_lines() -> usize {
    50000
}
fn default_auto_agree_eula() -> bool {
    false
}
fn default_download_concurrency() -> usize {
    4
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            daemon_port: default_daemon_port(),
            max_log_lines: default_max_log_lines(),
            auto_agree_eula: default_auto_agree_eula(),
            download_concurrency: default_download_concurrency(),
        }
    }
}

/// Legacy format from TypeScript craft for smooth backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyServerConfig {
    path: String,
    software: String,
    version: String,
    #[serde(default)]
    auto: bool,
}

impl ServersRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        // If modern servers.toml exists, read it
        if paths.servers_file.exists() {
            let content = fs::read_to_string(&paths.servers_file)?;
            let registry: ServersRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse servers.toml: {}", e)))?;
            return Ok(registry);
        }

        // Check for legacy servers.json
        let legacy_json = paths.legacy_servers_json();
        if legacy_json.exists() {
            let content = fs::read_to_string(&legacy_json)?;
            if let Ok(legacy_list) = serde_json::from_str::<Vec<LegacyServerConfig>>(&content) {
                let mut servers = Vec::new();
                for item in legacy_list {
                    let path = PathBuf::from(&item.path);
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| item.path.clone());

                    servers.push(ServerConfig {
                        name,
                        path,
                        software: item.software,
                        version: item.version,
                        auto: item.auto,
                        java_path: None,
                        memory: Some("2G".to_string()),
                        port: None,
                        jvm_args: None,
                        created_at: Some(Utc::now()),
                    });
                }

                let registry = ServersRegistry { servers };
                // Migrate to servers.toml automatically
                registry.save(paths)?;
                return Ok(registry);
            }
        }

        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize servers.toml: {}", e)))?;

        let lock_file_path = paths.home.join(".servers.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;

        let temp_path = paths.servers_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.servers_file)?;

        // Also write legacy JSON mirror for any external tools expecting servers.json
        if let Ok(legacy_json) = serde_json::to_string_pretty(&self.to_legacy()) {
            let _ = fs::write(paths.legacy_servers_json(), legacy_json);
        }

        lock_file.unlock()?;
        Ok(())
    }

    fn to_legacy(&self) -> Vec<LegacyServerConfig> {
        self.servers
            .iter()
            .map(|s| LegacyServerConfig {
                path: s.path.to_string_lossy().to_string(),
                software: s.software.clone(),
                version: s.version.clone(),
                auto: s.auto,
            })
            .collect()
    }

    pub fn find_by_path<P: AsRef<Path>>(&self, path: P) -> Option<&ServerConfig> {
        let target = path.as_ref();
        let canonical_target = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
        self.servers.iter().find(|s| {
            s.path == target || s.path.canonicalize().map(|p| p == canonical_target).unwrap_or(false)
        })
    }

    pub fn find_by_name(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn add(&mut self, server: ServerConfig) -> Result<()> {
        if self.find_by_path(&server.path).is_some() {
            return Err(CraftError::ServerAlreadyExists(server.path.to_string_lossy().to_string()));
        }
        self.servers.push(server);
        Ok(())
    }

    pub fn remove<P: AsRef<Path>>(&mut self, path: P) -> Option<ServerConfig> {
        let target = path.as_ref();
        if let Some(pos) = self.servers.iter().position(|s| s.path == target) {
            Some(self.servers.remove(pos))
        } else {
            None
        }
    }

    pub fn set_auto<P: AsRef<Path>>(&mut self, path: P, auto: bool) -> bool {
        let target = path.as_ref();
        if let Some(s) = self.servers.iter_mut().find(|s| s.path == target) {
            s.auto = auto;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_add_remove_find() {
        let mut registry = ServersRegistry::default();
        let server = ServerConfig {
            name: "test-server".to_string(),
            path: PathBuf::from("/test/path"),
            software: "paper".to_string(),
            version: "1.21.4".to_string(),
            auto: false,
            java_path: None,
            memory: Some("4G".to_string()),
            port: Some(25565),
            jvm_args: None,
            created_at: Some(Utc::now()),
        };

        assert!(registry.add(server.clone()).is_ok());
        // Duplicate path should fail
        assert!(registry.add(server.clone()).is_err());

        // Find
        assert!(registry.find_by_name("test-server").is_some());
        assert!(registry.find_by_name("TEST-SERVER").is_some()); // case-insensitive
        assert!(registry.find_by_path(Path::new("/test/path")).is_some());

        // Toggle auto
        assert!(registry.set_auto(Path::new("/test/path"), true));
        assert!(registry.find_by_name("test-server").unwrap().auto);

        // Remove
        let removed = registry.remove(Path::new("/test/path"));
        assert!(removed.is_some());
        assert!(registry.find_by_name("test-server").is_none());
    }

    #[test]
    fn test_toml_serialization_roundtrip() {
        let mut registry = ServersRegistry::default();
        registry.servers.push(ServerConfig {
            name: "survival".to_string(),
            path: PathBuf::from("/servers/survival"),
            software: "purpur".to_string(),
            version: "1.21".to_string(),
            auto: true,
            java_path: Some(PathBuf::from("/usr/bin/java")),
            memory: Some("8G".to_string()),
            port: Some(25565),
            jvm_args: Some(vec!["-XX:+UseG1GC".to_string()]),
            created_at: Some(Utc::now()),
        });

        let serialized = toml::to_string(&registry).expect("Failed to serialize");
        let deserialized: ServersRegistry = toml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(registry.servers.len(), deserialized.servers.len());
        assert_eq!(registry.servers[0].name, deserialized.servers[0].name);
        assert_eq!(registry.servers[0].auto, deserialized.servers[0].auto);
    }
}
