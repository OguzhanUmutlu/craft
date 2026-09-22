use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};

fn default_idle_timeout() -> u64 {
    15
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerAutoscalePolicy {
    pub server_name: String,
    #[serde(default)]
    pub hibernation_enabled: bool,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_mins: u64,
    #[serde(default = "default_true")]
    pub wake_on_packet: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_motd: Option<String>,
}

impl ServerAutoscalePolicy {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            hibernation_enabled: false,
            idle_timeout_mins: 15,
            wake_on_packet: true,
            sleep_motd: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AutoscaleRegistry {
    #[serde(default)]
    pub policies: Vec<ServerAutoscalePolicy>,
}

impl AutoscaleRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.autoscale_file.exists() {
            let content = fs::read_to_string(&paths.autoscale_file)?;
            let reg: AutoscaleRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse autoscale.toml: {}", e)))?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    fn save_internal(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize autoscale.toml: {}", e)))?;
        let temp_path = paths.autoscale_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.autoscale_file)?;
        Ok(())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        fs::create_dir_all(&paths.locks_dir)?;
        let lock_file_path = paths.locks_dir.join("autoscale.lock");
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

    pub fn modify<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut AutoscaleRegistry) -> Result<R>,
    {
        fs::create_dir_all(&paths.locks_dir)?;
        let lock_file_path = paths.locks_dir.join("autoscale.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;

        let mut registry = if paths.autoscale_file.exists() {
            let content = fs::read_to_string(&paths.autoscale_file)?;
            toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse autoscale.toml: {}", e)))?
        } else {
            AutoscaleRegistry::default()
        };

        let result = f(&mut registry)?;
        registry.save_internal(paths)?;

        let _ = lock_file.unlock();
        Ok(result)
    }

    pub fn get_policy(&self, server: &str) -> Option<&ServerAutoscalePolicy> {
        self.policies.iter().find(|p| p.server_name.eq_ignore_ascii_case(server))
    }

    pub fn get_policy_or_default(&self, server: &str) -> ServerAutoscalePolicy {
        self.get_policy(server)
            .cloned()
            .unwrap_or_else(|| ServerAutoscalePolicy::new(server))
    }

    pub fn set_policy(&mut self, policy: ServerAutoscalePolicy) {
        if let Some(existing) = self
            .policies
            .iter_mut()
            .find(|p| p.server_name.eq_ignore_ascii_case(&policy.server_name))
        {
            *existing = policy;
        } else {
            self.policies.push(policy);
        }
    }

    pub fn remove_policy(&mut self, server: &str) -> bool {
        let initial = self.policies.len();
        self.policies.retain(|p| !p.server_name.eq_ignore_ascii_case(server));
        self.policies.len() < initial
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_autoscale_policy_crud_and_locking() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        // Default empty load
        let reg = AutoscaleRegistry::load(&paths).unwrap();
        assert!(reg.policies.is_empty());

        // Add policy via modify
        AutoscaleRegistry::modify(&paths, |r| {
            let mut pol = ServerAutoscalePolicy::new("survival");
            pol.hibernation_enabled = true;
            pol.idle_timeout_mins = 20;
            pol.sleep_motd = Some("Server sleeping".to_string());
            r.set_policy(pol);
            Ok(())
        })
        .unwrap();

        // Reload and assert
        let reloaded = AutoscaleRegistry::load(&paths).unwrap();
        let p = reloaded.get_policy("survival").unwrap();
        assert!(p.hibernation_enabled);
        assert_eq!(p.idle_timeout_mins, 20);
        assert_eq!(p.sleep_motd.as_deref(), Some("Server sleeping"));

        // Default fallback
        let def = reloaded.get_policy_or_default("nonexistent");
        assert_eq!(def.server_name, "nonexistent");
        assert!(!def.hibernation_enabled);
    }
}
