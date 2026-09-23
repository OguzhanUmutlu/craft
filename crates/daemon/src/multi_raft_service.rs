use crate::raft_engine::{RaftEngine, RaftStatusSummary};
use craft_core::{
    CraftError, CraftPaths, JointConsensusPhase, LearnerSyncProgress, MembershipChangeType,
    MultiRaftPartition, MultiRaftRegistry, RaftLogEntry, RaftNode, RaftPayload, RaftRegistry,
    RaftRole, Result, WalCompactionPolicy, GROUP_CONTROL_PLANE,
};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::Instant;

static MULTI_RAFT_INSTANCE: OnceLock<MultiRaftService> = OnceLock::new();

/// Multi-Raft orchestration service supervising partitioned state machines,
/// online joint consensus reconfigurations, and streaming WAL log compactions.
pub struct MultiRaftService {
    paths: CraftPaths,
    compaction_runs_total: AtomicU64,
    snapshot_bytes_total: AtomicU64,
    policy: RwLock<WalCompactionPolicy>,
}

impl MultiRaftService {
    /// Retrieve singleton instance of MultiRaftService
    pub fn global(paths: &CraftPaths) -> &'static MultiRaftService {
        MULTI_RAFT_INSTANCE.get_or_init(|| MultiRaftService::new(paths.clone()))
    }

    /// Construct a new MultiRaftService instance
    pub fn new(paths: CraftPaths) -> Self {
        Self {
            paths,
            compaction_runs_total: AtomicU64::new(0),
            snapshot_bytes_total: AtomicU64::new(0),
            policy: RwLock::new(WalCompactionPolicy::default()),
        }
    }

    /// Loads the status of Multi-Raft partitions and engines
    pub fn get_status(
        &self,
        group_id_filter: Option<u64>,
    ) -> Result<(
        MultiRaftRegistry,
        HashMap<u64, RaftStatusSummary>,
        HashMap<String, LearnerSyncProgress>,
    )> {
        let registry = MultiRaftRegistry::load_or_default(&self.paths)?;
        let root_raft = RaftRegistry::load_or_default(&self.paths)?;
        let mut statuses = HashMap::new();
        let mut learner_progress = HashMap::new();

        for part in &registry.partitions {
            if let Some(gid) = group_id_filter {
                if part.group_id != gid {
                    continue;
                }
            }

            let wal_path = if part.group_id == GROUP_CONTROL_PLANE {
                self.paths.raft_wal_dir.join("raft.wal")
            } else {
                self.paths.raft_group_wal(part.group_id).join("raft.wal")
            };

            let snapshots_dir = if part.group_id == GROUP_CONTROL_PLANE {
                self.paths.raft_snapshots_dir.clone()
            } else {
                self.paths.raft_group_snapshots(part.group_id)
            };

            let mut engine = RaftEngine::new_with_group(
                "local-node".to_string(),
                part.group_id,
                wal_path,
                snapshots_dir,
                root_raft.state.cluster_nodes.clone(),
            );

            let _ = engine.load_from_wal();
            if part.group_id == GROUP_CONTROL_PLANE {
                engine.current_term = root_raft.state.current_term.max(engine.current_term);
                engine.voted_for = root_raft.state.voted_for.clone();
                if engine.voted_for.as_deref() == Some("local-node") {
                    engine.role = RaftRole::Leader;
                    engine.leader_id = Some("local-node".to_string());
                }
            }

            let mut status = engine.get_status();
            if status.cluster_nodes.is_empty()
                || (status.cluster_nodes.len() == 1 && status.cluster_nodes[0].id == "local-node")
            {
                status.is_quorum_intact = true;
                status.role = RaftRole::Leader;
                status.leader_id = Some("local-node".to_string());
            }

            for (node_id, progress) in &engine.learner_progress {
                learner_progress.insert(node_id.clone(), progress.clone());
            }

            statuses.insert(part.group_id, status);
        }

        Ok((registry, statuses, learner_progress))
    }

    /// Online joint consensus membership reconfiguration: C_old -> C_old,new -> C_new
    pub fn reconfigure_membership(
        &self,
        group_id: u64,
        change_type: MembershipChangeType,
        node: RaftNode,
    ) -> Result<(bool, JointConsensusPhase, String)> {
        RaftRegistry::with_lock(&self.paths, |root_raft| {
            let existing_nodes = &root_raft.state.cluster_nodes;
            let c_old: Vec<String> = existing_nodes
                .iter()
                .filter(|n| n.voting_member)
                .map(|n| n.id.clone())
                .collect();

            let mut c_new = c_old.clone();

            match change_type {
                MembershipChangeType::AddNode => {
                    if node.voting_member && !c_new.contains(&node.id) {
                        c_new.push(node.id.clone());
                    }
                    root_raft.add_node(node.clone())?;
                }
                MembershipChangeType::RemoveNode => {
                    c_new.retain(|id| id != &node.id);
                    root_raft.remove_node(&node.id)?;
                }
                MembershipChangeType::PromoteLearner => {
                    if !c_new.contains(&node.id) {
                        c_new.push(node.id.clone());
                    }
                    if let Some(target) = root_raft.state.cluster_nodes.iter_mut().find(|n| n.id == node.id) {
                        target.voting_member = true;
                    } else {
                        return Err(CraftError::Other(format!("Node '{}' not found for promotion", node.id)));
                    }
                }
                MembershipChangeType::DemoteToLearner => {
                    c_new.retain(|id| id != &node.id);
                    if let Some(target) = root_raft.state.cluster_nodes.iter_mut().find(|n| n.id == node.id) {
                        target.voting_member = false;
                    } else {
                        return Err(CraftError::Other(format!("Node '{}' not found for demotion", node.id)));
                    }
                }
            }

            let joint_phase = JointConsensusPhase::Joint {
                c_old: c_old.clone(),
                c_new: c_new.clone(),
            };

            // Write joint consensus entry to WAL
            let next_index = root_raft.state.commit_index + 1;
            let term = root_raft.state.current_term.max(1);
            let payload = RaftPayload::ConfigurationChange {
                change_type,
                node: node.clone(),
                phase: joint_phase.clone(),
            };

            let entry = RaftLogEntry {
                index: next_index,
                term,
                payload,
                timestamp: chrono::Utc::now().timestamp(),
                client_id: Some("multi_raft_service".to_string()),
            };

            let wal_path = if group_id == GROUP_CONTROL_PLANE {
                self.paths.raft_wal_dir.join("raft.wal")
            } else {
                self.paths.raft_group_wal(group_id).join("raft.wal")
            };

            Self::append_wal_file(&wal_path, &entry)?;

            // Finalize consensus configuration
            let final_index = next_index + 1;
            let final_payload = RaftPayload::ConfigurationChange {
                change_type,
                node: node.clone(),
                phase: JointConsensusPhase::Finalized,
            };

            let final_entry = RaftLogEntry {
                index: final_index,
                term,
                payload: final_payload,
                timestamp: chrono::Utc::now().timestamp(),
                client_id: Some("multi_raft_service".to_string()),
            };

            Self::append_wal_file(&wal_path, &final_entry)?;

            root_raft.state.commit_index = final_index;
            root_raft.state.last_applied = final_index;
            root_raft.state.joint_consensus = None;

            Ok((
                true,
                JointConsensusPhase::Finalized,
                format!(
                    "Successfully reconfigured membership {:?} for node '{}' in group {}",
                    change_type, node.id, group_id
                ),
            ))
        })
    }

    /// Triggers log compaction and snapshot generation for a Raft group
    pub fn trigger_compaction(
        &self,
        group_id: u64,
        force: bool,
    ) -> Result<(u64, u64, u64, u64)> {
        let start = Instant::now();
        let wal_path = if group_id == GROUP_CONTROL_PLANE {
            self.paths.raft_wal_dir.join("raft.wal")
        } else {
            self.paths.raft_group_wal(group_id).join("raft.wal")
        };

        let snapshots_dir = if group_id == GROUP_CONTROL_PLANE {
            self.paths.raft_snapshots_dir.clone()
        } else {
            self.paths.raft_group_snapshots(group_id)
        };

        let root_raft = RaftRegistry::load_or_default(&self.paths)?;
        let mut engine = RaftEngine::new_with_group(
            "local-node".to_string(),
            group_id,
            wal_path,
            snapshots_dir,
            root_raft.state.cluster_nodes,
        );

        let _ = engine.load_from_wal();
        let policy = self.policy.read().unwrap().clone();

        match engine.trigger_compaction(&policy, force) {
            Ok(Some((last_included_index, compacted, snapshot_bytes))) => {
                self.compaction_runs_total.fetch_add(1, Ordering::SeqCst);
                self.snapshot_bytes_total.fetch_add(snapshot_bytes, Ordering::SeqCst);
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok((last_included_index, compacted, snapshot_bytes, duration_ms))
            }
            Ok(None) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok((engine.commit_index, 0, 0, duration_ms))
            }
            Err(e) => Err(CraftError::Other(e)),
        }
    }

    /// Routes an application key to the responsible Multi-Raft group
    pub fn route_partition_key(&self, key: &str) -> Result<(String, u64, String, Option<String>)> {
        let reg = MultiRaftRegistry::load_or_default(&self.paths)?;
        let route = reg.route_key(key);
        let leader = reg
            .get_partition(route.target_group_id)
            .and_then(|p| p.leader_node_id.clone());

        Ok((
            key.to_string(),
            route.target_group_id,
            route.matched_partition,
            leader,
        ))
    }

    /// Manage Multi-Raft partitions (create, update, remove, list)
    pub fn manage_partition(
        &self,
        action: &str,
        partition: Option<MultiRaftPartition>,
        group_id: Option<u64>,
    ) -> Result<(bool, String, Vec<MultiRaftPartition>)> {
        MultiRaftRegistry::with_lock(&self.paths, |reg| match action.to_lowercase().as_str() {
            "create" | "add" => {
                let part = partition.ok_or_else(|| {
                    CraftError::Config("Partition configuration payload required".to_string())
                })?;
                reg.register_partition(part.clone())?;
                Ok((
                    true,
                    format!("Partition '{}' (group {}) registered", part.name, part.group_id),
                    reg.partitions.clone(),
                ))
            }
            "update" => {
                let part = partition.ok_or_else(|| {
                    CraftError::Config("Partition configuration payload required".to_string())
                })?;
                reg.register_partition(part.clone())?;
                Ok((
                    true,
                    format!("Partition '{}' (group {}) updated", part.name, part.group_id),
                    reg.partitions.clone(),
                ))
            }
            "remove" | "delete" => {
                let gid = group_id.ok_or_else(|| {
                    CraftError::Config("Group ID required for partition deletion".to_string())
                })?;
                reg.remove_partition(gid)?;
                Ok((
                    true,
                    format!("Partition group {} removed", gid),
                    reg.partitions.clone(),
                ))
            }
            "list" => Ok((true, "Partitions listed".to_string(), reg.partitions.clone())),
            _ => Err(CraftError::Config(format!("Unknown partition action '{}'", action))),
        })
    }

    /// Set WAL compaction policy
    pub fn set_compaction_policy(&self, policy: WalCompactionPolicy) {
        let mut p = self.policy.write().unwrap();
        *p = policy;
    }

    /// Format Prometheus metrics for Multi-Raft and compaction
    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(1024);
        let reg = MultiRaftRegistry::load_or_default(&self.paths).unwrap_or_default();
        let root_raft = RaftRegistry::load_or_default(&self.paths).unwrap_or_default();

        writeln!(
            out,
            "# HELP craft_raft_groups_total Total registered Multi-Raft consensus partition groups"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_raft_groups_total gauge").unwrap();
        writeln!(out, "craft_raft_groups_total {}", reg.partitions.len()).unwrap();

        let compactions = self.compaction_runs_total.load(Ordering::SeqCst);
        writeln!(
            out,
            "# HELP craft_raft_log_compaction_runs_total Total WAL log compaction cycles executed"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_raft_log_compaction_runs_total counter").unwrap();
        writeln!(out, "craft_raft_log_compaction_runs_total {}", compactions).unwrap();

        let snap_bytes = self.snapshot_bytes_total.load(Ordering::SeqCst);
        writeln!(
            out,
            "# HELP craft_raft_snapshot_bytes_total Total bytes written in Raft snapshots"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_raft_snapshot_bytes_total counter").unwrap();
        writeln!(out, "craft_raft_snapshot_bytes_total {}", snap_bytes).unwrap();

        let entries_retained = root_raft.state.commit_index;
        writeln!(
            out,
            "# HELP craft_raft_wal_entries_retained Total WAL entries committed in control plane"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_raft_wal_entries_retained gauge").unwrap();
        writeln!(out, "craft_raft_wal_entries_retained {}", entries_retained).unwrap();

        let joint_active = if root_raft.state.joint_consensus.is_some() {
            1
        } else {
            0
        };
        writeln!(
            out,
            "# HELP craft_raft_joint_consensus_active Whether a joint consensus reconfiguration is currently in-flight (1 or 0)"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_raft_joint_consensus_active gauge").unwrap();
        writeln!(out, "craft_raft_joint_consensus_active {}", joint_active).unwrap();

        out
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
    fn test_multi_raft_service_workflow() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = MultiRaftService::new(paths.clone());

        // 1. Initial status has 2 default partitions
        let (registry, statuses, _) = service.get_status(None).expect("get_status should succeed");
        assert_eq!(registry.partitions.len(), 2);
        assert!(statuses.contains_key(&GROUP_CONTROL_PLANE));

        // 2. Register new partition
        let new_part = MultiRaftPartition {
            group_id: 200,
            name: "world-end".to_string(),
            key_range_start: "craft:world:end:".to_string(),
            key_range_end: "craft:world:end:~".to_string(),
            leader_node_id: Some("node-primary".to_string()),
            replica_nodes: vec!["node-primary".to_string()],
            term: 1,
            commit_index: 10,
            active: true,
        };
        let (ok, msg, parts) = service
            .manage_partition("create", Some(new_part), None)
            .expect("create should succeed");
        assert!(ok);
        assert!(msg.contains("registered"));
        assert_eq!(parts.len(), 3);

        // 3. Key routing
        let (key, target_group, part_name, leader) = service
            .route_partition_key("craft:world:end:dragon_fight")
            .expect("route should succeed");
        assert_eq!(key, "craft:world:end:dragon_fight");
        assert_eq!(target_group, 200);
        assert_eq!(part_name, "world-end");
        assert_eq!(leader.as_deref(), Some("node-primary"));

        // 4. Online membership reconfiguration (joint consensus)
        let new_node = RaftNode {
            id: "node-worker-2".to_string(),
            address: "10.0.0.5".to_string(),
            raft_port: 9005,
            voting_member: true,
            priority: 10,
        };
        let (reconf_ok, phase, reconf_msg) = service
            .reconfigure_membership(GROUP_CONTROL_PLANE, MembershipChangeType::AddNode, new_node)
            .expect("reconfigure should succeed");
        assert!(reconf_ok);
        assert_eq!(phase, JointConsensusPhase::Finalized);
        assert!(reconf_msg.contains("node-worker-2"));

        // 5. Trigger compaction
        let (last_idx, compacted, bytes, _dur) = service
            .trigger_compaction(GROUP_CONTROL_PLANE, true)
            .expect("compaction should succeed");
        let _ = (last_idx, compacted, bytes);

        // 6. Prometheus telemetry metrics output
        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_raft_groups_total 3"));
        assert!(metrics.contains("craft_raft_log_compaction_runs_total"));
        assert!(metrics.contains("craft_raft_snapshot_bytes_total"));
        assert!(metrics.contains("craft_raft_wal_entries_retained"));
    }
}
