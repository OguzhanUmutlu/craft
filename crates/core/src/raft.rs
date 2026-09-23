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
}
