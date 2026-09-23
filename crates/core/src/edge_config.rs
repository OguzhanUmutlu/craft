use crate::audit::compute_hmac_sha256;
use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_unique_id(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(&now.to_be_bytes());
    hasher.update(&count.to_be_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{}-{}", prefix, &digest[..16])
}

pub const DEFAULT_CHAT_SECRET: &str = "craft_edge_chat_mesh_secret_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    LowestLatency,
    WeightedRoundRobin,
    RegionAffinity,
    FailoverOnly,
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        RoutingStrategy::LowestLatency
    }
}

impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::LowestLatency => write!(f, "LowestLatency"),
            RoutingStrategy::WeightedRoundRobin => write!(f, "WeightedRoundRobin"),
            RoutingStrategy::RegionAffinity => write!(f, "RegionAffinity"),
            RoutingStrategy::FailoverOnly => write!(f, "FailoverOnly"),
        }
    }
}

impl std::str::FromStr for RoutingStrategy {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "lowestlatency" | "lowest_latency" | "latency" => Ok(RoutingStrategy::LowestLatency),
            "weightedroundrobin" | "weighted_round_robin" | "wrr" | "weighted" => {
                Ok(RoutingStrategy::WeightedRoundRobin)
            }
            "regionaffinity" | "region_affinity" | "region" | "geo" => {
                Ok(RoutingStrategy::RegionAffinity)
            }
            "failoveronly" | "failover_only" | "failover" => Ok(RoutingStrategy::FailoverOnly),
            other => Err(CraftError::Other(format!(
                "Unknown routing strategy '{}'. Supported: lowest_latency, weighted_round_robin, region_affinity, failover_only",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoRoutingPolicy {
    pub strategy: RoutingStrategy,
    pub health_check_interval_secs: u64,
    pub latency_penalty_ms: u64,
    pub max_acceptable_jitter_ms: f64,
    pub default_region: String,
}

impl Default for GeoRoutingPolicy {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::LowestLatency,
            health_check_interval_secs: 15,
            latency_penalty_ms: 50,
            max_acceptable_jitter_ms: 30.0,
            default_region: "us-east".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeNode {
    pub name: String,
    pub region: String,
    pub endpoint: String,
    pub weight: u32,
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl EdgeNode {
    pub fn new(name: impl Into<String>, region: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            region: region.into(),
            endpoint: endpoint.into(),
            weight: 100,
            enabled: true,
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemStackSnapshot {
    pub slot: u32,
    pub item_id: String,
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durability: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbt_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PotionEffectSnapshot {
    pub effect_id: String,
    pub duration_ticks: u32,
    pub amplifier: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventorySnapshot {
    pub player_uuid: String,
    pub experience_level: u32,
    pub experience_progress: f32,
    pub health: f32,
    pub food_level: u32,
    pub saturation: f32,
    #[serde(default)]
    pub main_inventory: Vec<ItemStackSnapshot>,
    #[serde(default)]
    pub armor: Vec<ItemStackSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offhand: Option<ItemStackSnapshot>,
    #[serde(default)]
    pub ender_chest: Vec<ItemStackSnapshot>,
    #[serde(default)]
    pub active_effects: Vec<PotionEffectSnapshot>,
    pub checksum_sha256: String,
}

impl InventorySnapshot {
    pub fn compute_checksum(
        player_uuid: &str,
        level: u32,
        health: f32,
        food: u32,
        main_inventory: &[ItemStackSnapshot],
        armor: &[ItemStackSnapshot],
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(player_uuid.as_bytes());
        hasher.update(&level.to_be_bytes());
        hasher.update(&health.to_bits().to_be_bytes());
        hasher.update(&food.to_be_bytes());
        for item in main_inventory {
            hasher.update(&item.slot.to_be_bytes());
            hasher.update(item.item_id.as_bytes());
            hasher.update(&item.count.to_be_bytes());
        }
        for item in armor {
            hasher.update(&item.slot.to_be_bytes());
            hasher.update(item.item_id.as_bytes());
            hasher.update(&item.count.to_be_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn validate_checksum(&self) -> bool {
        let expected = Self::compute_checksum(
            &self.player_uuid,
            self.experience_level,
            self.health,
            self.food_level,
            &self.main_inventory,
            &self.armor,
        );
        self.checksum_sha256 == expected
    }

    pub fn serialize_compact_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| CraftError::Other(format!("Failed to serialize inventory: {}", e)))
    }

    pub fn deserialize_compact_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| CraftError::Other(format!("Failed to deserialize inventory: {}", e)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerSessionHandoff {
    pub session_id: String,
    pub player_uuid: String,
    pub player_name: String,
    pub source_server: String,
    pub target_server: String,
    pub source_region: String,
    pub target_region: String,
    pub transfer_token: String,
    pub created_at: u64,
    pub expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory_snapshot: Option<InventorySnapshot>,
}

impl PlayerSessionHandoff {
    pub fn new(
        player_uuid: impl Into<String>,
        player_name: impl Into<String>,
        source_server: impl Into<String>,
        target_server: impl Into<String>,
        source_region: impl Into<String>,
        target_region: impl Into<String>,
        ttl_secs: u64,
        inventory_snapshot: Option<InventorySnapshot>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let session_id = generate_unique_id("sess");
        let mut hasher = Sha256::new();
        hasher.update(session_id.as_bytes());
        hasher.update(&now.to_be_bytes());
        let transfer_token = format!("{:x}", hasher.finalize())[..32].to_string();

        Self {
            session_id,
            player_uuid: player_uuid.into(),
            player_name: player_name.into(),
            source_server: source_server.into(),
            target_server: target_server.into(),
            source_region: source_region.into(),
            target_region: target_region.into(),
            transfer_token,
            created_at: now,
            expires_at: now + ttl_secs,
            inventory_snapshot,
        }
    }

    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time >= self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossRegionChatEnvelope {
    pub id: String,
    pub channel: String,
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_uuid: Option<String>,
    pub content: String,
    pub timestamp: u64,
    pub origin_region: String,
    pub signature: String,
}

impl CrossRegionChatEnvelope {
    pub fn compute_signature(
        secret: &str,
        channel: &str,
        sender: &str,
        content: &str,
        timestamp: u64,
        origin_region: &str,
    ) -> String {
        let mut data = Vec::new();
        data.extend_from_slice(channel.as_bytes());
        data.push(b'|');
        data.extend_from_slice(sender.as_bytes());
        data.push(b'|');
        data.extend_from_slice(content.as_bytes());
        data.push(b'|');
        data.extend_from_slice(&timestamp.to_be_bytes());
        data.push(b'|');
        data.extend_from_slice(origin_region.as_bytes());
        hex::encode(compute_hmac_sha256(secret.as_bytes(), &data))
    }

    pub fn new(
        channel: impl Into<String>,
        sender: impl Into<String>,
        sender_uuid: Option<String>,
        content: impl Into<String>,
        origin_region: impl Into<String>,
        secret: &str,
    ) -> Self {
        let channel = channel.into();
        let sender = sender.into();
        let content = content.into();
        let origin_region = origin_region.into();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let signature = Self::compute_signature(secret, &channel, &sender, &content, now, &origin_region);
        let id = generate_unique_id("chat");

        Self {
            id,
            channel,
            sender,
            sender_uuid,
            content,
            timestamp: now,
            origin_region,
            signature,
        }
    }

    pub fn verify_signature(&self, secret: &str) -> bool {
        let expected = Self::compute_signature(
            secret,
            &self.channel,
            &self.sender,
            &self.content,
            self.timestamp,
            &self.origin_region,
        );
        self.signature == expected
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackboneStatus {
    Optimal,
    Elevated,
    Degraded,
    Critical,
}

impl std::fmt::Display for BackboneStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackboneStatus::Optimal => write!(f, "Optimal"),
            BackboneStatus::Elevated => write!(f, "Elevated"),
            BackboneStatus::Degraded => write!(f, "Degraded"),
            BackboneStatus::Critical => write!(f, "Critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackboneCondition {
    pub source_region: String,
    pub target_region: String,
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub status: BackboneStatus,
    pub updated_at: u64,
}

impl BackboneCondition {
    pub fn evaluate_status(rtt_ms: f64, jitter_ms: f64, loss_pct: f64) -> BackboneStatus {
        if loss_pct > 10.0 || rtt_ms > 250.0 || jitter_ms > 60.0 {
            BackboneStatus::Critical
        } else if loss_pct > 3.0 || rtt_ms > 150.0 || jitter_ms > 30.0 {
            BackboneStatus::Degraded
        } else if loss_pct > 0.5 || rtt_ms > 80.0 || jitter_ms > 15.0 {
            BackboneStatus::Elevated
        } else {
            BackboneStatus::Optimal
        }
    }

    pub fn new(source_region: impl Into<String>, target_region: impl Into<String>, rtt_ms: f64, jitter_ms: f64, loss_pct: f64) -> Self {
        let status = Self::evaluate_status(rtt_ms, jitter_ms, loss_pct);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            source_region: source_region.into(),
            target_region: target_region.into(),
            rtt_ms,
            jitter_ms,
            packet_loss_pct: loss_pct,
            status,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybookPreset {
    Performance,
    Balanced,
    Fidelity,
    DegradedSafe,
}

impl std::fmt::Display for PlaybookPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybookPreset::Performance => write!(f, "Performance"),
            PlaybookPreset::Balanced => write!(f, "Balanced"),
            PlaybookPreset::Fidelity => write!(f, "Fidelity"),
            PlaybookPreset::DegradedSafe => write!(f, "DegradedSafe"),
        }
    }
}

impl std::str::FromStr for PlaybookPreset {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "performance" | "perf" | "fast" => Ok(PlaybookPreset::Performance),
            "balanced" | "default" => Ok(PlaybookPreset::Balanced),
            "fidelity" | "high" | "quality" => Ok(PlaybookPreset::Fidelity),
            "degradedsafe" | "degraded_safe" | "safe" | "emergency" => Ok(PlaybookPreset::DegradedSafe),
            other => Err(CraftError::Other(format!(
                "Unknown playbook preset '{}'. Supported: performance, balanced, fidelity, degraded_safe",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencyPlaybook {
    pub preset: PlaybookPreset,
    pub view_distance: u32,
    pub simulation_distance: u32,
    pub entity_tracking_range_percent: u32,
    pub target_tps: f64,
}

impl LatencyPlaybook {
    pub fn from_preset(preset: PlaybookPreset) -> Self {
        match preset {
            PlaybookPreset::Performance => Self {
                preset,
                view_distance: 6,
                simulation_distance: 4,
                entity_tracking_range_percent: 75,
                target_tps: 20.0,
            },
            PlaybookPreset::Balanced => Self {
                preset,
                view_distance: 8,
                simulation_distance: 6,
                entity_tracking_range_percent: 100,
                target_tps: 20.0,
            },
            PlaybookPreset::Fidelity => Self {
                preset,
                view_distance: 12,
                simulation_distance: 10,
                entity_tracking_range_percent: 120,
                target_tps: 20.0,
            },
            PlaybookPreset::DegradedSafe => Self {
                preset,
                view_distance: 4,
                simulation_distance: 3,
                entity_tracking_range_percent: 50,
                target_tps: 18.0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EdgeRegistry {
    #[serde(default)]
    pub nodes: Vec<EdgeNode>,
    #[serde(default)]
    pub policy: GeoRoutingPolicy,
    #[serde(default)]
    pub playbooks: HashMap<String, LatencyPlaybook>,
}

impl EdgeRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.edge_file.exists() {
            let default_reg = Self::default();
            default_reg.save(paths)?;
            return Ok(default_reg);
        }

        let content = fs::read_to_string(&paths.edge_file).map_err(|e| {
            CraftError::Config(format!("Failed to read edge config file {:?}: {}", paths.edge_file, e))
        })?;

        toml::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse edge config file {:?}: {}", paths.edge_file, e))
        })
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.edge_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize edge config: {}", e))
        })?;
        let temp_file = paths.edge_file.with_extension("tmp");
        fs::write(&temp_file, content)?;
        fs::rename(temp_file, &paths.edge_file)?;
        Ok(())
    }

    pub fn modify<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.edge_lock.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.edge_lock)?;

        lock_file.lock_exclusive()?;

        let res = (|| {
            let mut registry = Self::load(paths)?;
            let ret = f(&mut registry)?;
            registry.save(paths)?;
            Ok(ret)
        })();

        let _ = lock_file.unlock();
        res
    }

    pub fn add_node(&mut self, node: EdgeNode) -> Result<()> {
        if self.nodes.iter().any(|n| n.name.eq_ignore_ascii_case(&node.name)) {
            return Err(CraftError::Other(format!(
                "Edge node '{}' is already registered in edge mesh",
                node.name
            )));
        }
        self.nodes.push(node);
        Ok(())
    }

    pub fn remove_node(&mut self, name: &str) -> Result<bool> {
        let initial_len = self.nodes.len();
        self.nodes.retain(|n| !n.name.eq_ignore_ascii_case(name));
        Ok(self.nodes.len() < initial_len)
    }

    pub fn get_node(&self, name: &str) -> Option<&EdgeNode> {
        self.nodes.iter().find(|n| n.name.eq_ignore_ascii_case(name))
    }

    pub fn list_nodes(&self) -> &[EdgeNode] {
        &self.nodes
    }

    pub fn find_lowest_latency_node<'a>(&'a self, latencies: &[(String, f64)]) -> Option<&'a EdgeNode> {
        let mut best: Option<(&'a EdgeNode, f64)> = None;

        for node in &self.nodes {
            if !node.enabled {
                continue;
            }
            if let Some((_, rtt)) = latencies.iter().find(|(name, _)| name.eq_ignore_ascii_case(&node.name)) {
                match best {
                    None => best = Some((node, *rtt)),
                    Some((_, best_rtt)) if *rtt < best_rtt => best = Some((node, *rtt)),
                    _ => {}
                }
            }
        }

        best.map(|(node, _)| node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_edge_registry_lifecycle_and_lock() {
        let temp_dir = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());

        let node1 = EdgeNode::new("edge-us-east", "us-east", "10.0.1.10:25565");
        let node2 = EdgeNode::new("edge-eu-west", "eu-west", "10.0.2.10:25565");

        EdgeRegistry::modify(&paths, |reg| {
            reg.add_node(node1.clone())?;
            reg.add_node(node2.clone())?;
            Ok(())
        })
        .unwrap();

        let loaded = EdgeRegistry::load(&paths).unwrap();
        assert_eq!(loaded.nodes.len(), 2);
        assert!(loaded.get_node("edge-us-east").is_some());
        assert!(loaded.get_node("edge-eu-west").is_some());

        // Remove node
        EdgeRegistry::modify(&paths, |reg| {
            let removed = reg.remove_node("edge-us-east")?;
            assert!(removed);
            Ok(())
        })
        .unwrap();

        let loaded2 = EdgeRegistry::load(&paths).unwrap();
        assert_eq!(loaded2.nodes.len(), 1);
        assert!(loaded2.get_node("edge-us-east").is_none());
        assert!(loaded2.get_node("edge-eu-west").is_some());
    }

    #[test]
    fn test_inventory_snapshot_binary_roundtrip() {
        let items = vec![
            ItemStackSnapshot {
                slot: 0,
                item_id: "minecraft:diamond_sword".to_string(),
                count: 1,
                durability: Some(1500),
                custom_name: Some("Excalibur".to_string()),
                nbt_hash: None,
            },
            ItemStackSnapshot {
                slot: 1,
                item_id: "minecraft:golden_apple".to_string(),
                count: 64,
                durability: None,
                custom_name: None,
                nbt_hash: None,
            },
        ];
        let armor = vec![ItemStackSnapshot {
            slot: 103,
            item_id: "minecraft:netherite_helmet".to_string(),
            count: 1,
            durability: Some(400),
            custom_name: None,
            nbt_hash: None,
        }];

        let checksum = InventorySnapshot::compute_checksum(
            "123e4567-e89b-12d3-a456-426614174000",
            30,
            20.0,
            20,
            &items,
            &armor,
        );

        let snapshot = InventorySnapshot {
            player_uuid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            experience_level: 30,
            experience_progress: 0.5,
            health: 20.0,
            food_level: 20,
            saturation: 5.0,
            main_inventory: items,
            armor,
            offhand: None,
            ender_chest: Vec::new(),
            active_effects: vec![PotionEffectSnapshot {
                effect_id: "minecraft:speed".to_string(),
                duration_ticks: 1200,
                amplifier: 1,
            }],
            checksum_sha256: checksum,
        };

        assert!(snapshot.validate_checksum());

        let bytes = snapshot.serialize_compact_json().unwrap();
        let decoded = InventorySnapshot::deserialize_compact_json(&bytes).unwrap();
        assert_eq!(snapshot, decoded);
        assert!(decoded.validate_checksum());
    }

    #[test]
    fn test_chat_envelope_signature_verification() {
        let secret = "test_edge_secret_key";
        let env = CrossRegionChatEnvelope::new(
            "global",
            "Notch",
            Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
            "Hello from us-east!",
            "us-east",
            secret,
        );

        assert!(env.verify_signature(secret));
        assert!(!env.verify_signature("wrong_secret"));

        let mut tampered = env.clone();
        tampered.content = "Tampered message content!".to_string();
        assert!(!tampered.verify_signature(secret));
    }

    #[test]
    fn test_latency_playbook_presets() {
        let perf = LatencyPlaybook::from_preset(PlaybookPreset::Performance);
        assert_eq!(perf.view_distance, 6);
        assert_eq!(perf.simulation_distance, 4);

        let safe = LatencyPlaybook::from_preset(PlaybookPreset::DegradedSafe);
        assert_eq!(safe.view_distance, 4);
        assert_eq!(safe.target_tps, 18.0);
    }

    #[test]
    fn test_backbone_condition_evaluation() {
        assert_eq!(
            BackboneCondition::evaluate_status(25.0, 5.0, 0.0),
            BackboneStatus::Optimal
        );
        assert_eq!(
            BackboneCondition::evaluate_status(100.0, 20.0, 1.0),
            BackboneStatus::Elevated
        );
        assert_eq!(
            BackboneCondition::evaluate_status(180.0, 40.0, 4.0),
            BackboneStatus::Degraded
        );
        assert_eq!(
            BackboneCondition::evaluate_status(300.0, 70.0, 15.0),
            BackboneStatus::Critical
        );
    }
}
