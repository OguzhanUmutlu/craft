use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;

/// Pure-Rust Raft node role state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaftRole {
    Leader,
    Candidate,
    Follower,
}

impl RaftRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            RaftRole::Leader => "Leader",
            RaftRole::Candidate => "Candidate",
            RaftRole::Follower => "Follower",
        }
    }
}

impl fmt::Display for RaftRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for RaftRole {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "leader" => Ok(RaftRole::Leader),
            "candidate" => Ok(RaftRole::Candidate),
            "follower" => Ok(RaftRole::Follower),
            _ => Err(format!(
                "Invalid Raft role '{}'. Expected 'leader', 'candidate', or 'follower'.",
                s
            )),
        }
    }
}

/// Online joint consensus membership reconfiguration phase
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", content = "members")]
pub enum JointConsensusPhase {
    /// Normal single-configuration operation
    None,
    /// Transitional joint consensus state requiring majority in both C_old and C_new
    Joint {
        c_old: Vec<String>,
        c_new: Vec<String>,
    },
    /// C_new committed, preparing finalization
    Committed {
        c_new: Vec<String>,
    },
    /// Joint consensus fully finalized to standard configuration
    Finalized,
}

/// Action to perform on cluster membership during reconfiguration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipChangeType {
    AddNode,
    RemoveNode,
    PromoteLearner,
    DemoteToLearner,
}

/// Tracking learner non-voting synchronization progress before promotion
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnerSyncProgress {
    pub node_id: String,
    pub match_index: u64,
    pub leader_last_index: u64,
    pub is_caught_up: bool,
    pub sync_percentage: f64,
}

impl LearnerSyncProgress {
    pub fn calculate(node_id: String, match_index: u64, leader_last_index: u64) -> Self {
        let is_caught_up = match_index >= leader_last_index;
        let sync_percentage = if leader_last_index == 0 {
            100.0
        } else {
            ((match_index as f64) / (leader_last_index as f64) * 100.0).min(100.0)
        };
        Self {
            node_id,
            match_index,
            leader_last_index,
            is_caught_up,
            sync_percentage,
        }
    }
}

/// Dynamic payload applied through replicated consensus log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RaftPayload {
    /// Update a global cluster configuration key-value
    ClusterConfigUpdate { key: String, value: String },
    /// Register a new server instance into the consensus catalog
    ServerRegistration {
        name: String,
        software: String,
        port: u16,
    },
    /// Deregister a server instance from consensus catalog
    ServerDeregistration { name: String },
    /// Acquire a linearizable distributed lock with fencing token
    DistributedLockAcquire {
        lock_name: String,
        holder_id: String,
        lease_secs: u64,
    },
    /// Release an existing distributed lock
    DistributedLockRelease {
        lock_name: String,
        holder_id: String,
    },
    /// Reconfigure cluster membership through joint consensus
    ConfigurationChange {
        change_type: MembershipChangeType,
        node: RaftNode,
        phase: JointConsensusPhase,
    },
    /// No-op entry used for commit boundary commitment after leadership election
    NoOp,
    /// Arbitrary application command string
    Custom { action: String, data: String },
}

/// Replicated append-only Write-Ahead Log (WAL) entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftLogEntry {
    pub index: u64,
    pub term: u64,
    pub payload: RaftPayload,
    pub timestamp: i64,
    pub client_id: Option<String>,
}

/// Compact snapshot of cluster state at a given log index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaftSnapshot {
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub cluster_state_json: String,
    pub active_locks: HashMap<String, DistributedLock>,
    pub created_at: i64,
    #[serde(default)]
    pub group_id: u64,
    #[serde(default)]
    pub membership: Vec<RaftNode>,
}

/// Cluster member participating in consensus or replication
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftNode {
    pub id: String,
    pub address: String,
    pub raft_port: u16,
    pub voting_member: bool,
    pub priority: u32,
}

impl RaftNode {
    pub fn is_learner(&self) -> bool {
        !self.voting_member
    }
}

/// Monotonically ordered distributed lock with fencing token
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributedLock {
    pub name: String,
    pub holder_id: String,
    pub fencing_token: u64,
    pub acquired_at: i64,
    pub lease_expires_at: i64,
}

impl DistributedLock {
    pub fn is_expired(&self, current_time: i64) -> bool {
        current_time >= self.lease_expires_at
    }
}

/// Calculates a 64-bit monotonically increasing fencing token from Raft term and log commit index
pub fn calculate_fencing_token(term: u64, index: u64) -> u64 {
    (term << 32) | (index & 0xFFFF_FFFF)
}

/// Decomposes a 64-bit fencing token back into (term, commit_index)
pub fn fencing_token_parts(token: u64) -> (u64, u64) {
    let term = token >> 32;
    let index = token & 0xFFFF_FFFF;
    (term, index)
}

/// Node health and latency weight score for dynamic split-brain tie-breaking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbitrationWeight {
    pub node_id: String,
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub uptime_secs: u64,
    pub weight_score: f64,
}

impl ArbitrationWeight {
    /// Compute dynamic weight score from latency, jitter, and continuous uptime
    pub fn calculate(node_id: String, latency_ms: f64, jitter_ms: f64, uptime_secs: u64) -> Self {
        let uptime_factor = (uptime_secs as f64).min(86400.0) / 86400.0 * 30.0;
        let latency_factor = (100.0 / (latency_ms + 1.0)).min(40.0);
        let jitter_factor = (50.0 / (jitter_ms + 1.0)).min(30.0);
        let weight_score = (uptime_factor + latency_factor + jitter_factor).max(0.1);

        Self {
            node_id,
            latency_ms,
            jitter_ms,
            uptime_secs,
            weight_score,
        }
    }
}

/// Dynamic quorum status evaluation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuorumStatus {
    pub total_members: usize,
    pub healthy_members: usize,
    pub majority_threshold: usize,
    pub is_quorum_intact: bool,
    pub arbitrated_leader: Option<String>,
}

/// Persistent Raft node state saved to disk across reboots
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistentRaftState {
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub cluster_nodes: Vec<RaftNode>,
    pub active_locks: HashMap<String, DistributedLock>,
    #[serde(default)]
    pub group_id: u64,
    #[serde(default)]
    pub joint_consensus: Option<JointConsensusPhase>,
}

impl Default for PersistentRaftState {
    fn default() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied: 0,
            cluster_nodes: Vec::new(),
            active_locks: HashMap::new(),
            group_id: 0,
            joint_consensus: None,
        }
    }
}

/// Thread-safe and inter-process safe Raft state registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftRegistry {
    pub state: PersistentRaftState,
}

impl RaftRegistry {
    pub fn new() -> Self {
        Self {
            state: PersistentRaftState::default(),
        }
    }

    /// Load persistent Raft state from disk, or return default empty state
    pub fn load_or_default(paths: &CraftPaths) -> Result<Self> {
        let file_path = &paths.raft_state_file;
        if !file_path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(file_path)?;
        let state: PersistentRaftState = toml::from_str(&content)?;

        Ok(Self { state })
    }

    /// Save persistent Raft state to disk
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.raft_state_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&self.state)?;
        fs::write(&paths.raft_state_file, content)?;

        Ok(())
    }

    /// Execute a mutating closure while holding an advisory file lock on `raft.lock`
    pub fn with_lock<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut RaftRegistry) -> Result<R>,
    {
        if let Some(parent) = paths.raft_lock.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.raft_lock)?;

        lock_file.lock_exclusive().map_err(|e| {
            CraftError::Other(format!(
                "Failed to acquire exclusive lock on {}: {}",
                paths.raft_lock.display(),
                e
            ))
        })?;

        let mut registry = Self::load_or_default(paths)?;
        let result = f(&mut registry);

        if result.is_ok() {
            if let Err(save_err) = registry.save(paths) {
                let _ = lock_file.unlock();
                return Err(save_err);
            }
        }

        let _ = lock_file.unlock();
        result
    }

    /// Add or update a cluster node
    pub fn add_node(&mut self, node: RaftNode) -> Result<()> {
        if let Some(pos) = self.state.cluster_nodes.iter().position(|n| n.id == node.id) {
            self.state.cluster_nodes[pos] = node;
        } else {
            self.state.cluster_nodes.push(node);
        }
        Ok(())
    }

    /// Remove a cluster node by ID
    pub fn remove_node(&mut self, node_id: &str) -> Result<()> {
        let initial_len = self.state.cluster_nodes.len();
        self.state.cluster_nodes.retain(|n| n.id != node_id);
        if self.state.cluster_nodes.len() == initial_len {
            return Err(CraftError::Other(format!(
                "Raft cluster node '{}' not found",
                node_id
            )));
        }
        Ok(())
    }

    /// Retrieve node metadata by ID
    pub fn get_node(&self, node_id: &str) -> Option<&RaftNode> {
        self.state.cluster_nodes.iter().find(|n| n.id == node_id)
    }

    /// Acquire a distributed lock with monotonic fencing token
    pub fn acquire_lock(
        &mut self,
        lock_name: &str,
        holder_id: &str,
        lease_secs: u64,
        current_time: i64,
        term: u64,
        index: u64,
    ) -> Result<DistributedLock> {
        if let Some(existing) = self.state.active_locks.get(lock_name) {
            if !existing.is_expired(current_time) && existing.holder_id != holder_id {
                return Err(CraftError::Other(format!(
                    "Lock '{}' is already held by node '{}' until timestamp {} (fencing token: {})",
                    lock_name, existing.holder_id, existing.lease_expires_at, existing.fencing_token
                )));
            }
        }

        let fencing_token = calculate_fencing_token(term, index);
        let lock = DistributedLock {
            name: lock_name.to_string(),
            holder_id: holder_id.to_string(),
            fencing_token,
            acquired_at: current_time,
            lease_expires_at: current_time + (lease_secs as i64),
        };

        self.state
            .active_locks
            .insert(lock_name.to_string(), lock.clone());
        Ok(lock)
    }

    /// Release an existing distributed lock if held by caller
    pub fn release_lock(&mut self, lock_name: &str, holder_id: &str) -> Result<()> {
        match self.state.active_locks.get(lock_name) {
            Some(existing) => {
                if existing.holder_id != holder_id {
                    return Err(CraftError::Other(format!(
                        "Lock '{}' is held by '{}', not '{}'",
                        lock_name, existing.holder_id, holder_id
                    )));
                }
                self.state.active_locks.remove(lock_name);
                Ok(())
            }
            None => Err(CraftError::Other(format!(
                "Lock '{}' does not exist or has expired",
                lock_name
            ))),
        }
    }

    /// Prune expired distributed locks
    pub fn prune_expired_locks(&mut self, current_time: i64) -> usize {
        let initial_count = self.state.active_locks.len();
        self.state
            .active_locks
            .retain(|_, lock| !lock.is_expired(current_time));
        initial_count.saturating_sub(self.state.active_locks.len())
    }

    /// Access active locks map
    pub fn get_active_locks(&self) -> &HashMap<String, DistributedLock> {
        &self.state.active_locks
    }
}

impl Default for RaftRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Well-known Multi-Raft group identifiers
pub const GROUP_CONTROL_PLANE: u64 = 0;
pub const GROUP_DISTRIBUTED_LOCKS: u64 = 1;
pub const GROUP_WORLD_BASE: u64 = 100;

/// Multi-Raft partition specification for horizontally partitioned state machines
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiRaftPartition {
    pub group_id: u64,
    pub name: String,
    pub key_range_start: String,
    pub key_range_end: String,
    pub leader_node_id: Option<String>,
    pub replica_nodes: Vec<String>,
    pub term: u64,
    pub commit_index: u64,
    pub active: bool,
}

/// Routing decision mapping an application key to a Multi-Raft group
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionRoutingKey {
    pub key: String,
    pub target_group_id: u64,
    pub matched_partition: String,
}

/// Multi-Raft partition catalog saved to disk and protected by advisory lock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiRaftRegistry {
    pub partitions: Vec<MultiRaftPartition>,
    pub default_group_id: u64,
    pub updated_at: i64,
}

impl MultiRaftRegistry {
    pub fn new() -> Self {
        Self {
            partitions: vec![
                MultiRaftPartition {
                    group_id: GROUP_CONTROL_PLANE,
                    name: "control-plane".to_string(),
                    key_range_start: "".to_string(),
                    key_range_end: "craft:lock:".to_string(),
                    leader_node_id: None,
                    replica_nodes: Vec::new(),
                    term: 0,
                    commit_index: 0,
                    active: true,
                },
                MultiRaftPartition {
                    group_id: GROUP_DISTRIBUTED_LOCKS,
                    name: "distributed-locks".to_string(),
                    key_range_start: "craft:lock:".to_string(),
                    key_range_end: "craft:world:".to_string(),
                    leader_node_id: None,
                    replica_nodes: Vec::new(),
                    term: 0,
                    commit_index: 0,
                    active: true,
                },
            ],
            default_group_id: GROUP_CONTROL_PLANE,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn load_or_default(paths: &CraftPaths) -> Result<Self> {
        let file_path = &paths.multiraft_file;
        if !file_path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(file_path)?;
        let registry: MultiRaftRegistry = toml::from_str(&content)?;
        Ok(registry)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.multiraft_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&paths.multiraft_file, content)?;
        Ok(())
    }

    pub fn with_lock<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut MultiRaftRegistry) -> Result<R>,
    {
        if let Some(parent) = paths.multiraft_lock.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&paths.multiraft_lock)?;

        lock_file.lock_exclusive().map_err(|e| {
            CraftError::Other(format!(
                "Failed to acquire exclusive lock on {}: {}",
                paths.multiraft_lock.display(),
                e
            ))
        })?;

        let mut registry = Self::load_or_default(paths)?;
        let result = f(&mut registry);

        if result.is_ok() {
            if let Err(save_err) = registry.save(paths) {
                let _ = lock_file.unlock();
                return Err(save_err);
            }
        }

        let _ = lock_file.unlock();
        result
    }

    pub fn register_partition(&mut self, partition: MultiRaftPartition) -> Result<()> {
        if let Some(pos) = self.partitions.iter().position(|p| p.group_id == partition.group_id) {
            self.partitions[pos] = partition;
        } else {
            self.partitions.push(partition);
        }
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn remove_partition(&mut self, group_id: u64) -> Result<()> {
        if group_id == GROUP_CONTROL_PLANE {
            return Err(CraftError::Other("Cannot remove root control plane partition (group 0)".to_string()));
        }
        let initial_len = self.partitions.len();
        self.partitions.retain(|p| p.group_id != group_id);
        if self.partitions.len() == initial_len {
            return Err(CraftError::Other(format!("Partition group {} not found", group_id)));
        }
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn get_partition(&self, group_id: u64) -> Option<&MultiRaftPartition> {
        self.partitions.iter().find(|p| p.group_id == group_id)
    }

    pub fn route_key(&self, key: &str) -> PartitionRoutingKey {
        for part in &self.partitions {
            if !part.active {
                continue;
            }
            if !part.key_range_start.is_empty() {
                let in_start = key >= part.key_range_start.as_str();
                let in_end = part.key_range_end.is_empty() || key < part.key_range_end.as_str();
                if in_start && in_end {
                    return PartitionRoutingKey {
                        key: key.to_string(),
                        target_group_id: part.group_id,
                        matched_partition: part.name.clone(),
                    };
                }
            }
        }
        PartitionRoutingKey {
            key: key.to_string(),
            target_group_id: self.default_group_id,
            matched_partition: "control-plane".to_string(),
        }
    }

    pub fn update_partition_leader(
        &mut self,
        group_id: u64,
        leader_id: Option<String>,
        term: u64,
        commit_index: u64,
    ) -> Result<()> {
        if let Some(part) = self.partitions.iter_mut().find(|p| p.group_id == group_id) {
            part.leader_node_id = leader_id;
            part.term = term;
            part.commit_index = commit_index;
            self.updated_at = chrono::Utc::now().timestamp();
            Ok(())
        } else {
            Err(CraftError::Other(format!("Partition group {} not found", group_id)))
        }
    }
}

impl Default for MultiRaftRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute IEEE 802.3 CRC-32 checksum for snapshot data verification
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Metadata header describing a streaming Raft snapshot
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaftSnapshotMeta {
    pub snapshot_id: String,
    pub group_id: u64,
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub membership: Vec<RaftNode>,
    pub total_bytes: u64,
    pub total_chunks: u32,
    pub crc32: u32,
    pub created_at: i64,
}

/// Individual chunk of a streaming Raft snapshot for network transmission
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChunk {
    pub snapshot_id: String,
    pub group_id: u64,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
    pub crc32: u32,
    pub is_last: bool,
}

impl SnapshotChunk {
    pub fn new(
        snapshot_id: String,
        group_id: u64,
        chunk_index: u32,
        total_chunks: u32,
        data: Vec<u8>,
        is_last: bool,
    ) -> Self {
        let crc32 = compute_crc32(&data);
        Self {
            snapshot_id,
            group_id,
            chunk_index,
            total_chunks,
            data,
            crc32,
            is_last,
        }
    }

    pub fn verify_checksum(&self) -> bool {
        compute_crc32(&self.data) == self.crc32
    }
}

/// Chunks binary snapshot data into chunk_size byte fragments
pub fn chunk_snapshot_data(
    snapshot_id: &str,
    group_id: u64,
    data: &[u8],
    chunk_size: usize,
) -> Vec<SnapshotChunk> {
    if data.is_empty() {
        return vec![SnapshotChunk::new(
            snapshot_id.to_string(),
            group_id,
            0,
            1,
            Vec::new(),
            true,
        )];
    }
    let total_chunks = ((data.len() + chunk_size - 1) / chunk_size) as u32;
    let mut chunks = Vec::with_capacity(total_chunks as usize);
    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        let is_last = (i as u32) == total_chunks - 1;
        chunks.push(SnapshotChunk::new(
            snapshot_id.to_string(),
            group_id,
            i as u32,
            total_chunks,
            chunk.to_vec(),
            is_last,
        ));
    }
    chunks
}

/// Reassembles validated snapshot chunks back into contiguous bytes
pub fn reassemble_snapshot_chunks(mut chunks: Vec<SnapshotChunk>) -> Result<Vec<u8>> {
    chunks.sort_by_key(|c| c.chunk_index);
    let mut buffer = Vec::new();
    for (expected_idx, chunk) in chunks.iter().enumerate() {
        if chunk.chunk_index != expected_idx as u32 {
            return Err(CraftError::Other(format!(
                "Missing chunk index {}: found chunk {}",
                expected_idx, chunk.chunk_index
            )));
        }
        if !chunk.verify_checksum() {
            return Err(CraftError::Other(format!(
                "Checksum mismatch on snapshot chunk {}",
                chunk.chunk_index
            )));
        }
        buffer.extend_from_slice(&chunk.data);
    }
    Ok(buffer)
}

/// WAL Compaction policy tuning snapshot trigger criteria
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalCompactionPolicy {
    pub max_retained_entries: u64,
    pub min_compaction_interval_secs: u64,
    pub auto_compact_enabled: bool,
    pub snapshot_threshold_bytes: u64,
}

impl Default for WalCompactionPolicy {
    fn default() -> Self {
        Self {
            max_retained_entries: 5000,
            min_compaction_interval_secs: 60,
            auto_compact_enabled: true,
            snapshot_threshold_bytes: 10 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_raft_role_display_and_parse() {
        assert_eq!(RaftRole::Leader.as_str(), "Leader");
        assert_eq!(RaftRole::Candidate.as_str(), "Candidate");
        assert_eq!(RaftRole::Follower.as_str(), "Follower");

        assert_eq!("leader".parse::<RaftRole>().unwrap(), RaftRole::Leader);
        assert_eq!(
            "candidate".parse::<RaftRole>().unwrap(),
            RaftRole::Candidate
        );
        assert_eq!("follower".parse::<RaftRole>().unwrap(), RaftRole::Follower);
        assert!("invalid".parse::<RaftRole>().is_err());
    }

    #[test]
    fn test_fencing_token_monotonicity() {
        let token1 = calculate_fencing_token(1, 100);
        let token2 = calculate_fencing_token(1, 101);
        let token3 = calculate_fencing_token(2, 1);

        assert!(token1 < token2);
        assert!(token2 < token3);

        let (term, index) = fencing_token_parts(token1);
        assert_eq!(term, 1);
        assert_eq!(index, 100);

        let (term3, index3) = fencing_token_parts(token3);
        assert_eq!(term3, 2);
        assert_eq!(index3, 1);
    }

    #[test]
    fn test_distributed_lock_acquisition_and_expiry() {
        let mut registry = RaftRegistry::new();
        let now = 1_000_000;

        let lock = registry
            .acquire_lock("world-write-lock", "node-1", 60, now, 2, 45)
            .expect("should acquire lock");
        assert_eq!(lock.holder_id, "node-1");
        assert_eq!(lock.fencing_token, calculate_fencing_token(2, 45));
        assert!(!lock.is_expired(now + 30));
        assert!(lock.is_expired(now + 60));

        // Another node cannot acquire non-expired lock
        let conflict = registry.acquire_lock("world-write-lock", "node-2", 60, now + 10, 2, 46);
        assert!(conflict.is_err());

        // Same node can renew
        let renewed = registry.acquire_lock("world-write-lock", "node-1", 120, now + 10, 2, 47);
        assert!(renewed.is_ok());

        // Once expired, another node can acquire
        let after_expiry =
            registry.acquire_lock("world-write-lock", "node-2", 60, now + 200, 2, 48);
        assert!(after_expiry.is_ok());

        // Release lock
        assert!(registry.release_lock("world-write-lock", "node-1").is_err()); // wrong holder
        assert!(registry.release_lock("world-write-lock", "node-2").is_ok());
    }

    #[test]
    fn test_arbitration_weight_calculation() {
        let w1 = ArbitrationWeight::calculate("node-1".to_string(), 1.5, 0.2, 86400);
        let w2 = ArbitrationWeight::calculate("node-2".to_string(), 150.0, 25.0, 100);

        assert!(
            w1.weight_score > w2.weight_score,
            "Low latency, high uptime node should have higher weight score"
        );
    }

    #[test]
    fn test_raft_registry_save_and_load_roundtrip() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut reg = RaftRegistry::new();
        reg.state.current_term = 3;
        reg.state.voted_for = Some("node-1".to_string());
        reg.add_node(RaftNode {
            id: "node-1".to_string(),
            address: "10.0.0.1".to_string(),
            raft_port: 9876,
            voting_member: true,
            priority: 10,
        })
        .unwrap();

        reg.save(&paths).unwrap();

        let loaded = RaftRegistry::load_or_default(&paths).unwrap();
        assert_eq!(loaded.state.current_term, 3);
        assert_eq!(loaded.state.voted_for.as_deref(), Some("node-1"));
        assert_eq!(loaded.state.cluster_nodes.len(), 1);
        assert_eq!(loaded.state.cluster_nodes[0].id, "node-1");
    }

    #[test]
    fn test_compute_crc32_standard() {
        // Standard IEEE 802.3 CRC-32 test vector for "123456789"
        let data = b"123456789";
        let crc = compute_crc32(data);
        assert_eq!(crc, 0xCBF43926);
    }

    #[test]
    fn test_learner_sync_progress() {
        let p1 = LearnerSyncProgress::calculate("node-learner".to_string(), 50, 100);
        assert_eq!(p1.node_id, "node-learner");
        assert_eq!(p1.match_index, 50);
        assert_eq!(p1.leader_last_index, 100);
        assert!(!p1.is_caught_up);
        assert!((p1.sync_percentage - 50.0).abs() < f64::EPSILON);

        let p2 = LearnerSyncProgress::calculate("node-learner".to_string(), 100, 100);
        assert!(p2.is_caught_up);
        assert!((p2.sync_percentage - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multiraft_registry_routing_and_crud() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut reg = MultiRaftRegistry::new();
        assert_eq!(reg.partitions.len(), 2);

        // Test default routing
        let r1 = reg.route_key("arbitrary:config:key");
        assert_eq!(r1.target_group_id, GROUP_CONTROL_PLANE);

        let r2 = reg.route_key("craft:lock:world_edit");
        assert_eq!(r2.target_group_id, GROUP_DISTRIBUTED_LOCKS);

        // Register custom world partition
        let world_part = MultiRaftPartition {
            group_id: 100,
            name: "world-nether".to_string(),
            key_range_start: "craft:world:nether:".to_string(),
            key_range_end: "craft:world:nether:~".to_string(),
            leader_node_id: Some("node-1".to_string()),
            replica_nodes: vec!["node-1".to_string(), "node-2".to_string()],
            term: 2,
            commit_index: 85,
            active: true,
        };
        reg.register_partition(world_part).unwrap();

        let r3 = reg.route_key("craft:world:nether:chunk_12_34");
        assert_eq!(r3.target_group_id, 100);
        assert_eq!(r3.matched_partition, "world-nether");

        // Save and reload
        reg.save(&paths).unwrap();
        let loaded = MultiRaftRegistry::load_or_default(&paths).unwrap();
        assert_eq!(loaded.partitions.len(), 3);
        assert!(loaded.get_partition(100).is_some());

        // Update leader
        let mut reg2 = loaded;
        reg2.update_partition_leader(100, Some("node-2".to_string()), 3, 90).unwrap();
        assert_eq!(reg2.get_partition(100).unwrap().leader_node_id.as_deref(), Some("node-2"));

        // Remove partition
        assert!(reg2.remove_partition(GROUP_CONTROL_PLANE).is_err()); // Cannot remove control plane
        assert!(reg2.remove_partition(100).is_ok());
        assert!(reg2.get_partition(100).is_none());
    }

    #[test]
    fn test_snapshot_chunking_and_reassembly() {
        let data = b"Craft Multi-Raft Log Compaction Snapshot Payload with multiple chunks for streaming RPC verification! Repeat: Craft Multi-Raft Log Compaction Snapshot Payload with multiple chunks!";
        let chunks = chunk_snapshot_data("snap-101", 100, data, 32);
        assert!(chunks.len() > 1);

        for chunk in &chunks {
            assert!(chunk.verify_checksum());
            assert_eq!(chunk.group_id, 100);
        }

        let reassembled = reassemble_snapshot_chunks(chunks).expect("should reassemble cleanly");
        assert_eq!(&reassembled[..], &data[..]);
    }

    #[test]
    fn test_configuration_change_payload_serialization() {
        let node = RaftNode {
            id: "node-3".to_string(),
            address: "10.0.0.3".to_string(),
            raft_port: 9876,
            voting_member: false,
            priority: 5,
        };
        assert!(node.is_learner());

        let payload = RaftPayload::ConfigurationChange {
            change_type: MembershipChangeType::AddNode,
            node: node.clone(),
            phase: JointConsensusPhase::Joint {
                c_old: vec!["node-1".to_string(), "node-2".to_string()],
                c_new: vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()],
            },
        };

        let json = serde_json::to_string(&payload).expect("should serialize");
        let deserialized: RaftPayload = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(payload, deserialized);
    }
}
