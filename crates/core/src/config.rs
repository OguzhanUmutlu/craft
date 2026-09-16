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
    #[serde(default = "default_game_id")]
    pub game: String,
    #[serde(default)]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rcon_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jdwp_debug_port: Option<u16>,
}

pub fn default_game_id() -> String {
    "minecraft".to_string()
}

impl ServerConfig {
    pub fn game_definition(&self) -> crate::game::GameDefinition {
        crate::game::find_game(&self.game).unwrap_or_else(|| {
            crate::game::GameDefinition::custom(
                &self.game,
                &self.game,
                self.port.unwrap_or(25565),
                crate::game::TransportProtocol::Both,
            )
        })
    }
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
    #[serde(default = "default_cache_max_bytes")]
    pub cache_max_bytes: u64,
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
fn default_cache_max_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            daemon_port: default_daemon_port(),
            max_log_lines: default_max_log_lines(),
            auto_agree_eula: default_auto_agree_eula(),
            download_concurrency: default_download_concurrency(),
            cache_max_bytes: default_cache_max_bytes(),
        }
    }
}

impl GlobalSettings {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.config_file.exists() {
            let content = fs::read_to_string(&paths.config_file)?;
            let settings: GlobalSettings = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse config.toml: {}", e)))?;
            return Ok(settings);
        }
        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize config.toml: {}", e)))?;
        let temp_path = paths.config_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.config_file)?;
        Ok(())
    }
}

impl ServersRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.servers_file.exists() {
            let content = fs::read_to_string(&paths.servers_file)?;
            let registry: ServersRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse servers.toml: {}", e)))?;
            return Ok(registry);
        }

        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize servers.toml: {}", e)))?;

        let lock_file_path = paths.locks_dir.join("servers.lock");
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

        lock_file.unlock()?;
        Ok(())
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

/// Reads `level-name` from server.properties in `server_path`, defaulting to `"world"`.
pub fn get_default_world(server_path: &Path) -> String {
    let props_path = server_path.join("server.properties");
    if let Ok(content) = fs::read_to_string(&props_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("level-name=") {
                let v = val.trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    "world".to_string()
}

/// Sets `level-name` in server.properties in `server_path`.
pub fn set_default_world(server_path: &Path, world_name: &str) -> Result<()> {
    let props_path = server_path.join("server.properties");
    let content = if props_path.exists() {
        fs::read_to_string(&props_path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut new_lines = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("level-name=") {
            new_lines.push(format!("level-name={}", world_name));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }
    if !found {
        new_lines.push(format!("level-name={}", world_name));
    }
    fs::write(&props_path, new_lines.join("\n") + "\n")?;
    Ok(())
}

/// Discovers the (Overworld, Nether, The End) active world directory names.
pub fn get_dimension_worlds(server_path: &Path) -> (String, Option<String>, Option<String>) {
    let overworld = get_default_world(server_path);
    let mut nether = None;
    let mut end = None;

    let props_path = server_path.join("server.properties");
    if let Ok(content) = fs::read_to_string(&props_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("craft-nether-world=") {
                let v = val.trim();
                if !v.is_empty() {
                    nether = Some(v.to_string());
                }
            } else if let Some(val) = trimmed.strip_prefix("craft-end-world=") {
                let v = val.trim();
                if !v.is_empty() {
                    end = Some(v.to_string());
                }
            }
        }
    }

    if nether.is_none() {
        let candidate = format!("{}_nether", overworld);
        if server_path.join(&candidate).exists() {
            nether = Some(candidate);
        }
    }

    if end.is_none() {
        let candidate = format!("{}_the_end", overworld);
        if server_path.join(&candidate).exists() {
            end = Some(candidate);
        }
    }

    (overworld, nether, end)
}

/// Sets the active Nether world role in server.properties and Bukkit/Paper setup.
pub fn set_nether_world(server_path: &Path, world_name: &str) -> Result<()> {
    let props_path = server_path.join("server.properties");
    let content = if props_path.exists() {
        fs::read_to_string(&props_path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut new_lines = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("craft-nether-world=") {
            new_lines.push(format!("craft-nether-world={}", world_name));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }
    if !found {
        new_lines.push(format!("craft-nether-world={}", world_name));
    }
    fs::write(&props_path, new_lines.join("\n") + "\n")?;
    Ok(())
}

/// Sets the active The End world role in server.properties and Bukkit/Paper setup.
pub fn set_end_world(server_path: &Path, world_name: &str) -> Result<()> {
    let props_path = server_path.join("server.properties");
    let content = if props_path.exists() {
        fs::read_to_string(&props_path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut new_lines = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("craft-end-world=") {
            new_lines.push(format!("craft-end-world={}", world_name));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }
    if !found {
        new_lines.push(format!("craft-end-world={}", world_name));
    }
    fs::write(&props_path, new_lines.join("\n") + "\n")?;
    Ok(())
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
            game: "minecraft".to_string(),
            auto: false,
            port: Some(25565),
            query_port: Some(25565),
            rcon_port: None,
            memory: Some("4G".to_string()),
            java_path: None,
            binary_path: None,
            start_args: None,
            jvm_args: None,
            created_at: Some(Utc::now()),
            backup_method: None,
            jdwp_debug_port: None,
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
            game: "minecraft".to_string(),
            auto: true,
            port: Some(25565),
            query_port: Some(25565),
            rcon_port: Some(25575),
            memory: Some("8G".to_string()),
            java_path: Some(PathBuf::from("/usr/bin/java")),
            binary_path: None,
            start_args: None,
            jvm_args: Some(vec!["-XX:+UseG1GC".to_string()]),
            created_at: Some(Utc::now()),
            backup_method: None,
            jdwp_debug_port: None,
        });

        let serialized = toml::to_string(&registry).expect("Failed to serialize");
        let deserialized: ServersRegistry = toml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(registry.servers.len(), deserialized.servers.len());
        assert_eq!(registry.servers[0].name, deserialized.servers[0].name);
        assert_eq!(registry.servers[0].game, "minecraft");
        assert_eq!(registry.servers[0].auto, deserialized.servers[0].auto);
    }

    #[test]
    fn test_legacy_servers_toml_backward_compatibility() {
        let legacy_toml = r#"
[[servers]]
name = "legacy-mc"
path = "/servers/legacy-mc"
software = "paper"
version = "1.20.4"
auto = false
port = 25565
"#;
        let reg: ServersRegistry = toml::from_str(legacy_toml).expect("Should parse legacy TOML");
        assert_eq!(reg.servers.len(), 1);
        assert_eq!(reg.servers[0].name, "legacy-mc");
        assert_eq!(reg.servers[0].game, "minecraft");
        assert_eq!(reg.servers[0].game_definition().name, "Minecraft");
    }

    #[test]
    fn test_default_world_helpers() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        // Default when no server.properties exists
        assert_eq!(get_default_world(path), "world");

        // Set default world
        assert!(set_default_world(path, "adventure_map").is_ok());
        assert_eq!(get_default_world(path), "adventure_map");

        // Update default world
        assert!(set_default_world(path, "skyblock").is_ok());
        assert_eq!(get_default_world(path), "skyblock");
    }
}
