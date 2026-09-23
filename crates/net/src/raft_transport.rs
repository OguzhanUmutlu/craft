use craft_core::{ArbitrationWeight, QuorumStatus, RaftLogEntry, RaftNode, SnapshotChunk};
use serde::{Deserialize, Serialize};

/// 4-byte pure-Rust Raft binary wire magic: 'CRFT' (0x43, 0x52, 0x46, 0x54)
pub const CRAFT_RAFT_MAGIC: [u8; 4] = [0x43, 0x52, 0x46, 0x54];

/// Raft RequestVote RPC arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestVoteArgs {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

/// Raft RequestVote RPC response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestVoteReply {
    pub term: u64,
    pub vote_granted: bool,
    pub reason: Option<String>,
}

/// Raft AppendEntries RPC arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntriesArgs {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<RaftLogEntry>,
    pub leader_commit: u64,
}

/// Raft AppendEntries RPC response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntriesReply {
    pub term: u64,
    pub success: bool,
    pub match_index: u64,
    pub conflict_term: Option<u64>,
    pub conflict_index: Option<u64>,
}

/// Raft InstallSnapshot RPC arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotArgs {
    pub term: u64,
    pub leader_id: String,
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub data: Vec<u8>,
    pub offset: u64,
    pub done: bool,
}

/// Raft InstallSnapshot RPC response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotReply {
    pub term: u64,
}

/// Streaming Snapshot Chunk RPC arguments for Multi-Raft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotChunkArgs {
    pub term: u64,
    pub leader_id: String,
    pub group_id: u64,
    pub chunk: SnapshotChunk,
}

/// Streaming Snapshot Chunk RPC response for Multi-Raft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotChunkReply {
    pub term: u64,
    pub group_id: u64,
    pub chunk_index: u32,
    pub success: bool,
    pub error: Option<String>,
}

/// Fast Raft Heartbeat arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatArgs {
    pub term: u64,
    pub leader_id: String,
    pub leader_commit: u64,
}

/// Fast Raft Heartbeat response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatReply {
    pub term: u64,
    pub node_id: String,
    pub ack: bool,
}

/// Top-level Raft RPC network envelope
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RaftRpcMessage {
    RequestVote(RequestVoteArgs),
    RequestVoteReply(RequestVoteReply),
    AppendEntries(AppendEntriesArgs),
    AppendEntriesReply(AppendEntriesReply),
    InstallSnapshot(InstallSnapshotArgs),
    InstallSnapshotReply(InstallSnapshotReply),
    InstallSnapshotChunk(InstallSnapshotChunkArgs),
    InstallSnapshotChunkReply(InstallSnapshotChunkReply),
    Heartbeat(HeartbeatArgs),
    HeartbeatReply(HeartbeatReply),
}

/// Multi-Raft channel multiplexing envelope allowing multiple Raft groups over a single connection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaftMessageEnvelope {
    pub group_id: u64,
    pub message: RaftRpcMessage,
}

impl RaftMessageEnvelope {
    pub fn new(group_id: u64, message: RaftRpcMessage) -> Self {
        Self { group_id, message }
    }
}

/// Encodes a Raft RPC message into a binary frame prefixed with CRAFT_RAFT_MAGIC and length
pub fn encode_raft_message(msg: &RaftRpcMessage) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(msg).map_err(|e| format!("Serialization error: {}", e))?;
    let length = payload.len() as u32;

    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&CRAFT_RAFT_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);

    Ok(frame)
}

/// Decodes a Raft RPC message from a binary buffer if a full frame is present.
/// Returns Ok(Some((msg, bytes_consumed))) or Ok(None) if more bytes are needed.
pub fn decode_raft_message(buffer: &[u8]) -> Result<Option<(RaftRpcMessage, usize)>, String> {
    if buffer.len() < 8 {
        return Ok(None);
    }

    if buffer[0..4] != CRAFT_RAFT_MAGIC {
        return Err(format!(
            "Invalid Raft wire magic: expected {:?}, got {:?}",
            CRAFT_RAFT_MAGIC,
            &buffer[0..4]
        ));
    }

    let length = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
    let total_frame_len = 8 + length;

    if buffer.len() < total_frame_len {
        return Ok(None);
    }

    let payload = &buffer[8..total_frame_len];
    let msg: RaftRpcMessage =
        serde_json::from_slice(payload).map_err(|e| format!("Deserialization error: {}", e))?;

    Ok(Some((msg, total_frame_len)))
}

/// Encodes a multi-group Raft message envelope into binary wire frame
pub fn encode_raft_envelope(env: &RaftMessageEnvelope) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(env).map_err(|e| format!("Serialization error: {}", e))?;
    let length = payload.len() as u32;

    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&CRAFT_RAFT_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);

    Ok(frame)
}

/// Decodes a multi-group Raft message envelope from binary wire frame
pub fn decode_raft_envelope(buffer: &[u8]) -> Result<Option<(RaftMessageEnvelope, usize)>, String> {
    if buffer.len() < 8 {
        return Ok(None);
    }

    if buffer[0..4] != CRAFT_RAFT_MAGIC {
        return Err(format!(
            "Invalid Raft wire magic: expected {:?}, got {:?}",
            CRAFT_RAFT_MAGIC,
            &buffer[0..4]
        ));
    }

    let length = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
    let total_frame_len = 8 + length;

    if buffer.len() < total_frame_len {
        return Ok(None);
    }

    let payload = &buffer[8..total_frame_len];
    let envelope: RaftMessageEnvelope =
        serde_json::from_slice(payload).map_err(|e| format!("Deserialization error: {}", e))?;

    Ok(Some((envelope, total_frame_len)))
}

/// Dynamic Split-Brain Arbitrator with health-weighted quorums and edge tie-breaker logic
#[derive(Debug, Clone, Default)]
pub struct SplitBrainArbitrator;

impl SplitBrainArbitrator {
    /// Evaluates whether a cluster partition possesses a valid quorum, resolving even splits via edge tie-breakers
    pub fn evaluate_quorum(
        total_nodes: &[RaftNode],
        responsive_node_ids: &[String],
        weights: &[ArbitrationWeight],
        edge_tie_breaker: Option<&str>,
    ) -> QuorumStatus {
        let voting_nodes: Vec<&RaftNode> = total_nodes.iter().filter(|n| n.voting_member).collect();
        let total_members = voting_nodes.len();

        if total_members == 0 {
            return QuorumStatus {
                total_members: 0,
                healthy_members: 0,
                majority_threshold: 0,
                is_quorum_intact: false,
                arbitrated_leader: None,
            };
        }

        let majority_threshold = (total_members / 2) + 1;
        let healthy_members = voting_nodes
            .iter()
            .filter(|n| responsive_node_ids.iter().any(|id| id == &n.id))
            .count();

        // Standard majority quorum rule
        if healthy_members >= majority_threshold {
            let leader = Self::select_best_candidate(responsive_node_ids, weights);
            return QuorumStatus {
                total_members,
                healthy_members,
                majority_threshold,
                is_quorum_intact: true,
                arbitrated_leader: leader,
            };
        }

        // Even-split condition: exact 50% partition (e.g., 2 of 4 or 1 of 2)
        if total_members > 1 && healthy_members * 2 == total_members {
            if let Some(tie_breaker) = edge_tie_breaker {
                let has_tie_breaker = responsive_node_ids.iter().any(|id| id == tie_breaker);
                if has_tie_breaker {
                    let leader = Self::select_best_candidate(responsive_node_ids, weights);
                    return QuorumStatus {
                        total_members,
                        healthy_members,
                        majority_threshold,
                        is_quorum_intact: true,
                        arbitrated_leader: leader,
                    };
                }
            }
        }

        // Sub-quorum partition: cannot commit new log entries
        QuorumStatus {
            total_members,
            healthy_members,
            majority_threshold,
            is_quorum_intact: false,
            arbitrated_leader: None,
        }
    }

    /// Selects the highest weighted responsive node as candidate
    fn select_best_candidate(
        responsive_node_ids: &[String],
        weights: &[ArbitrationWeight],
    ) -> Option<String> {
        let mut best_id = responsive_node_ids.first().cloned();
        let mut best_score = -1.0;

        for id in responsive_node_ids {
            if let Some(w) = weights.iter().find(|w| &w.node_id == id) {
                if w.weight_score > best_score {
                    best_score = w.weight_score;
                    best_id = Some(id.clone());
                }
            }
        }

        best_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_message_framing_roundtrip() {
        let msg = RaftRpcMessage::RequestVote(RequestVoteArgs {
            term: 5,
            candidate_id: "node-alpha".to_string(),
            last_log_index: 42,
            last_log_term: 4,
        });

        let encoded = encode_raft_message(&msg).expect("encoding should succeed");
        assert_eq!(&encoded[0..4], &CRAFT_RAFT_MAGIC);

        let decoded = decode_raft_message(&encoded).expect("decoding should succeed");
        assert!(decoded.is_some());

        let (unpacked, consumed) = decoded.unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(unpacked, msg);
    }

    #[test]
    fn test_corrupted_magic_rejection() {
        let mut bad_frame = vec![0x00, 0x01, 0x02, 0x03, 0x00, 0x00, 0x00, 0x05, 1, 2, 3, 4, 5];
        let result = decode_raft_message(&bad_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid Raft wire magic"));

        // Incomplete buffer returns None
        bad_frame[0..4].copy_from_slice(&CRAFT_RAFT_MAGIC);
        let incomplete = decode_raft_message(&bad_frame[0..6]).unwrap();
        assert!(incomplete.is_none());
    }

    #[test]
    fn test_arbitrator_odd_cluster_majority() {
        let nodes = vec![
            RaftNode {
                id: "n1".into(),
                address: "10.0.0.1".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n2".into(),
                address: "10.0.0.2".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n3".into(),
                address: "10.0.0.3".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
        ];

        let responsive = vec!["n1".to_string(), "n2".to_string()];
        let status = SplitBrainArbitrator::evaluate_quorum(&nodes, &responsive, &[], None);

        assert!(status.is_quorum_intact);
        assert_eq!(status.healthy_members, 2);
        assert_eq!(status.majority_threshold, 2);
        assert_eq!(status.total_members, 3);
    }

    #[test]
    fn test_arbitrator_minority_partition_loss() {
        let nodes = vec![
            RaftNode {
                id: "n1".into(),
                address: "10.0.0.1".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n2".into(),
                address: "10.0.0.2".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n3".into(),
                address: "10.0.0.3".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
        ];

        let responsive = vec!["n1".to_string()];
        let status = SplitBrainArbitrator::evaluate_quorum(&nodes, &responsive, &[], None);

        assert!(!status.is_quorum_intact);
        assert_eq!(status.healthy_members, 1);
        assert_eq!(status.majority_threshold, 2);
        assert!(status.arbitrated_leader.is_none());
    }

    #[test]
    fn test_arbitrator_even_split_with_edge_tie_breaker() {
        let nodes = vec![
            RaftNode {
                id: "n1".into(),
                address: "10.0.0.1".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n2".into(),
                address: "10.0.0.2".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n3".into(),
                address: "10.0.0.3".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
            RaftNode {
                id: "n4".into(),
                address: "10.0.0.4".into(),
                raft_port: 9000,
                voting_member: true,
                priority: 10,
            },
        ];

        let partition_a = vec!["n1".to_string(), "n2".to_string()];
        let partition_b = vec!["n3".to_string(), "n4".to_string()];

        // Without tie-breaker: neither side can form quorum in 2 vs 2 split
        let status_no_tb =
            SplitBrainArbitrator::evaluate_quorum(&nodes, &partition_a, &[], None);
        assert!(!status_no_tb.is_quorum_intact);

        // With tie-breaker designated as n1: partition A wins quorum, partition B loses
        let status_a =
            SplitBrainArbitrator::evaluate_quorum(&nodes, &partition_a, &[], Some("n1"));
        assert!(status_a.is_quorum_intact);

        let status_b =
            SplitBrainArbitrator::evaluate_quorum(&nodes, &partition_b, &[], Some("n1"));
        assert!(!status_b.is_quorum_intact);
    }

    #[test]
    fn test_raft_message_envelope_roundtrip() {
        let inner = RaftRpcMessage::Heartbeat(HeartbeatArgs {
            term: 7,
            leader_id: "node-primary".to_string(),
            leader_commit: 350,
        });

        let envelope = RaftMessageEnvelope::new(100, inner);
        let encoded = encode_raft_envelope(&envelope).expect("envelope should encode");
        assert_eq!(&encoded[0..4], &CRAFT_RAFT_MAGIC);

        let decoded = decode_raft_envelope(&encoded).expect("envelope should decode");
        assert!(decoded.is_some());
        let (unpacked, consumed) = decoded.unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(unpacked.group_id, 100);
        assert_eq!(unpacked, envelope);
    }

    #[test]
    fn test_install_snapshot_chunk_rpc_framing() {
        use craft_core::SnapshotChunk;

        let chunk = SnapshotChunk::new("snap-55".to_string(), 100, 0, 1, vec![10, 20, 30, 40], true);
        let msg = RaftRpcMessage::InstallSnapshotChunk(InstallSnapshotChunkArgs {
            term: 3,
            leader_id: "node-lead".to_string(),
            group_id: 100,
            chunk,
        });

        let encoded = encode_raft_message(&msg).expect("snapshot chunk rpc should encode");
        let decoded = decode_raft_message(&encoded).expect("snapshot chunk rpc should decode");
        assert!(decoded.is_some());
        let (unpacked, _) = decoded.unwrap();
        assert_eq!(unpacked, msg);
    }
}
