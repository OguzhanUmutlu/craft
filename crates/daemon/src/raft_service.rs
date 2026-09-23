use crate::raft_engine::{RaftEngine, RaftStatusSummary};
use craft_core::{
    CraftError, CraftPaths, DistributedLock, RaftLogEntry, RaftPayload, RaftRegistry, RaftRole,
    Result,
};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// In-process consensus coordinator service managing Raft state and WAL persistence
pub struct RaftConsensusService;

impl RaftConsensusService {
    /// Loads the current Raft consensus status
    pub fn get_status(paths: &CraftPaths) -> Result<RaftStatusSummary> {
        let registry = RaftRegistry::load_or_default(paths)?;
        let wal_path = paths.raft_wal_dir.join("raft.wal");
        let snapshots_dir = paths.raft_snapshots_dir.clone();

        let mut engine = RaftEngine::new(
            "local-node".to_string(),
            wal_path,
            snapshots_dir,
            registry.state.cluster_nodes.clone(),
        );

        let _ = engine.load_from_wal();
        engine.current_term = registry.state.current_term.max(engine.current_term);
        engine.voted_for = registry.state.voted_for.clone();
        if engine.voted_for.as_deref() == Some("local-node") {
            engine.role = RaftRole::Leader;
            engine.leader_id = Some("local-node".to_string());
        }

        let mut status = engine.get_status();
        // If single node cluster or no remote peers, local node can act as autonomous leader
        if status.cluster_nodes.is_empty()
            || (status.cluster_nodes.len() == 1 && status.cluster_nodes[0].id == "local-node")
        {
            status.is_quorum_intact = true;
            status.role = RaftRole::Leader;
            status.leader_id = Some("local-node".to_string());
        }

        Ok(status)
    }

    /// Proposes a new entry to the consensus log and writes to WAL
    pub fn propose(paths: &CraftPaths, payload: RaftPayload) -> Result<(u64, u64)> {
        RaftRegistry::with_lock(paths, |registry| {
            let wal_path = paths.raft_wal_dir.join("raft.wal");
            let snapshots_dir = paths.raft_snapshots_dir.clone();

            let mut engine = RaftEngine::new(
                "local-node".to_string(),
                wal_path,
                snapshots_dir,
                registry.state.cluster_nodes.clone(),
            );

            let _ = engine.load_from_wal();
            if registry.state.current_term > engine.current_term {
                engine.current_term = registry.state.current_term;
            } else if engine.current_term > registry.state.current_term {
                registry.state.current_term = engine.current_term;
            }

            engine.role = RaftRole::Leader;
            engine.leader_id = Some("local-node".to_string());

            let index = engine
                .propose(payload, Some("cli".to_string()))
                .map_err(CraftError::Other)?;

            registry.state.commit_index = index;
            registry.state.last_applied = index;

            Ok((engine.current_term, index))
        })
    }

    /// Acquires a linearizable distributed lock with monotonic fencing token
    pub fn acquire_lock(
        paths: &CraftPaths,
        lock_name: &str,
        holder_id: &str,
        lease_secs: u64,
    ) -> Result<DistributedLock> {
        RaftRegistry::with_lock(paths, |registry| {
            let now = chrono::Utc::now().timestamp();
            let wal_path = paths.raft_wal_dir.join("raft.wal");
            let snapshots_dir = paths.raft_snapshots_dir.clone();

            let mut engine = RaftEngine::new(
                holder_id.to_string(),
                wal_path,
                snapshots_dir,
                registry.state.cluster_nodes.clone(),
            );
            let _ = engine.load_from_wal();

            let term = registry.state.current_term.max(1);
            let next_index = engine.log.last().map(|e| e.index + 1).unwrap_or(1);

            let lock = registry.acquire_lock(
                lock_name,
                holder_id,
                lease_secs,
                now,
                term,
                next_index,
            )?;

            // Replicate lock acquisition via consensus log
            let payload = RaftPayload::DistributedLockAcquire {
                lock_name: lock_name.to_string(),
                holder_id: holder_id.to_string(),
                lease_secs,
            };

            let entry = RaftLogEntry {
                index: next_index,
                term,
                payload,
                timestamp: now,
                client_id: Some(holder_id.to_string()),
            };

            Self::append_wal_file(&paths.raft_wal_dir.join("raft.wal"), &entry)?;

            registry.state.commit_index = next_index;
            registry.state.last_applied = next_index;

            Ok(lock)
        })
    }

    /// Releases a distributed lock
    pub fn release_lock(paths: &CraftPaths, lock_name: &str, holder_id: &str) -> Result<()> {
        RaftRegistry::with_lock(paths, |registry| {
            registry.release_lock(lock_name, holder_id)?;

            let wal_path = paths.raft_wal_dir.join("raft.wal");
            let now = chrono::Utc::now().timestamp();
            let term = registry.state.current_term.max(1);
            let next_index = registry.state.commit_index + 1;

            let payload = RaftPayload::DistributedLockRelease {
                lock_name: lock_name.to_string(),
                holder_id: holder_id.to_string(),
            };

            let entry = RaftLogEntry {
                index: next_index,
                term,
                payload,
                timestamp: now,
                client_id: Some(holder_id.to_string()),
            };

            Self::append_wal_file(&wal_path, &entry)?;
            registry.state.commit_index = next_index;
            registry.state.last_applied = next_index;

            Ok(())
        })
    }

    /// Steps down the active leader
    pub fn step_down(paths: &CraftPaths) -> Result<()> {
        RaftRegistry::with_lock(paths, |registry| {
            registry.state.voted_for = None;
            Ok(())
        })
    }

    /// Transfers leadership to another node
    pub fn transfer_leadership(paths: &CraftPaths, target_node_id: &str) -> Result<()> {
        RaftRegistry::with_lock(paths, |registry| {
            let exists = registry
                .state
                .cluster_nodes
                .iter()
                .any(|n| n.id == target_node_id);

            if !exists {
                return Err(CraftError::Other(format!(
                    "Target node '{}' is not registered in the Raft cluster",
                    target_node_id
                )));
            }

            registry.state.voted_for = Some(target_node_id.to_string());
            Ok(())
        })
    }

    /// Reads consensus log entries from disk
    pub fn get_logs(paths: &CraftPaths, limit: Option<usize>) -> Result<Vec<RaftLogEntry>> {
        let wal_path = paths.raft_wal_dir.join("raft.wal");
        if !wal_path.exists() {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new().read(true).open(&wal_path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<RaftLogEntry>(&line) {
                entries.push(entry);
            }
        }

        if let Some(lim) = limit {
            if entries.len() > lim {
                let start = entries.len() - lim;
                entries = entries.split_off(start);
            }
        }

        Ok(entries)
    }

    /// Appends a single log entry to the WAL file
    fn append_wal_file(wal_path: &Path, entry: &RaftLogEntry) -> Result<()> {
        if let Some(parent) = wal_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(wal_path)?;

        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;
        file.flush()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_raft_consensus_service_workflow() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        // 1. Propose configuration update
        let (term, idx) = RaftConsensusService::propose(
            &paths,
            RaftPayload::ClusterConfigUpdate {
                key: "max_players".into(),
                value: "100".into(),
            },
        )
        .expect("propose should succeed");

        assert_eq!(idx, 1);
        let _ = term;

        // 2. Read logs back
        let logs = RaftConsensusService::get_logs(&paths, None).expect("should get logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].index, 1);

        // 3. Acquire distributed lock
        let lock = RaftConsensusService::acquire_lock(&paths, "test-resource", "worker-1", 60)
            .expect("should acquire lock");
        assert_eq!(lock.name, "test-resource");
        assert_eq!(lock.holder_id, "worker-1");

        // 4. Check status
        let status = RaftConsensusService::get_status(&paths).expect("should get status");
        assert!(status.is_quorum_intact);

        // 5. Release lock
        RaftConsensusService::release_lock(&paths, "test-resource", "worker-1")
            .expect("should release lock");
    }
}
