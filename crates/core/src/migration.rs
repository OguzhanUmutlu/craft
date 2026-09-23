use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Operational lifecycle stages for zero-downtime live migration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum MigrationStage {
    /// Initial scheduling and pre-flight validation
    Pending,
    /// Iterative memory & storage synchronization round
    PreCopyRound {
        round: u32,
        bytes_transferred: u64,
        dirty_bytes_remaining: u64,
    },
    /// Sub-150ms execution pause, connection splicing and state handoff
    FreezeAndHandoff,
    /// Memory and state restoration on target node
    StateRestoration,
    /// Dynamic Anycast and edge proxy traffic cutover
    TrafficSwitch,
    /// Successful zero-downtime migration finalization
    Completed,
    /// Automated safety abort and state failback
    RolledBack {
        reason: String,
    },
}

impl MigrationStage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::PreCopyRound { .. } => "pre_copy_round",
            Self::FreezeAndHandoff => "freeze_and_handoff",
            Self::StateRestoration => "state_restoration",
            Self::TrafficSwitch => "traffic_switch",
            Self::Completed => "completed",
            Self::RolledBack { .. } => "rolled_back",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::RolledBack { .. })
    }

    pub fn is_active(&self) -> bool {
        !self.is_terminal() && !matches!(self, Self::Pending)
    }
}

/// Statistics for a completed iterative pre-copy pass
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCopyRound {
    pub round: u32,
    pub bytes_transferred: u64,
    pub dirty_bytes_remaining: u64,
    pub duration_ms: u64,
}

/// Comprehensive specification and execution state of a live server migration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveMigrationPlan {
    pub migration_id: String,
    pub server_name: String,
    pub source_node: String,
    pub target_node: String,
    pub target_host: String,
    pub target_port: u16,
    pub pre_copy_threshold_bytes: u64,
    pub max_pre_copy_rounds: u32,
    pub freeze_timeout_ms: u64,
    pub status: MigrationStage,
    #[serde(default)]
    pub rounds: Vec<PreCopyRound>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl LiveMigrationPlan {
    pub fn new(
        server_name: impl Into<String>,
        source_node: impl Into<String>,
        target_node: impl Into<String>,
        target_host: impl Into<String>,
        target_port: u16,
    ) -> Self {
        let now = chrono::Utc::now().timestamp() as u64;
        let s_name = server_name.into();
        let migration_id = format!("mig-{}-{}", s_name, now);
        Self {
            migration_id,
            server_name: s_name,
            source_node: source_node.into(),
            target_node: target_node.into(),
            target_host: target_host.into(),
            target_port,
            pre_copy_threshold_bytes: 50 * 1024 * 1024, // 50 MB convergence default
            max_pre_copy_rounds: 5,
            freeze_timeout_ms: 250, // 250ms freeze boundary default
            status: MigrationStage::Pending,
            rounds: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Compressed memory page chunk for iterative live migration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPageChunk {
    pub page_addr: u64,
    pub length: usize,
    pub crc32: u32,
    pub compressed_data: Vec<u8>,
}

impl MemoryPageChunk {
    pub fn from_raw(page_addr: u64, raw_data: &[u8]) -> Self {
        let crc = crate::raft::compute_crc32(raw_data);
        let compressed = zstd::encode_all(raw_data, 1).unwrap_or_else(|_| raw_data.to_vec());
        Self {
            page_addr,
            length: raw_data.len(),
            crc32: crc,
            compressed_data: compressed,
        }
    }

    pub fn decompress(&self) -> Result<Vec<u8>> {
        let decompressed = zstd::decode_all(&self.compressed_data[..])
            .map_err(|e| CraftError::Other(format!("Failed to decompress memory chunk: {}", e)))?;
        let crc = crate::raft::compute_crc32(&decompressed);
        if crc != self.crc32 {
            return Err(CraftError::Other(format!(
                "CRC32 mismatch on memory chunk at 0x{:X}: expected 0x{:08X}, got 0x{:08X}",
                self.page_addr, self.crc32, crc
            )));
        }
        Ok(decompressed)
    }
}

/// Bitmask and offset tracker for memory pages modified during pre-copy iterations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyMemoryTracker {
    pub page_size: usize,
    pub dirty_pages: BTreeSet<u64>,
    pub total_tracked_bytes: u64,
}

impl DirtyMemoryTracker {
    pub fn new(page_size: usize) -> Self {
        Self {
            page_size: if page_size == 0 { 4096 } else { page_size },
            dirty_pages: BTreeSet::new(),
            total_tracked_bytes: 0,
        }
    }

    pub fn mark_dirty(&mut self, page_addr: u64) {
        let aligned = page_addr - (page_addr % self.page_size as u64);
        self.dirty_pages.insert(aligned);
        self.total_tracked_bytes = (self.dirty_pages.len() * self.page_size) as u64;
    }

    pub fn mark_range_dirty(&mut self, start_addr: u64, len: usize) {
        if len == 0 {
            return;
        }
        let end_addr = start_addr.saturating_add(len as u64);
        let start_page = start_addr - (start_addr % self.page_size as u64);
        let mut cur = start_page;
        while cur < end_addr {
            self.dirty_pages.insert(cur);
            cur = cur.saturating_add(self.page_size as u64);
        }
        self.total_tracked_bytes = (self.dirty_pages.len() * self.page_size) as u64;
    }

    pub fn get_dirty_pages(&self) -> Vec<u64> {
        self.dirty_pages.iter().copied().collect()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_pages.clear();
        self.total_tracked_bytes = 0;
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty_pages.len()
    }

    pub fn dirty_bytes(&self) -> u64 {
        (self.dirty_pages.len() * self.page_size) as u64
    }
}

/// Connected player session state descriptor for zero-loss socket handoff
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSessionDescriptor {
    pub player_uuid: String,
    pub username: String,
    pub socket_addr: String,
    pub protocol_version: i32,
    pub entity_id: i32,
    pub cipher_state_bytes: Vec<u8>,
    pub unacked_packets_count: usize,
}

/// Manifest capturing atomic server state at the freeze point
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCheckpointManifest {
    pub migration_id: String,
    pub server_name: String,
    pub game_tick: u64,
    pub active_players: Vec<PlayerSessionDescriptor>,
    pub inventory_fencing_token: u64,
    pub memory_chunks_count: usize,
    pub memory_bytes: u64,
    pub total_checksum: u32,
    pub created_at: u64,
}

/// BGP routing advertisement status for global Anycast steering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnycastRouteStatus {
    Announced,
    Withdrawn,
    PrependPath,
}

impl AnycastRouteStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Announced => "announced",
            Self::Withdrawn => "withdrawn",
            Self::PrependPath => "prepend_path",
        }
    }
}

/// Anycast route declaration managed across regional cluster nodes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnycastRouteAnnouncement {
    pub prefix: String,
    pub asn: u32,
    #[serde(default)]
    pub community: Vec<String>,
    pub local_pref: u32,
    pub metric: u32,
    pub status: AnycastRouteStatus,
    pub active: bool,
    pub updated_at: u64,
}

impl AnycastRouteAnnouncement {
    pub fn new(prefix: impl Into<String>, asn: u32) -> Self {
        Self {
            prefix: prefix.into(),
            asn,
            community: vec!["65000:100".to_string()],
            local_pref: 100,
            metric: 10,
            status: AnycastRouteStatus::Announced,
            active: true,
            updated_at: chrono::Utc::now().timestamp() as u64,
        }
    }
}

/// Evaluates if dirty memory transfer has converged below the target freeze threshold
pub fn evaluate_pre_copy_convergence(rounds: &[PreCopyRound], threshold_bytes: u64) -> bool {
    if rounds.is_empty() {
        return false;
    }
    let last = &rounds[rounds.len() - 1];
    last.dirty_bytes_remaining <= threshold_bytes
}

/// Constructs a point-in-time checkpoint manifest for a server instance
pub fn build_server_checkpoint_manifest(
    server_path: &Path,
    migration_id: &str,
    server_name: &str,
    game_tick: u64,
) -> Result<ServerCheckpointManifest> {
    let now = chrono::Utc::now().timestamp() as u64;
    let fencing_token = crate::raft::calculate_fencing_token(1, game_tick);

    let mut total_bytes = 0u64;
    let mut combined_hash = 0u32;

    if server_path.exists() {
        if let Ok(entries) = fs::read_dir(server_path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total_bytes = total_bytes.saturating_add(meta.len());
                        if let Ok(data) = fs::read(entry.path()) {
                            let file_crc = crate::raft::compute_crc32(&data);
                            combined_hash = combined_hash.wrapping_add(file_crc);
                        }
                    }
                }
            }
        }
    }

    Ok(ServerCheckpointManifest {
        migration_id: migration_id.to_string(),
        server_name: server_name.to_string(),
        game_tick,
        active_players: Vec::new(),
        inventory_fencing_token: fencing_token,
        memory_chunks_count: 0,
        memory_bytes: total_bytes,
        total_checksum: combined_hash,
        created_at: now,
    })
}

/// Thread-safe and inter-process file-locked live migration registry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MigrationRegistry {
    #[serde(default)]
    pub plans: HashMap<String, LiveMigrationPlan>,
    #[serde(default)]
    pub routes: Vec<AnycastRouteAnnouncement>,
}

impl MigrationRegistry {
    /// Loads the migration registry from disk under an advisory shared lock
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.migrations_file.exists() {
            return Ok(Self::default());
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.migrations_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_shared().map_err(CraftError::Io)?;

        let mut content = String::new();
        let mut file = fs::File::open(&paths.migrations_file).map_err(CraftError::Io)?;
        file.read_to_string(&mut content).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();

        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse migrations.toml: {}", e)))
    }

    /// Saves the migration registry atomically under an advisory exclusive lock
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.migrations_file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }
        if let Some(parent) = paths.migrations_lock.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(CraftError::Io)?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.migrations_lock)
            .map_err(CraftError::Io)?;
        lock_file.lock_exclusive().map_err(CraftError::Io)?;

        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize migrations: {}", e)))?;

        let tmp_path = paths.migrations_dir.join(format!(
            "migrations.toml.tmp.{}",
            std::process::id()
        ));
        {
            let mut tmp_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .map_err(CraftError::Io)?;
            tmp_file.write_all(content.as_bytes()).map_err(CraftError::Io)?;
            tmp_file.flush().map_err(CraftError::Io)?;
        }

        fs::rename(&tmp_path, &paths.migrations_file).map_err(CraftError::Io)?;
        let _ = lock_file.unlock();

        Ok(())
    }

    pub fn add_plan(&mut self, plan: LiveMigrationPlan) -> Result<()> {
        if self.plans.contains_key(&plan.migration_id) {
            return Err(CraftError::Other(format!(
                "Migration plan '{}' already exists in registry",
                plan.migration_id
            )));
        }
        self.plans.insert(plan.migration_id.clone(), plan);
        Ok(())
    }

    pub fn get_plan(&self, migration_id: &str) -> Option<&LiveMigrationPlan> {
        self.plans.get(migration_id)
    }

    pub fn get_plan_mut(&mut self, migration_id: &str) -> Option<&mut LiveMigrationPlan> {
        self.plans.get_mut(migration_id)
    }

    pub fn update_status(&mut self, migration_id: &str, stage: MigrationStage) -> Result<()> {
        let plan = self.plans.get_mut(migration_id).ok_or_else(|| {
            CraftError::Other(format!("Migration plan '{}' not found", migration_id))
        })?;
        plan.status = stage;
        plan.updated_at = chrono::Utc::now().timestamp() as u64;
        Ok(())
    }

    pub fn list_plans(&self) -> Vec<&LiveMigrationPlan> {
        let mut list: Vec<&LiveMigrationPlan> = self.plans.values().collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    pub fn active_migration_for_server(&self, server_name: &str) -> Option<&LiveMigrationPlan> {
        self.plans
            .values()
            .find(|p| p.server_name == server_name && p.status.is_active())
    }

    pub fn add_route(&mut self, route: AnycastRouteAnnouncement) {
        if let Some(pos) = self.routes.iter().position(|r| r.prefix == route.prefix) {
            self.routes[pos] = route;
        } else {
            self.routes.push(route);
        }
    }

    pub fn find_route(&self, prefix: &str) -> Option<&AnycastRouteAnnouncement> {
        self.routes.iter().find(|r| r.prefix == prefix)
    }

    pub fn find_route_mut(&mut self, prefix: &str) -> Option<&mut AnycastRouteAnnouncement> {
        self.routes.iter_mut().find(|r| r.prefix == prefix)
    }

    pub fn list_routes(&self) -> &[AnycastRouteAnnouncement] {
        &self.routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_migration_stage_lifecycle() {
        let pending = MigrationStage::Pending;
        assert_eq!(pending.name(), "pending");
        assert!(!pending.is_terminal());
        assert!(!pending.is_active());

        let precopy = MigrationStage::PreCopyRound {
            round: 1,
            bytes_transferred: 1024,
            dirty_bytes_remaining: 512,
        };
        assert_eq!(precopy.name(), "pre_copy_round");
        assert!(!precopy.is_terminal());
        assert!(precopy.is_active());

        let completed = MigrationStage::Completed;
        assert_eq!(completed.name(), "completed");
        assert!(completed.is_terminal());
        assert!(!completed.is_active());

        let rolled_back = MigrationStage::RolledBack {
            reason: "freeze window exceeded".to_string(),
        };
        assert_eq!(rolled_back.name(), "rolled_back");
        assert!(rolled_back.is_terminal());
        assert!(!rolled_back.is_active());
    }

    #[test]
    fn test_memory_page_chunk_compression_roundtrip() {
        let raw = vec![42u8; 8192];
        let chunk = MemoryPageChunk::from_raw(0x7FFF0000, &raw);
        assert_eq!(chunk.page_addr, 0x7FFF0000);
        assert_eq!(chunk.length, 8192);
        assert!(chunk.compressed_data.len() < raw.len());

        let restored = chunk.decompress().expect("decompression must succeed");
        assert_eq!(restored, raw);
    }

    #[test]
    fn test_dirty_memory_tracker_ranges() {
        let mut tracker = DirtyMemoryTracker::new(4096);
        assert_eq!(tracker.dirty_count(), 0);
        assert_eq!(tracker.dirty_bytes(), 0);

        tracker.mark_dirty(0x1000);
        tracker.mark_dirty(0x1050); // Same 4KB page
        assert_eq!(tracker.dirty_count(), 1);
        assert_eq!(tracker.dirty_bytes(), 4096);

        tracker.mark_range_dirty(0x2000, 8192); // Spans 2 pages (0x2000, 0x3000)
        assert_eq!(tracker.dirty_count(), 3);
        assert_eq!(tracker.dirty_bytes(), 12288);

        let dirty = tracker.get_dirty_pages();
        assert_eq!(dirty, vec![0x1000, 0x2000, 0x3000]);

        tracker.clear_dirty();
        assert_eq!(tracker.dirty_count(), 0);
        assert_eq!(tracker.dirty_bytes(), 0);
    }

    #[test]
    fn test_pre_copy_convergence() {
        let rounds = vec![
            PreCopyRound {
                round: 1,
                bytes_transferred: 500_000_000,
                dirty_bytes_remaining: 120_000_000,
                duration_ms: 1200,
            },
            PreCopyRound {
                round: 2,
                bytes_transferred: 120_000_000,
                dirty_bytes_remaining: 25_000_000,
                duration_ms: 300,
            },
        ];

        // 50 MB threshold (52428800 bytes) -> 25 MB remaining should converge
        assert!(evaluate_pre_copy_convergence(&rounds, 50 * 1024 * 1024));
        // 10 MB threshold -> 25 MB remaining should NOT converge yet
        assert!(!evaluate_pre_copy_convergence(&rounds, 10 * 1024 * 1024));
    }

    #[test]
    fn test_checkpoint_manifest_generation() {
        let dir = tempdir().unwrap();
        let dummy_file = dir.path().join("server.properties");
        fs::write(&dummy_file, b"motd=Craft Live Migration\nserver-port=25565\n").unwrap();

        let manifest = build_server_checkpoint_manifest(
            dir.path(),
            "mig-test-01",
            "survival",
            12345,
        ).unwrap();

        assert_eq!(manifest.migration_id, "mig-test-01");
        assert_eq!(manifest.server_name, "survival");
        assert_eq!(manifest.game_tick, 12345);
        assert!(manifest.memory_bytes > 0);
        assert_ne!(manifest.total_checksum, 0);
        assert_ne!(manifest.inventory_fencing_token, 0);
    }

    #[test]
    fn test_migration_registry_persistence() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let mut reg = MigrationRegistry::load(&paths).unwrap();
        assert_eq!(reg.list_plans().len(), 0);

        let mut plan = LiveMigrationPlan::new(
            "lobby",
            "node-us-east",
            "node-eu-west",
            "192.168.1.50",
            25565,
        );
        plan.status = MigrationStage::FreezeAndHandoff;
        reg.add_plan(plan.clone()).unwrap();

        let route = AnycastRouteAnnouncement::new("198.51.100.0/24", 65001);
        reg.add_route(route);

        reg.save(&paths).unwrap();

        // Reload and verify
        let loaded = MigrationRegistry::load(&paths).unwrap();
        assert_eq!(loaded.list_plans().len(), 1);
        let loaded_plan = loaded.get_plan(&plan.migration_id).unwrap();
        assert_eq!(loaded_plan.server_name, "lobby");
        assert_eq!(loaded_plan.status, MigrationStage::FreezeAndHandoff);

        assert_eq!(loaded.list_routes().len(), 1);
        assert_eq!(loaded.find_route("198.51.100.0/24").unwrap().asn, 65001);
    }
}
