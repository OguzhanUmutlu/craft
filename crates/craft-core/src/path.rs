use std::path::{Path, PathBuf};
use std::env;
use std::fs;
use crate::error::{CraftError, Result};

#[derive(Debug, Clone)]
pub struct CraftPaths {
    pub home: PathBuf,
    pub servers_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub servers_file: PathBuf,
    pub remotes_file: PathBuf,
    pub config_file: PathBuf,
    pub socket_file: PathBuf,
    pub pid_file: PathBuf,
}

impl CraftPaths {
    pub fn new() -> Result<Self> {
        let home = if let Ok(val) = env::var("CRAFT_HOME").or_else(|_| env::var("CraftHomePath")) {
            PathBuf::from(val)
        } else {
            let user_home = directories::UserDirs::new()
                .ok_or_else(|| CraftError::Config("Unable to locate user home directory".to_string()))?
                .home_dir()
                .to_path_buf();

            #[cfg(target_os = "windows")]
            {
                user_home.join("craft")
            }
            #[cfg(target_os = "macos")]
            {
                user_home.join("Library").join("craft")
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            {
                user_home.join(".craft")
            }
        };

        if !home.exists() {
            fs::create_dir_all(&home)?;
        }

        let servers_dir = home.join("servers");
        if !servers_dir.exists() {
            fs::create_dir_all(&servers_dir)?;
        }

        let cache_dir = home.join("download_cache");
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        let backups_dir = home.join("backups");
        if !backups_dir.exists() {
            fs::create_dir_all(&backups_dir)?;
        }

        let servers_file = home.join("servers.toml");
        let remotes_file = home.join("remotes.toml");
        let config_file = home.join("config.toml");
        let socket_file = home.join("service.sock");
        let pid_file = home.join("service.pid");

        Ok(Self {
            home,
            servers_dir,
            cache_dir,
            backups_dir,
            servers_file,
            remotes_file,
            config_file,
            socket_file,
            pid_file,
        })
    }

    /// Resolves a server path either from a provided path, a name, or partial name match
    pub fn resolve_server_path(&self, explicit_path: Option<&Path>, name: Option<&str>, allow_partial: bool) -> Result<PathBuf> {
        if let Some(p) = explicit_path {
            return Ok(p.to_path_buf());
        }

        let name = name.ok_or_else(|| {
            CraftError::Config("Please specify a server name with --name, --path or as an argument.".to_string())
        })?;

        // 1. Check if the name matches any registered server in servers.toml
        if let Ok(registry) = crate::config::ServersRegistry::load(self) {
            if let Some(server) = registry.find_by_name(name) {
                return Ok(server.path.clone());
            }
            // Also check if any registered server's path ends with the name
            for server in &registry.servers {
                if let Some(fname) = server.path.file_name() {
                    if fname.to_string_lossy().eq_ignore_ascii_case(name) {
                        return Ok(server.path.clone());
                    }
                }
            }
        }

        let candidate = self.servers_dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }

        if allow_partial {
            let lower_name = name.to_lowercase();
            let mut matches = Vec::new();

            if let Ok(entries) = fs::read_dir(&self.servers_dir) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.to_lowercase().starts_with(&lower_name) {
                        matches.push(entry.path());
                    }
                }
            }

            if matches.len() == 1 {
                return Ok(matches.remove(0));
            } else if matches.len() > 1 {
                return Err(CraftError::Config(format!(
                    "Ambiguous server name '{}'. Matches: {}",
                    name,
                    matches
                        .iter()
                        .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }

        Ok(candidate)
    }

    /// Legacy servers.json path for backward compatibility
    pub fn legacy_servers_json(&self) -> PathBuf {
        self.home.join("servers.json")
    }
}
