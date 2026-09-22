use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebhookKind {
    Discord,
    Slack,
    GenericJson,
}

impl std::fmt::Display for WebhookKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebhookKind::Discord => write!(f, "discord"),
            WebhookKind::Slack => write!(f, "slack"),
            WebhookKind::GenericJson => write!(f, "generic"),
        }
    }
}

impl FromStr for WebhookKind {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().trim() {
            "discord" => Ok(WebhookKind::Discord),
            "slack" => Ok(WebhookKind::Slack),
            "generic" | "genericjson" | "json" => Ok(WebhookKind::GenericJson),
            other => Err(CraftError::Other(format!(
                "Unknown webhook kind '{}'. Valid kinds: discord, slack, generic",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    ServerCrash,
    AutoRestart,
    CircuitTrip,
    BackupComplete,
    StorageExhaustion,
    ServerStart,
    ServerStop,
    All,
}

impl std::fmt::Display for WebhookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebhookEvent::ServerCrash => write!(f, "server_crash"),
            WebhookEvent::AutoRestart => write!(f, "auto_restart"),
            WebhookEvent::CircuitTrip => write!(f, "circuit_trip"),
            WebhookEvent::BackupComplete => write!(f, "backup_complete"),
            WebhookEvent::StorageExhaustion => write!(f, "storage_exhaustion"),
            WebhookEvent::ServerStart => write!(f, "server_start"),
            WebhookEvent::ServerStop => write!(f, "server_stop"),
            WebhookEvent::All => write!(f, "all"),
        }
    }
}

impl FromStr for WebhookEvent {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().replace('-', "_").trim() {
            "server_crash" | "crash" => Ok(WebhookEvent::ServerCrash),
            "auto_restart" | "restart" => Ok(WebhookEvent::AutoRestart),
            "circuit_trip" | "circuit_breaker" | "trip" => Ok(WebhookEvent::CircuitTrip),
            "backup_complete" | "backup" => Ok(WebhookEvent::BackupComplete),
            "storage_exhaustion" | "storage" | "disk" => Ok(WebhookEvent::StorageExhaustion),
            "server_start" | "start" => Ok(WebhookEvent::ServerStart),
            "server_stop" | "stop" => Ok(WebhookEvent::ServerStop),
            "all" | "*" => Ok(WebhookEvent::All),
            other => Err(CraftError::Other(format!(
                "Unknown webhook event '{}'. Valid events: crash, restart, circuit_trip, backup, storage, start, stop, all",
                other
            ))),
        }
    }
}

impl WebhookEvent {
    pub fn matches(&self, target: &WebhookEvent) -> bool {
        if *self == WebhookEvent::All || *target == WebhookEvent::All {
            true
        } else {
            self == target
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: String,
    pub url: String,
    pub kind: WebhookKind,
    #[serde(default)]
    pub events: Vec<WebhookEvent>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

fn default_true() -> bool {
    true
}

impl WebhookEndpoint {
    pub fn new(
        id: impl Into<String>,
        url: impl Into<String>,
        kind: WebhookKind,
        events: Vec<WebhookEvent>,
    ) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            kind,
            events,
            enabled: true,
            secret: None,
        }
    }

    pub fn subscribes_to(&self, event: &WebhookEvent) -> bool {
        if !self.enabled {
            return false;
        }
        if self.events.is_empty() {
            return true; // Default to all events if none specified
        }
        self.events.iter().any(|e| e.matches(event))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WebhooksRegistry {
    #[serde(default)]
    pub webhooks: Vec<WebhookEndpoint>,
}

impl WebhooksRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.webhooks_file.exists() {
            let content = fs::read_to_string(&paths.webhooks_file)?;
            let registry: WebhooksRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse webhooks.toml: {}", e)))?;
            return Ok(registry);
        }
        Ok(Self::default())
    }

    fn save_internal(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize webhooks.toml: {}", e)))?;

        let temp_path = paths.webhooks_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.webhooks_file)?;
        Ok(())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        fs::create_dir_all(&paths.locks_dir)?;
        let lock_file_path = paths.locks_dir.join("webhooks.lock");
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
        F: FnOnce(&mut WebhooksRegistry) -> Result<R>,
    {
        fs::create_dir_all(&paths.locks_dir)?;
        let lock_file_path = paths.locks_dir.join("webhooks.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;

        let mut registry = if paths.webhooks_file.exists() {
            let content = fs::read_to_string(&paths.webhooks_file)?;
            toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse webhooks.toml: {}", e)))?
        } else {
            WebhooksRegistry::default()
        };

        let result = f(&mut registry)?;
        registry.save_internal(paths)?;

        let _ = lock_file.unlock();
        Ok(result)
    }

    pub fn add(&mut self, endpoint: WebhookEndpoint) -> Result<()> {
        if self
            .webhooks
            .iter()
            .any(|w| w.id.eq_ignore_ascii_case(&endpoint.id))
        {
            return Err(CraftError::Other(format!(
                "Webhook with ID '{}' already exists",
                endpoint.id
            )));
        }
        self.webhooks.push(endpoint);
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let initial_len = self.webhooks.len();
        self.webhooks.retain(|w| !w.id.eq_ignore_ascii_case(id));
        self.webhooks.len() < initial_len
    }

    pub fn find(&self, id: &str) -> Option<&WebhookEndpoint> {
        self.webhooks
            .iter()
            .find(|w| w.id.eq_ignore_ascii_case(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut WebhookEndpoint> {
        self.webhooks
            .iter_mut()
            .find(|w| w.id.eq_ignore_ascii_case(id))
    }

    pub fn subscribers_for(&self, event: &WebhookEvent) -> Vec<&WebhookEndpoint> {
        self.webhooks
            .iter()
            .filter(|w| w.subscribes_to(event))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_webhook_event_parsing_and_matching() {
        assert_eq!(
            WebhookEvent::from_str("crash").unwrap(),
            WebhookEvent::ServerCrash
        );
        assert_eq!(
            WebhookEvent::from_str("restart").unwrap(),
            WebhookEvent::AutoRestart
        );
        assert_eq!(
            WebhookEvent::from_str("circuit_trip").unwrap(),
            WebhookEvent::CircuitTrip
        );
        assert_eq!(
            WebhookEvent::from_str("backup").unwrap(),
            WebhookEvent::BackupComplete
        );
        assert_eq!(
            WebhookEvent::from_str("storage").unwrap(),
            WebhookEvent::StorageExhaustion
        );
        assert_eq!(
            WebhookEvent::from_str("start").unwrap(),
            WebhookEvent::ServerStart
        );
        assert_eq!(
            WebhookEvent::from_str("stop").unwrap(),
            WebhookEvent::ServerStop
        );
        assert_eq!(WebhookEvent::from_str("all").unwrap(), WebhookEvent::All);

        assert!(WebhookEvent::All.matches(&WebhookEvent::ServerCrash));
        assert!(WebhookEvent::ServerCrash.matches(&WebhookEvent::All));
        assert!(WebhookEvent::ServerCrash.matches(&WebhookEvent::ServerCrash));
        assert!(!WebhookEvent::ServerCrash.matches(&WebhookEvent::BackupComplete));
    }

    #[test]
    fn test_webhook_kind_parsing() {
        assert_eq!(
            WebhookKind::from_str("discord").unwrap(),
            WebhookKind::Discord
        );
        assert_eq!(WebhookKind::from_str("slack").unwrap(), WebhookKind::Slack);
        assert_eq!(
            WebhookKind::from_str("generic").unwrap(),
            WebhookKind::GenericJson
        );
        assert_eq!(
            WebhookKind::from_str("json").unwrap(),
            WebhookKind::GenericJson
        );
    }

    #[test]
    fn test_webhooks_registry_lifecycle() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let mut reg = WebhooksRegistry::load(&paths).unwrap();
        assert!(reg.webhooks.is_empty());

        let ep = WebhookEndpoint::new(
            "discord-ops",
            "https://discord.com/api/webhooks/123/abc",
            WebhookKind::Discord,
            vec![WebhookEvent::ServerCrash, WebhookEvent::CircuitTrip],
        );
        reg.add(ep).unwrap();

        // Duplicate rejection
        let dup = WebhookEndpoint::new(
            "discord-ops",
            "https://other.url",
            WebhookKind::Discord,
            vec![],
        );
        assert!(reg.add(dup).is_err());

        reg.save(&paths).unwrap();

        let loaded = WebhooksRegistry::load(&paths).unwrap();
        assert_eq!(loaded.webhooks.len(), 1);
        let item = loaded.find("discord-ops").unwrap();
        assert_eq!(item.kind, WebhookKind::Discord);
        assert!(item.subscribes_to(&WebhookEvent::ServerCrash));
        assert!(!item.subscribes_to(&WebhookEvent::BackupComplete));

        // Subscribers for event
        let subs = loaded.subscribers_for(&WebhookEvent::ServerCrash);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].id, "discord-ops");

        let non_subs = loaded.subscribers_for(&WebhookEvent::BackupComplete);
        assert!(non_subs.is_empty());

        // Removal
        let mut reg2 = loaded;
        assert!(reg2.remove("discord-ops"));
        assert!(!reg2.remove("discord-ops"));
        assert!(reg2.webhooks.is_empty());
    }
}
