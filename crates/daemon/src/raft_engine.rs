use craft_core::{
    calculate_fencing_token, DistributedLock, JointConsensusPhase, LearnerSyncProgress,
    MembershipChangeType, RaftLogEntry, RaftNode, RaftPayload, RaftRole, RaftSnapshot,
    WalCompactionPolicy,
};
use craft_net::{
    AppendEntriesArgs, AppendEntriesReply, HeartbeatArgs, HeartbeatReply, RequestVoteArgs,
    RequestVoteReply, SplitBrainArbitrator,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Status summary returned to clients and IPC callers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaftStatusSummary {
    pub node_id: String,
    pub role: RaftRole,
    pub current_term: u64,
    pub leader_id: Option<String>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub log_entries_count: usize,
    pub active_locks_count: usize,
    pub cluster_nodes: Vec<RaftNode>,
    pub is_quorum_intact: bool,
    pub edge_tie_breaker: Option<String>,
    #[serde(default)]
    pub group_id: u64,
    #[serde(default)]
    pub joint_consensus: Option<JointConsensusPhase>,
}

/// Linearizable distributed lock manager with monotonic fencing tokens
#[derive(Debug, Clone, Default)]
pub struct DistributedLockManager {
    locks: HashMap<String, DistributedLock>,
}

impl DistributedLockManager {
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
        }
    }

    pub fn acquire(
        &mut self,
        lock_name: &str,
        holder_id: &str,
        lease_secs: u64,
        now: i64,
        term: u64,
        index: u64,
    ) -> Result<DistributedLock, String> {
        if let Some(existing) = self.locks.get(lock_name) {
            if !existing.is_expired(now) && existing.holder_id != holder_id {
                return Err(format!(
                    "Lock '{}' is already held by node '{}' (token: {}) until {}",
                    lock_name, existing.holder_id, existing.fencing_token, existing.lease_expires_at
                ));
            }
        }

        let fencing_token = calculate_fencing_token(term, index);
        let lock = DistributedLock {
            name: lock_name.to_string(),
            holder_id: holder_id.to_string(),
            fencing_token,
            acquired_at: now,
            lease_expires_at: now + (lease_secs as i64),
        };

        self.locks.insert(lock_name.to_string(), lock.clone());
        Ok(lock)
    }

    pub fn release(&mut self, lock_name: &str, holder_id: &str) -> Result<(), String> {
        match self.locks.get(lock_name) {
            Some(existing) => {
                if existing.holder_id != holder_id {
                    return Err(format!(
                        "Cannot release lock '{}': held by '{}', caller is '{}'",
                        lock_name, existing.holder_id, holder_id
                    ));
                }
                self.locks.remove(lock_name);
                Ok(())
            }
            None => Err(format!("Lock '{}' does not exist", lock_name)),
        }
    }

    pub fn prune_expired(&mut self, now: i64) -> usize {
        let initial = self.locks.len();
        self.locks.retain(|_, lock| !lock.is_expired(now));
        initial.saturating_sub(self.locks.len())
    }

    pub fn get_locks(&self) -> &HashMap<String, DistributedLock> {
        &self.locks
    }
}

/// Pure-Rust in-memory Raft state machine with append-only WAL persistence
#[derive(Debug)]
pub struct RaftEngine {
    pub node_id: String,
    pub role: RaftRole,
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub log: Vec<RaftLogEntry>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub leader_id: Option<String>,
    pub wal_path: PathBuf,
    pub snapshots_dir: PathBuf,
    pub cluster_nodes: Vec<RaftNode>,
    pub lock_manager: DistributedLockManager,
    pub edge_tie_breaker: Option<String>,
    pub votes_received: HashSet<String>,
    pub last_activity_time: i64,
    pub group_id: u64,
    pub joint_consensus: Option<JointConsensusPhase>,
    pub learner_progress: HashMap<String, LearnerSyncProgress>,
}

impl RaftEngine {
    /// Initialize a new RaftEngine instance with default group 0
    pub fn new(
        node_id: String,
        wal_path: PathBuf,
        snapshots_dir: PathBuf,
        cluster_nodes: Vec<RaftNode>,
    ) -> Self {
        Self::new_with_group(node_id, 0, wal_path, snapshots_dir, cluster_nodes)
    }

    /// Initialize a new RaftEngine instance for a specific Multi-Raft group
    pub fn new_with_group(
        node_id: String,
        group_id: u64,
        wal_path: PathBuf,
        snapshots_dir: PathBuf,
        cluster_nodes: Vec<RaftNode>,
    ) -> Self {
        Self {
            node_id,
            role: RaftRole::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            leader_id: None,
            wal_path,
            snapshots_dir,
            cluster_nodes,
            lock_manager: DistributedLockManager::new(),
            edge_tie_breaker: None,
            votes_received: HashSet::new(),
            last_activity_time: chrono::Utc::now().timestamp(),
            group_id,
            joint_consensus: None,
            learner_progress: HashMap::new(),
        }
    }

    /// Load existing WAL log entries from disk
    pub fn load_from_wal(&mut self) -> Result<(), String> {
        if !self.wal_path.exists() {
            return Ok(());
        }

        let file = OpenOptions::new()
            .read(true)
            .open(&self.wal_path)
            .map_err(|e| format!("Failed to open WAL file {}: {}", self.wal_path.display(), e))?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line.map_err(|e| format!("Failed reading WAL line: {}", e))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: RaftLogEntry = serde_json::from_str(&line)
                .map_err(|e| format!("Corrupt entry in WAL: {}", e))?;

            if entry.term > self.current_term {
                self.current_term = entry.term;
            }
            self.apply_entry(&entry);
            self.commit_index = entry.index;
            self.last_applied = entry.index;
            self.log.push(entry);
        }

        Ok(())
    }

    /// Append an entry directly to the WAL file on disk
    fn persist_wal_entry(wal_path: &Path, entry: &RaftLogEntry) -> Result<(), String> {
        if let Some(parent) = wal_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create WAL dir {}: {}", parent.display(), e))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(wal_path)
            .map_err(|e| format!("Failed to open WAL for append: {}", e))?;

        let json = serde_json::to_string(entry)
            .map_err(|e| format!("Failed to serialize WAL entry: {}", e))?;

        writeln!(file, "{}", json).map_err(|e| format!("Failed writing WAL entry: {}", e))?;
        file.flush()
            .map_err(|e| format!("Failed flushing WAL entry: {}", e))?;

        Ok(())
    }

    /// Apply an entry to the local state machine (e.g. locks)
    fn apply_entry(&mut self, entry: &RaftLogEntry) {
        match &entry.payload {
            RaftPayload::DistributedLockAcquire {
                lock_name,
                holder_id,
                lease_secs,
            } => {
                let _ = self.lock_manager.acquire(
                    lock_name,
                    holder_id,
                    *lease_secs,
                    entry.timestamp,
                    entry.term,
                    entry.index,
                );
            }
            RaftPayload::DistributedLockRelease {
                lock_name,
                holder_id,
            } => {
                let _ = self.lock_manager.release(lock_name, holder_id);
            }
            RaftPayload::ConfigurationChange {
                change_type,
                node,
                phase,
            } => {
                self.joint_consensus = Some(phase.clone());
                match change_type {
                    MembershipChangeType::AddNode => {
                        if let Some(pos) = self.cluster_nodes.iter().position(|n| n.id == node.id) {
                            self.cluster_nodes[pos] = node.clone();
                        } else {
                            self.cluster_nodes.push(node.clone());
                        }
                    }
                    MembershipChangeType::RemoveNode => {
                        self.cluster_nodes.retain(|n| n.id != node.id);
                    }
                    MembershipChangeType::PromoteLearner => {
                        if let Some(n) = self.cluster_nodes.iter_mut().find(|n| n.id == node.id) {
                            n.voting_member = true;
                        }
                    }
                    MembershipChangeType::DemoteToLearner => {
                        if let Some(n) = self.cluster_nodes.iter_mut().find(|n| n.id == node.id) {
                            n.voting_member = false;
                        }
                    }
                }
                if *phase == JointConsensusPhase::Finalized {
                    self.joint_consensus = None;
                }
            }
            _ => {}
        }
    }

    /// Propose a new entry to the consensus log (Leader only)
    pub fn propose(
        &mut self,
        payload: RaftPayload,
        client_id: Option<String>,
    ) -> Result<u64, String> {
        if self.role != RaftRole::Leader {
            return Err(format!(
                "Cannot propose entry: node '{}' is {:?}, not Leader (current leader: {:?})",
                self.node_id, self.role, self.leader_id
            ));
        }

        let index = self.log.last().map(|e| e.index + 1).unwrap_or(1);
        let entry = RaftLogEntry {
            index,
            term: self.current_term,
            payload,
            timestamp: chrono::Utc::now().timestamp(),
            client_id,
        };

        Self::persist_wal_entry(&self.wal_path, &entry)?;
        self.apply_entry(&entry);
        self.log.push(entry);
        self.commit_index = index;
        self.last_applied = index;

        Ok(index)
    }

    /// Start a new election cycle, transitioning from Follower to Candidate
    pub fn start_election(&mut self) -> RequestVoteArgs {
        self.role = RaftRole::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.node_id.clone());
        self.leader_id = None;
        self.votes_received.clear();
        self.votes_received.insert(self.node_id.clone());
        self.last_activity_time = chrono::Utc::now().timestamp();

        let (last_log_index, last_log_term) = self.get_last_log_info();

        RequestVoteArgs {
            term: self.current_term,
            candidate_id: self.node_id.clone(),
            last_log_index,
            last_log_term,
        }
    }

    /// Handle incoming RequestVote RPC from another candidate
    pub fn handle_request_vote(&mut self, args: &RequestVoteArgs) -> RequestVoteReply {
        self.last_activity_time = chrono::Utc::now().timestamp();

        if args.term < self.current_term {
            return RequestVoteReply {
                term: self.current_term,
                vote_granted: false,
                reason: Some(format!(
                    "Stale term {} < current term {}",
                    args.term, self.current_term
                )),
            };
        }

        if args.term > self.current_term {
            self.current_term = args.term;
            self.role = RaftRole::Follower;
            self.voted_for = None;
            self.leader_id = None;
        }

        let can_vote = match &self.voted_for {
            None => true,
            Some(voted) => voted == &args.candidate_id,
        };

        if !can_vote {
            return RequestVoteReply {
                term: self.current_term,
                vote_granted: false,
                reason: Some(format!(
                    "Already voted for {:?} in term {}",
                    self.voted_for, self.current_term
                )),
            };
        }

        // Check log up-to-dateness
        let (my_last_index, my_last_term) = self.get_last_log_info();
        let log_ok = (args.last_log_term > my_last_term)
            || (args.last_log_term == my_last_term && args.last_log_index >= my_last_index);

        if !log_ok {
            return RequestVoteReply {
                term: self.current_term,
                vote_granted: false,
                reason: Some(format!(
                    "Candidate log (term={}, idx={}) is behind local log (term={}, idx={})",
                    args.last_log_term, args.last_log_index, my_last_term, my_last_index
                )),
            };
        }

        self.voted_for = Some(args.candidate_id.clone());
        RequestVoteReply {
            term: self.current_term,
            vote_granted: true,
            reason: None,
        }
    }

    /// Record a granted vote and transition to Leader if quorum reached
    pub fn record_vote(&mut self, from_node_id: String) -> bool {
        if self.role != RaftRole::Candidate {
            return false;
        }

        self.votes_received.insert(from_node_id);
        let responsive: Vec<String> = self.votes_received.iter().cloned().collect();

        // In joint consensus, candidate must achieve independent majorities in BOTH C_old and C_new
        if let Some(JointConsensusPhase::Joint { c_old, c_new }) = &self.joint_consensus {
            let old_nodes: Vec<RaftNode> = self
                .cluster_nodes
                .iter()
                .filter(|n| c_old.contains(&n.id))
                .cloned()
                .collect();
            let new_nodes: Vec<RaftNode> = self
                .cluster_nodes
                .iter()
                .filter(|n| c_new.contains(&n.id))
                .cloned()
                .collect();

            let status_old = SplitBrainArbitrator::evaluate_quorum(
                &old_nodes,
                &responsive,
                &[],
                self.edge_tie_breaker.as_deref(),
            );
            let status_new = SplitBrainArbitrator::evaluate_quorum(
                &new_nodes,
                &responsive,
                &[],
                self.edge_tie_breaker.as_deref(),
            );

            if status_old.is_quorum_intact && status_new.is_quorum_intact {
                self.role = RaftRole::Leader;
                self.leader_id = Some(self.node_id.clone());
                let _ = self.propose(RaftPayload::NoOp, None);
                return true;
            }

            return false;
        }

        let status = SplitBrainArbitrator::evaluate_quorum(
            &self.cluster_nodes,
            &responsive,
            &[],
            self.edge_tie_breaker.as_deref(),
        );

        if status.is_quorum_intact {
            self.role = RaftRole::Leader;
            self.leader_id = Some(self.node_id.clone());
            // Propose commit boundary No-Op entry
            let _ = self.propose(RaftPayload::NoOp, None);
            return true;
        }

        false
    }

    /// Handle incoming AppendEntries RPC from leader
    pub fn handle_append_entries(&mut self, args: &AppendEntriesArgs) -> AppendEntriesReply {
        self.last_activity_time = chrono::Utc::now().timestamp();

        if args.term < self.current_term {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
                match_index: self.commit_index,
                conflict_term: None,
                conflict_index: None,
            };
        }

        if args.term > self.current_term || self.role == RaftRole::Candidate {
            self.current_term = args.term;
            self.role = RaftRole::Follower;
            self.leader_id = Some(args.leader_id.clone());
        }

        self.leader_id = Some(args.leader_id.clone());

        // Check log consistency at prev_log_index
        if args.prev_log_index > 0 {
            let matching_entry = self.log.iter().find(|e| e.index == args.prev_log_index);
            match matching_entry {
                Some(entry) => {
                    if entry.term != args.prev_log_term {
                        return AppendEntriesReply {
                            term: self.current_term,
                            success: false,
                            match_index: self.commit_index,
                            conflict_term: Some(entry.term),
                            conflict_index: Some(entry.index),
                        };
                    }
                }
                None => {
                    return AppendEntriesReply {
                        term: self.current_term,
                        success: false,
                        match_index: self.commit_index,
                        conflict_term: None,
                        conflict_index: Some(self.log.last().map(|e| e.index).unwrap_or(0) + 1),
                    };
                }
            }
        }

        // Append new entries and truncate conflicts
        for entry in &args.entries {
            if let Some(pos) = self.log.iter().position(|e| e.index == entry.index) {
                if self.log[pos].term != entry.term {
                    self.log.truncate(pos);
                    let _ = Self::persist_wal_entry(&self.wal_path, entry);
                    self.apply_entry(entry);
                    self.log.push(entry.clone());
                }
            } else {
                let _ = Self::persist_wal_entry(&self.wal_path, entry);
                self.apply_entry(entry);
                self.log.push(entry.clone());
            }
        }

        // Update commit index
        let last_index = self.log.last().map(|e| e.index).unwrap_or(0);
        if args.leader_commit > self.commit_index {
            self.commit_index = args.leader_commit.min(last_index);
            self.last_applied = self.commit_index;
        }

        AppendEntriesReply {
            term: self.current_term,
            success: true,
            match_index: last_index,
            conflict_term: None,
            conflict_index: None,
        }
    }

    /// Handle fast Heartbeat frame from leader
    pub fn handle_heartbeat(&mut self, args: &HeartbeatArgs) -> HeartbeatReply {
        self.last_activity_time = chrono::Utc::now().timestamp();

        if args.term >= self.current_term {
            self.current_term = args.term;
            self.role = RaftRole::Follower;
            self.leader_id = Some(args.leader_id.clone());

            let last_index = self.log.last().map(|e| e.index).unwrap_or(0);
            if args.leader_commit > self.commit_index {
                self.commit_index = args.leader_commit.min(last_index);
                self.last_applied = self.commit_index;
            }

            HeartbeatReply {
                term: self.current_term,
                node_id: self.node_id.clone(),
                ack: true,
            }
        } else {
            HeartbeatReply {
                term: self.current_term,
                node_id: self.node_id.clone(),
                ack: false,
            }
        }
    }

    /// Voluntarily step down as leader to follower state
    pub fn step_down(&mut self, new_term: Option<u64>) {
        if let Some(t) = new_term {
            if t > self.current_term {
                self.current_term = t;
            }
        }
        self.role = RaftRole::Follower;
        self.leader_id = None;
        self.voted_for = None;
        self.votes_received.clear();
        self.last_activity_time = chrono::Utc::now().timestamp();
    }

    /// Transfer leadership to a designated peer node
    pub fn transfer_leadership(&mut self, target_node_id: &str) -> Result<(), String> {
        if self.role != RaftRole::Leader {
            return Err("Only leader can transfer leadership".to_string());
        }

        let target_exists = self
            .cluster_nodes
            .iter()
            .any(|n| n.id == target_node_id && n.voting_member);

        if !target_exists {
            return Err(format!(
                "Target node '{}' is not a voting member in cluster",
                target_node_id
            ));
        }

        self.step_down(None);
        self.leader_id = Some(target_node_id.to_string());
        Ok(())
    }

    /// Create a compact snapshot of current state and save to disk
    pub fn create_snapshot(&mut self) -> Result<RaftSnapshot, String> {
        let last_included_index = self.commit_index;
        let last_included_term = self
            .log
            .iter()
            .find(|e| e.index == last_included_index)
            .map(|e| e.term)
            .unwrap_or(self.current_term);

        let snapshot = RaftSnapshot {
            last_included_index,
            last_included_term,
            cluster_state_json: serde_json::to_string(&self.cluster_nodes)
                .map_err(|e| format!("Snapshot serialization failed: {}", e))?,
            active_locks: self.lock_manager.get_locks().clone(),
            created_at: chrono::Utc::now().timestamp(),
            group_id: self.group_id,
            membership: self.cluster_nodes.clone(),
        };

        if !self.snapshots_dir.exists() {
            fs::create_dir_all(&self.snapshots_dir).map_err(|e| {
                format!(
                    "Failed to create snapshot dir {}: {}",
                    self.snapshots_dir.display(),
                    e
                )
            })?;
        }

        let snap_path = self.snapshots_dir.join(format!(
            "snapshot_g{}_{}_{}.json",
            self.group_id, last_included_term, last_included_index
        ));

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;

        fs::write(&snap_path, json)
            .map_err(|e| format!("Failed to write snapshot {}: {}", snap_path.display(), e))?;

        Ok(snapshot)
    }

    /// Trigger WAL compaction according to compaction policy
    pub fn trigger_compaction(
        &mut self,
        policy: &WalCompactionPolicy,
        force: bool,
    ) -> Result<Option<(u64, u64, u64)>, String> {
        let entry_count = self.log.len() as u64;
        if !force && (!policy.auto_compact_enabled || entry_count <= policy.max_retained_entries) {
            return Ok(None);
        }

        let retain_entries = policy.max_retained_entries as usize;
        let entries_to_compact = if force {
            self.log.len().saturating_sub(1)
        } else {
            self.log.len().saturating_sub(retain_entries)
        };

        if entries_to_compact == 0 {
            return Ok(None);
        }

        let snapshot = self.create_snapshot()?;
        let last_included_index = snapshot.last_included_index;
        let json_bytes = serde_json::to_vec(&snapshot)
            .map_err(|e| format!("Failed to serialize snapshot for bytes count: {}", e))?;
        let snapshot_bytes = json_bytes.len() as u64;

        self.log.drain(0..entries_to_compact);

        // Rewrite WAL file with pruned entries
        if let Some(parent) = self.wal_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create WAL dir: {}", e))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.wal_path)
            .map_err(|e| format!("Failed to rewrite WAL file: {}", e))?;

        for entry in &self.log {
            let json = serde_json::to_string(entry)
                .map_err(|e| format!("Failed to serialize entry: {}", e))?;
            writeln!(file, "{}", json).map_err(|e| format!("Failed writing entry: {}", e))?;
        }
        file.flush().map_err(|e| format!("Failed flushing: {}", e))?;

        Ok(Some((last_included_index, entries_to_compact as u64, snapshot_bytes)))
    }

    /// Prune WAL entries if log exceeds max_entries
    pub fn prune_wal_if_needed(&mut self, keep_last: usize) -> Result<(), String> {
        if self.log.len() <= keep_last {
            return Ok(());
        }

        let split_at = self.log.len() - keep_last;
        let _ = self.create_snapshot()?;
        self.log.drain(0..split_at);

        // Rewrite WAL file with pruned entries
        if let Some(parent) = self.wal_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create WAL dir: {}", e))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.wal_path)
            .map_err(|e| format!("Failed to rewrite WAL file: {}", e))?;

        for entry in &self.log {
            let json = serde_json::to_string(entry)
                .map_err(|e| format!("Failed to serialize entry: {}", e))?;
            writeln!(file, "{}", json).map_err(|e| format!("Failed writing entry: {}", e))?;
        }
        file.flush().map_err(|e| format!("Failed flushing: {}", e))?;

        Ok(())
    }

    /// Track synchronization progress of a non-voting learner
    pub fn update_learner_progress(&mut self, node_id: String, match_index: u64) {
        let (last_idx, _) = self.get_last_log_info();
        let progress = LearnerSyncProgress::calculate(node_id.clone(), match_index, last_idx);
        self.learner_progress.insert(node_id, progress);
    }

    /// Retrieve summary status
    pub fn get_status(&self) -> RaftStatusSummary {
        let all_ids: Vec<String> = self.cluster_nodes.iter().map(|n| n.id.clone()).collect();
        let quorum = SplitBrainArbitrator::evaluate_quorum(
            &self.cluster_nodes,
            &all_ids,
            &[],
            self.edge_tie_breaker.as_deref(),
        );

        RaftStatusSummary {
            node_id: self.node_id.clone(),
            role: self.role,
            current_term: self.current_term,
            leader_id: self.leader_id.clone(),
            commit_index: self.commit_index,
            last_applied: self.last_applied,
            log_entries_count: self.log.len(),
            active_locks_count: self.lock_manager.get_locks().len(),
            cluster_nodes: self.cluster_nodes.clone(),
            is_quorum_intact: quorum.is_quorum_intact,
            edge_tie_breaker: self.edge_tie_breaker.clone(),
            group_id: self.group_id,
            joint_consensus: self.joint_consensus.clone(),
        }
    }

    /// Helper to get last log index and term
    pub fn get_last_log_info(&self) -> (u64, u64) {
        match self.log.last() {
            Some(entry) => (entry.index, entry.term),
            None => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_engine() -> (RaftEngine, TempDir) {
        let temp = TempDir::new().unwrap();
        let wal_path = temp.path().join("wal").join("raft.wal");
        let snapshots_dir = temp.path().join("snapshots");

        let nodes = vec![
            RaftNode {
                id: "node-1".into(),
                address: "127.0.0.1".into(),
                raft_port: 9001,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "node-2".into(),
                address: "127.0.0.1".into(),
                raft_port: 9002,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "node-3".into(),
                address: "127.0.0.1".into(),
                raft_port: 9003,
                voting_member: true,
                priority: 10,
            },
        ];

        let engine = RaftEngine::new("node-1".to_string(), wal_path, snapshots_dir, nodes);
        (engine, temp)
    }

    #[test]
    fn test_election_and_voting_cycle() {
        let (mut engine, _temp) = setup_engine();
        assert_eq!(engine.role, RaftRole::Follower);

        let vote_req = engine.start_election();
        assert_eq!(engine.role, RaftRole::Candidate);
        assert_eq!(engine.current_term, 1);
        assert_eq!(vote_req.candidate_id, "node-1");

        // Record vote from node-2 -> 2 out of 3 votes -> quorum achieved!
        let won = engine.record_vote("node-2".to_string());
        assert!(won);
        assert_eq!(engine.role, RaftRole::Leader);
        assert_eq!(engine.leader_id.as_deref(), Some("node-1"));
    }

    #[test]
    fn test_propose_and_wal_persistence() {
        let (mut engine, _temp) = setup_engine();
        engine.role = RaftRole::Leader;
        engine.current_term = 1;

        let idx = engine
            .propose(
                RaftPayload::ClusterConfigUpdate {
                    key: "motd".into(),
                    value: "Welcome to Craft".into(),
                },
                Some("cli".into()),
            )
            .expect("proposal should succeed");

        assert_eq!(idx, 1);
        assert_eq!(engine.log.len(), 1);
        assert_eq!(engine.commit_index, 1);

        // Verify WAL file can be reloaded
        let mut reloaded = RaftEngine::new(
            "node-1".to_string(),
            engine.wal_path.clone(),
            engine.snapshots_dir.clone(),
            engine.cluster_nodes.clone(),
        );
        reloaded.load_from_wal().expect("WAL reload should succeed");
        assert_eq!(reloaded.log.len(), 1);
        assert_eq!(reloaded.commit_index, 1);
    }

    #[test]
    fn test_distributed_lock_via_consensus() {
        let (mut engine, _temp) = setup_engine();
        engine.role = RaftRole::Leader;
        engine.current_term = 2;

        let acquire_payload = RaftPayload::DistributedLockAcquire {
            lock_name: "cluster-deploy".into(),
            holder_id: "node-1".into(),
            lease_secs: 60,
        };

        engine.propose(acquire_payload, None).unwrap();

        assert_eq!(engine.lock_manager.get_locks().len(), 1);
        let lock = engine.lock_manager.get_locks().get("cluster-deploy").unwrap();
        assert_eq!(lock.holder_id, "node-1");
        assert_eq!(lock.fencing_token, calculate_fencing_token(2, 1));
    }

    #[test]
    fn test_snapshot_creation() {
        let (mut engine, _temp) = setup_engine();
        engine.role = RaftRole::Leader;
        engine.current_term = 1;

        engine.propose(RaftPayload::NoOp, None).unwrap();
        let snap = engine.create_snapshot().expect("snapshot should succeed");

        assert_eq!(snap.last_included_index, 1);
        assert_eq!(snap.last_included_term, 1);
        assert_eq!(snap.group_id, 0);
    }

    #[test]
    fn test_trigger_compaction() {
        let (mut engine, _temp) = setup_engine();
        engine.role = RaftRole::Leader;
        engine.current_term = 1;

        for i in 0..10 {
            engine
                .propose(
                    RaftPayload::Custom {
                        action: "test".to_string(),
                        data: format!("val-{}", i),
                    },
                    None,
                )
                .unwrap();
        }

        assert_eq!(engine.log.len(), 10);

        let policy = WalCompactionPolicy {
            max_retained_entries: 3,
            min_compaction_interval_secs: 0,
            auto_compact_enabled: true,
            snapshot_threshold_bytes: 1024,
        };

        let result = engine.trigger_compaction(&policy, false).expect("compaction should succeed");
        assert!(result.is_some());
        let (last_idx, compacted, bytes) = result.unwrap();
        assert_eq!(last_idx, 10);
        assert_eq!(compacted, 7);
        assert!(bytes > 0);
        assert_eq!(engine.log.len(), 3);
    }

    #[test]
    fn test_joint_consensus_dual_majority() {
        let temp = TempDir::new().unwrap();
        let wal_path = temp.path().join("wal").join("raft.wal");
        let snapshots_dir = temp.path().join("snapshots");

        let nodes = vec![
            RaftNode { id: "n1".into(), address: "127.0.0.1".into(), raft_port: 9001, voting_member: true, priority: 10 },
            RaftNode { id: "n2".into(), address: "127.0.0.1".into(), raft_port: 9002, voting_member: true, priority: 10 },
            RaftNode { id: "n3".into(), address: "127.0.0.1".into(), raft_port: 9003, voting_member: true, priority: 10 },
            RaftNode { id: "n4".into(), address: "127.0.0.1".into(), raft_port: 9004, voting_member: true, priority: 10 },
            RaftNode { id: "n5".into(), address: "127.0.0.1".into(), raft_port: 9005, voting_member: true, priority: 10 },
        ];

        let mut engine = RaftEngine::new("n1".to_string(), wal_path, snapshots_dir, nodes);
        engine.role = RaftRole::Candidate;
        engine.current_term = 2;
        engine.votes_received.insert("n1".to_string());

        // Set joint consensus: C_old = [n1, n2, n3], C_new = [n1, n2, n3, n4, n5]
        engine.joint_consensus = Some(JointConsensusPhase::Joint {
            c_old: vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
            c_new: vec!["n1".to_string(), "n2".to_string(), "n3".to_string(), "n4".to_string(), "n5".to_string()],
        });

        // Vote from n2 -> 2 votes (n1, n2).
        // For C_old (size 3), majority is 2. Quorum in C_old is intact!
        // But for C_new (size 5), majority is 3. Quorum in C_new is NOT intact yet!
        let won = engine.record_vote("n2".to_string());
        assert!(!won);
        assert_eq!(engine.role, RaftRole::Candidate);

        // Vote from n4 -> 3 votes (n1, n2, n4).
        // C_old has n1, n2 (2 of 3) -> valid!
        // C_new has n1, n2, n4 (3 of 5) -> valid!
        // Both quorums satisfied -> candidate transitions to Leader!
        let won2 = engine.record_vote("n4".to_string());
        assert!(won2);
        assert_eq!(engine.role, RaftRole::Leader);
    }
}
