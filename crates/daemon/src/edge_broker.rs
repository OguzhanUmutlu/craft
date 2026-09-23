use craft_core::{
    BackboneCondition, CraftError, CraftPaths, CrossRegionChatEnvelope, LatencyPlaybook,
    PlaybookPreset, PlayerSessionHandoff, Result, ServersRegistry,
};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct EdgeStateBroker {
    handoffs: Arc<RwLock<HashMap<String, PlayerSessionHandoff>>>,
    chat_history: Arc<RwLock<Vec<CrossRegionChatEnvelope>>>,
    backbone_conditions: Arc<RwLock<Vec<BackboneCondition>>>,
}

impl Default for EdgeStateBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl EdgeStateBroker {
    pub fn new() -> Self {
        Self {
            handoffs: Arc::new(RwLock::new(HashMap::new())),
            chat_history: Arc::new(RwLock::new(Vec::new())),
            backbone_conditions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Registers a cross-region player session handoff with single-use token and TTL
    pub async fn register_handoff(&self, handoff: PlayerSessionHandoff) -> Result<String> {
        let token = handoff.transfer_token.clone();
        let mut map = self.handoffs.write().await;
        map.insert(token.clone(), handoff);
        Ok(token)
    }

    /// Atomically consumes a session handoff by transfer token (single-use guard against dupes)
    pub async fn consume_handoff(&self, token: &str) -> Result<PlayerSessionHandoff> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut map = self.handoffs.write().await;
        let handoff = map.remove(token).ok_or_else(|| {
            CraftError::Other(format!(
                "Invalid or already consumed session handoff token '{}'",
                token
            ))
        })?;

        if handoff.is_expired(now) {
            return Err(CraftError::Other(format!(
                "Session handoff token '{}' has expired (created at {}, expired at {})",
                token, handoff.created_at, handoff.expires_at
            )));
        }

        Ok(handoff)
    }

    /// Returns all currently valid active handoffs, pruning expired records
    pub async fn get_active_handoffs(&self) -> Vec<PlayerSessionHandoff> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut map = self.handoffs.write().await;
        map.retain(|_, h| !h.is_expired(now));
        map.values().cloned().collect()
    }

    /// Broadcasts a cross-region chat envelope after verifying HMAC signature
    pub async fn broadcast_chat(
        &self,
        envelope: CrossRegionChatEnvelope,
        secret: &str,
    ) -> Result<usize> {
        if !envelope.verify_signature(secret) {
            return Err(CraftError::Other(
                "Rejected chat envelope: HMAC signature verification failed".to_string(),
            ));
        }

        let mut history = self.chat_history.write().await;
        history.push(envelope);
        if history.len() > 1000 {
            history.remove(0);
        }

        Ok(history.len())
    }

    /// Returns the most recent chat envelopes up to `limit`
    pub async fn get_chat_history(&self, limit: usize) -> Vec<CrossRegionChatEnvelope> {
        let history = self.chat_history.read().await;
        let count = history.len().min(limit);
        history[history.len() - count..].to_vec()
    }

    /// Updates current inter-region backbone conditions
    pub async fn update_backbone_conditions(&self, conditions: Vec<BackboneCondition>) {
        let mut bc = self.backbone_conditions.write().await;
        *bc = conditions;
    }

    /// Retrieves current backbone conditions
    pub async fn get_backbone_conditions(&self) -> Vec<BackboneCondition> {
        self.backbone_conditions.read().await.clone()
    }

    /// Applies a latency optimization playbook to a server's properties
    pub fn apply_playbook_to_server(
        paths: &CraftPaths,
        server_name: &str,
        preset_str: &str,
    ) -> Result<LatencyPlaybook> {
        let preset: PlaybookPreset = preset_str.parse()?;
        let playbook = LatencyPlaybook::from_preset(preset);

        let registry = ServersRegistry::load(paths)?;
        let server = registry.find_by_name(server_name).ok_or_else(|| {
            CraftError::Other(format!("Server '{}' not found in registry", server_name))
        })?;

        let props_path = server.path.join("server.properties");
        if props_path.exists() {
            let content = fs::read_to_string(&props_path)?;
            let mut updated_lines = Vec::new();
            let mut found_vd = false;
            let mut found_sd = false;
            let mut found_eb = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("view-distance=") {
                    updated_lines.push(format!("view-distance={}", playbook.view_distance));
                    found_vd = true;
                } else if trimmed.starts_with("simulation-distance=") {
                    updated_lines.push(format!("simulation-distance={}", playbook.simulation_distance));
                    found_sd = true;
                } else if trimmed.starts_with("entity-broadcast-range-percentage=") {
                    updated_lines.push(format!("entity-broadcast-range-percentage={}", playbook.entity_tracking_range_percent));
                    found_eb = true;
                } else {
                    updated_lines.push(line.to_string());
                }
            }

            if !found_vd {
                updated_lines.push(format!("view-distance={}", playbook.view_distance));
            }
            if !found_sd {
                updated_lines.push(format!("simulation-distance={}", playbook.simulation_distance));
            }
            if !found_eb {
                updated_lines.push(format!("entity-broadcast-range-percentage={}", playbook.entity_tracking_range_percent));
            }

            fs::write(&props_path, updated_lines.join("\n"))?;
        }

        Ok(playbook)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::{DEFAULT_CHAT_SECRET, ServerConfig};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_handoff_lifecycle_and_single_use_guard() {
        let broker = EdgeStateBroker::new();

        let handoff = PlayerSessionHandoff::new(
            "test-uuid-1",
            "Player1",
            "lobby-1",
            "survival-1",
            "us-east",
            "eu-west",
            30,
            None,
        );

        let token = broker.register_handoff(handoff.clone()).await.unwrap();
        assert_eq!(token, handoff.transfer_token);

        let active = broker.get_active_handoffs().await;
        assert_eq!(active.len(), 1);

        // First consumption succeeds
        let consumed = broker.consume_handoff(&token).await.unwrap();
        assert_eq!(consumed.player_name, "Player1");

        // Second consumption MUST fail (single use duplication guard)
        let second_try = broker.consume_handoff(&token).await;
        assert!(second_try.is_err());

        // Map is now empty
        let remaining = broker.get_active_handoffs().await;
        assert_eq!(remaining.len(), 0);
    }

    #[tokio::test]
    async fn test_handoff_expiration() {
        let broker = EdgeStateBroker::new();

        let mut handoff = PlayerSessionHandoff::new(
            "test-uuid-2",
            "Player2",
            "lobby-1",
            "hub-2",
            "us-east",
            "us-east",
            1, // 1 second TTL
            None,
        );
        // Force expiration
        handoff.expires_at = 1;

        let token = broker.register_handoff(handoff).await.unwrap();

        // Expired handoff cannot be consumed
        let res = broker.consume_handoff(&token).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_chat_broadcast_and_verification() {
        let broker = EdgeStateBroker::new();
        let secret = DEFAULT_CHAT_SECRET;

        let env = CrossRegionChatEnvelope::new(
            "global",
            "Steve",
            None,
            "Hello edge mesh!",
            "us-east",
            secret,
        );

        let count = broker.broadcast_chat(env, secret).await.unwrap();
        assert_eq!(count, 1);

        let history = broker.get_chat_history(10).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Hello edge mesh!");
    }

    #[test]
    fn test_apply_playbook_to_server_properties() {
        let temp_dir = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());
        fs::create_dir_all(&paths.locks_dir).unwrap();

        let server_dir = paths.servers_dir.join("test-server");
        fs::create_dir_all(&server_dir).unwrap();

        let initial_props = "motd=Craft Server\nview-distance=10\nsimulation-distance=8\n";
        fs::write(server_dir.join("server.properties"), initial_props).unwrap();

        let mut reg = ServersRegistry::default();
        reg.servers.push(ServerConfig {
            name: "test-server".to_string(),
            path: server_dir.clone(),
            software: "paper".to_string(),
            version: "1.21".to_string(),
            game: craft_core::default_game_id(),
            auto: false,
            port: Some(25565),
            query_port: None,
            rcon_port: None,
            memory: Some("4G".to_string()),
            java_path: None,
            binary_path: None,
            start_args: None,
            jvm_args: Some(Vec::new()),
            created_at: None,
            backup_method: None,
            jdwp_debug_port: None,
        });
        reg.save(&paths).unwrap();

        let playbook =
            EdgeStateBroker::apply_playbook_to_server(&paths, "test-server", "performance").unwrap();
        assert_eq!(playbook.view_distance, 6);
        assert_eq!(playbook.simulation_distance, 4);

        let updated_props = fs::read_to_string(server_dir.join("server.properties")).unwrap();
        assert!(updated_props.contains("view-distance=6"));
        assert!(updated_props.contains("simulation-distance=4"));
        assert!(updated_props.contains("entity-broadcast-range-percentage=75"));
    }
}
