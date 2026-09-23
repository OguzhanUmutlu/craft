use craft_core::{AnycastRouteAnnouncement, AnycastRouteStatus, MemoryPageChunk};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// 4-byte pure-Rust Migration wire magic: 'CMIG' (0x43, 0x4D, 0x49, 0x47)
pub const CRAFT_MIGRATION_MAGIC: [u8; 4] = [0x43, 0x4D, 0x49, 0x47];

/// In-flight state of an edge-proxied player TCP connection during live migration
#[derive(Debug)]
pub enum SplicerState {
    DirectPassThrough,
    Buffering {
        freeze_started_at: Instant,
        buffered_upstream: Vec<Vec<u8>>,
        buffered_downstream: Vec<Vec<u8>>,
    },
    HandoffComplete {
        buffered_upstream: Vec<Vec<u8>>,
        buffered_downstream: Vec<Vec<u8>>,
    },
    SplicedToNewTarget,
}

/// Transparent TCP connection splicer and packet buffer for sub-150ms live migration
#[derive(Debug)]
pub struct ConnectionSplicer {
    pub session_id: String,
    pub client_addr: String,
    pub target_addr: String,
    pub state: SplicerState,
    pub client_seq: u32,
    pub server_seq: u32,
}

impl ConnectionSplicer {
    pub fn new(session_id: impl Into<String>, client_addr: impl Into<String>, target_addr: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            client_addr: client_addr.into(),
            target_addr: target_addr.into(),
            state: SplicerState::DirectPassThrough,
            client_seq: 0,
            server_seq: 0,
        }
    }

    /// Transitions connection into freeze buffering mode
    pub fn start_freeze(&mut self) {
        self.state = SplicerState::Buffering {
            freeze_started_at: Instant::now(),
            buffered_upstream: Vec::new(),
            buffered_downstream: Vec::new(),
        };
    }

    /// Buffers an incoming packet from the client destined for the game server
    pub fn buffer_upstream(&mut self, packet: Vec<u8>) {
        if let SplicerState::Buffering { ref mut buffered_upstream, .. } = self.state {
            self.client_seq = self.client_seq.wrapping_add(packet.len() as u32);
            buffered_upstream.push(packet);
        }
    }

    /// Buffers an outgoing packet from the game server destined for the client
    pub fn buffer_downstream(&mut self, packet: Vec<u8>) {
        if let SplicerState::Buffering { ref mut buffered_downstream, .. } = self.state {
            self.server_seq = self.server_seq.wrapping_add(packet.len() as u32);
            buffered_downstream.push(packet);
        }
    }

    /// Returns elapsed freeze duration in milliseconds if currently buffering
    pub fn freeze_duration_ms(&self) -> u64 {
        match &self.state {
            SplicerState::Buffering { freeze_started_at, .. } => {
                freeze_started_at.elapsed().as_millis() as u64
            }
            _ => 0,
        }
    }

    pub fn is_buffering(&self) -> bool {
        matches!(self.state, SplicerState::Buffering { .. })
    }

    pub fn buffered_count(&self) -> (usize, usize) {
        match &self.state {
            SplicerState::Buffering { buffered_upstream, buffered_downstream, .. } => {
                (buffered_upstream.len(), buffered_downstream.len())
            }
            SplicerState::HandoffComplete { buffered_upstream, buffered_downstream } => {
                (buffered_upstream.len(), buffered_downstream.len())
            }
            _ => (0, 0),
        }
    }

    /// Serializes active socket state and buffered frames into an atomic handoff frame
    pub fn generate_handoff_frame(
        &mut self,
        new_target_addr: &str,
        player_uuid: &str,
        username: &str,
        protocol_version: i32,
        entity_id: i32,
        cipher_state: Vec<u8>,
    ) -> PlayerSocketHandoffFrame {
        let freeze_ms = self.freeze_duration_ms();
        let (up, down) = match std::mem::replace(&mut self.state, SplicerState::DirectPassThrough) {
            SplicerState::Buffering { buffered_upstream, buffered_downstream, .. } => {
                (buffered_upstream, buffered_downstream)
            }
            _ => (Vec::new(), Vec::new()),
        };

        let frame = PlayerSocketHandoffFrame {
            session_id: self.session_id.clone(),
            player_uuid: player_uuid.to_string(),
            username: username.to_string(),
            client_addr: self.client_addr.clone(),
            old_target_addr: self.target_addr.clone(),
            new_target_addr: new_target_addr.to_string(),
            protocol_version,
            entity_id,
            client_sequence_num: self.client_seq,
            server_sequence_num: self.server_seq,
            cipher_state_bytes: cipher_state,
            buffered_upstream_packets: up.clone(),
            buffered_downstream_packets: down.clone(),
            freeze_duration_ms: freeze_ms,
            created_at: chrono::Utc::now().timestamp() as u64,
        };

        self.target_addr = new_target_addr.to_string();
        self.state = SplicerState::HandoffComplete {
            buffered_upstream: up,
            buffered_downstream: down,
        };

        frame
    }

    /// Completes the splice transition to the new target server and returns buffered packets for replay
    pub fn resume_splice(&mut self) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let drained = match std::mem::replace(&mut self.state, SplicerState::SplicedToNewTarget) {
            SplicerState::HandoffComplete { buffered_upstream, buffered_downstream } => {
                (buffered_upstream, buffered_downstream)
            }
            SplicerState::Buffering { buffered_upstream, buffered_downstream, .. } => {
                (buffered_upstream, buffered_downstream)
            }
            _ => (Vec::new(), Vec::new()),
        };
        drained
    }
}

/// Encapsulated player socket descriptor transferred to destination node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSocketHandoffFrame {
    pub session_id: String,
    pub player_uuid: String,
    pub username: String,
    pub client_addr: String,
    pub old_target_addr: String,
    pub new_target_addr: String,
    pub protocol_version: i32,
    pub entity_id: i32,
    pub client_sequence_num: u32,
    pub server_sequence_num: u32,
    pub cipher_state_bytes: Vec<u8>,
    pub buffered_upstream_packets: Vec<Vec<u8>>,
    pub buffered_downstream_packets: Vec<Vec<u8>>,
    pub freeze_duration_ms: u64,
    pub created_at: u64,
}

/// Messages transmitted across the migration control and data plane
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MigrationWireMessage {
    Handshake {
        node_id: String,
        migration_id: String,
    },
    PreCopyData {
        migration_id: String,
        round: u32,
        chunk: MemoryPageChunk,
    },
    PreCopyAck {
        migration_id: String,
        round: u32,
        chunk_page_addr: u64,
        crc32: u32,
        status: String,
    },
    FreezeNotify {
        migration_id: String,
        freeze_timeout_ms: u64,
    },
    SocketHandoff {
        frame: PlayerSocketHandoffFrame,
    },
    HandoffAck {
        session_id: String,
        success: bool,
        error: Option<String>,
    },
    CommitSwitch {
        migration_id: String,
    },
    AbortRollback {
        migration_id: String,
        reason: String,
    },
}

/// Encodes a migration wire message into a framed byte stream
pub fn encode_migration_message(msg: &MigrationWireMessage) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(msg).map_err(|e| format!("Serialization error: {}", e))?;
    let length = payload.len() as u32;

    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&CRAFT_MIGRATION_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);

    Ok(frame)
}

/// Decodes a migration wire message from a binary buffer
pub fn decode_migration_message(buffer: &[u8]) -> Result<Option<(MigrationWireMessage, usize)>, String> {
    if buffer.len() < 8 {
        return Ok(None);
    }

    if buffer[0..4] != CRAFT_MIGRATION_MAGIC {
        return Err(format!(
            "Invalid migration wire magic: expected {:?}, got {:?}",
            CRAFT_MIGRATION_MAGIC,
            &buffer[0..4]
        ));
    }

    let length = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
    let total_len = 8 + length;

    if buffer.len() < total_len {
        return Ok(None);
    }

    let payload = &buffer[8..total_len];
    let msg: MigrationWireMessage =
        serde_json::from_slice(payload).map_err(|e| format!("Deserialization error: {}", e))?;

    Ok(Some((msg, total_len)))
}

/// BGP routing generator and dynamic route steering engine
pub struct AnycastBgpEngine;

impl AnycastBgpEngine {
    /// Generates BIRD routing protocol configuration for Anycast prefix advertisement
    pub fn generate_bird_config(
        route: &AnycastRouteAnnouncement,
        local_asn: u32,
        peer_ip: &str,
    ) -> String {
        let community_str = route
            .community
            .iter()
            .map(|c| format!("({})", c.replace(':', ",")))
            .collect::<Vec<_>>()
            .join(", ");

        let (action_stmt, prepend_stmt) = match route.status {
            AnycastRouteStatus::Announced => ("accept;", ""),
            AnycastRouteStatus::Withdrawn => ("reject;", ""),
            AnycastRouteStatus::PrependPath => (
                "accept;",
                "bgp_path.prepend(bgp_path.first);\nbgp_path.prepend(bgp_path.first);\n",
            ),
        };

        format!(
            r#"# Craft Anycast BIRD Configuration - Autogenerated
protocol bgp craft_anycast_{prefix_clean} {{
    local as {local_asn};
    neighbor {peer_ip} as {asn};
    import none;
    export filter {{
        if net = {prefix} then {{
            bgp_local_pref = {local_pref};
            bgp_med = {metric};
            {prepend_stmt}bgp_community.add([{community_str}]);
            {action_stmt}
        }}
        reject;
    }};
}}
"#,
            prefix_clean = route.prefix.replace(['.', '/'], "_"),
            local_asn = local_asn,
            peer_ip = peer_ip,
            asn = route.asn,
            prefix = route.prefix,
            local_pref = route.local_pref,
            metric = route.metric,
            prepend_stmt = prepend_stmt,
            community_str = community_str,
            action_stmt = action_stmt,
        )
    }

    /// Generates FRRouting (FRR) vtysh configuration commands
    pub fn generate_frr_config(
        route: &AnycastRouteAnnouncement,
        local_asn: u32,
        peer_ip: &str,
    ) -> String {
        let prefix_clean = route.prefix.replace(['.', '/'], "_");
        let route_map_name = format!("RM_CRAFT_{}", prefix_clean);

        match route.status {
            AnycastRouteStatus::Withdrawn => {
                format!(
                    "configure terminal\nrouter bgp {local_asn}\n no network {prefix}\nexit\n",
                    local_asn = local_asn,
                    prefix = route.prefix
                )
            }
            AnycastRouteStatus::Announced => {
                format!(
                    r#"configure terminal
router bgp {local_asn}
 neighbor {peer_ip} remote-as {peer_asn}
 network {prefix} route-map {route_map_name}
exit
route-map {route_map_name} permit 10
 set local-preference {local_pref}
 set metric {metric}
 set community {community}
exit
"#,
                    local_asn = local_asn,
                    peer_ip = peer_ip,
                    peer_asn = route.asn,
                    prefix = route.prefix,
                    route_map_name = route_map_name,
                    local_pref = route.local_pref,
                    metric = route.metric,
                    community = route.community.join(" "),
                )
            }
            AnycastRouteStatus::PrependPath => {
                format!(
                    r#"configure terminal
router bgp {local_asn}
 neighbor {peer_ip} remote-as {peer_asn}
 network {prefix} route-map {route_map_name}
exit
route-map {route_map_name} permit 10
 set local-preference {local_pref}
 set as-path prepend {local_asn} {local_asn}
 set metric {metric}
exit
"#,
                    local_asn = local_asn,
                    peer_ip = peer_ip,
                    peer_asn = route.asn,
                    prefix = route.prefix,
                    route_map_name = route_map_name,
                    local_pref = route.local_pref,
                    metric = route.metric,
                )
            }
        }
    }

    /// Generates ExaBGP CLI announcement or withdrawal commands
    pub fn generate_exabgp_command(
        route: &AnycastRouteAnnouncement,
        action: &str,
    ) -> String {
        match action.to_lowercase().as_str() {
            "withdraw" | "del" => {
                format!("neighbor all withdraw route {}", route.prefix)
            }
            "prepend" => {
                format!(
                    "neighbor all announce route {} next-hop self as-path [ {} {} ] local-preference {}",
                    route.prefix, route.asn, route.asn, route.local_pref
                )
            }
            _ => {
                let comm_str = if route.community.is_empty() {
                    String::new()
                } else {
                    format!(" community [ {} ]", route.community.join(" "))
                };
                format!(
                    "neighbor all announce route {} next-hop self local-preference {} med {}{}",
                    route.prefix, route.local_pref, route.metric, comm_str
                )
            }
        }
    }

    /// Evaluates current datacenter load and dynamically steers BGP prefix
    pub fn evaluate_health_and_steer(
        current_load_pct: f64,
        max_load_pct: f64,
        route: &mut AnycastRouteAnnouncement,
    ) -> bool {
        let changed = if current_load_pct >= max_load_pct {
            // Overloaded: prepend or withdraw to drain traffic to other POPs
            if route.status != AnycastRouteStatus::Withdrawn {
                route.status = AnycastRouteStatus::Withdrawn;
                route.active = false;
                route.updated_at = chrono::Utc::now().timestamp() as u64;
                true
            } else {
                false
            }
        } else if current_load_pct >= (max_load_pct * 0.85) {
            // High load: prepend path to prefer other POPs but keep fallback
            if route.status != AnycastRouteStatus::PrependPath {
                route.status = AnycastRouteStatus::PrependPath;
                route.active = true;
                route.updated_at = chrono::Utc::now().timestamp() as u64;
                true
            } else {
                false
            }
        } else {
            // Healthy load: announce normally
            if route.status != AnycastRouteStatus::Announced {
                route.status = AnycastRouteStatus::Announced;
                route.active = true;
                route.updated_at = chrono::Utc::now().timestamp() as u64;
                true
            } else {
                false
            }
        };

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_splicer_lifecycle_and_buffering() {
        let mut splicer = ConnectionSplicer::new(
            "sess-1234",
            "192.168.1.100:54321",
            "10.0.0.1:25565",
        );
        assert!(!splicer.is_buffering());
        assert_eq!(splicer.buffered_count(), (0, 0));

        // Start freeze
        splicer.start_freeze();
        assert!(splicer.is_buffering());

        // Buffer packets
        splicer.buffer_upstream(vec![0x01, 0x02, 0x03]);
        splicer.buffer_upstream(vec![0x04, 0x05]);
        splicer.buffer_downstream(vec![0xAA, 0xBB]);

        assert_eq!(splicer.buffered_count(), (2, 1));
        assert_eq!(splicer.client_seq, 5);
        assert_eq!(splicer.server_seq, 2);

        // Generate handoff frame
        let frame = splicer.generate_handoff_frame(
            "10.0.0.2:25565",
            "00000000-0000-0000-0000-000000000001",
            "Steve",
            763,
            42,
            vec![0xDE, 0xAD],
        );

        assert_eq!(frame.session_id, "sess-1234");
        assert_eq!(frame.username, "Steve");
        assert_eq!(frame.new_target_addr, "10.0.0.2:25565");
        assert_eq!(frame.buffered_upstream_packets.len(), 2);
        assert_eq!(frame.buffered_downstream_packets.len(), 1);

        // Resume splice and drain
        let (up, down) = splicer.resume_splice();
        assert_eq!(up.len(), 2);
        assert_eq!(down.len(), 1);
        assert_eq!(up[0], vec![0x01, 0x02, 0x03]);
        assert_eq!(down[0], vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_wire_framing_encode_decode_roundtrip() {
        let chunk = MemoryPageChunk::from_raw(0x400000, b"dirty page content chunk test");
        let msg = MigrationWireMessage::PreCopyData {
            migration_id: "mig-test-roundtrip".to_string(),
            round: 2,
            chunk,
        };

        let encoded = encode_migration_message(&msg).expect("encoding must succeed");
        assert!(encoded.len() > 8);
        assert_eq!(&encoded[0..4], &CRAFT_MIGRATION_MAGIC);

        let decoded = decode_migration_message(&encoded).expect("decoding must succeed");
        let (recovered_msg, consumed) = decoded.expect("full message must be parsed");
        assert_eq!(consumed, encoded.len());

        match recovered_msg {
            MigrationWireMessage::PreCopyData { migration_id, round, chunk } => {
                assert_eq!(migration_id, "mig-test-roundtrip");
                assert_eq!(round, 2);
                let restored = chunk.decompress().unwrap();
                assert_eq!(restored, b"dirty page content chunk test");
            }
            _ => panic!("Unexpected decoded message variant"),
        }
    }

    #[test]
    fn test_bgp_route_generation() {
        let route = AnycastRouteAnnouncement::new("203.0.113.0/24", 65000);
        let bird = AnycastBgpEngine::generate_bird_config(&route, 65001, "192.0.2.1");
        assert!(bird.contains("protocol bgp craft_anycast_203_0_113_0_24"));
        assert!(bird.contains("local as 65001"));
        assert!(bird.contains("net = 203.0.113.0/24"));

        let frr = AnycastBgpEngine::generate_frr_config(&route, 65001, "192.0.2.1");
        assert!(frr.contains("router bgp 65001"));
        assert!(frr.contains("network 203.0.113.0/24"));

        let exa = AnycastBgpEngine::generate_exabgp_command(&route, "announce");
        assert!(exa.contains("neighbor all announce route 203.0.113.0/24"));

        let exa_del = AnycastBgpEngine::generate_exabgp_command(&route, "withdraw");
        assert_eq!(exa_del, "neighbor all withdraw route 203.0.113.0/24");
    }

    #[test]
    fn test_dynamic_health_steering() {
        let mut route = AnycastRouteAnnouncement::new("198.51.100.0/24", 65000);
        assert_eq!(route.status, AnycastRouteStatus::Announced);

        // Healthy load -> remains announced
        let changed = AnycastBgpEngine::evaluate_health_and_steer(50.0, 90.0, &mut route);
        assert!(!changed);
        assert_eq!(route.status, AnycastRouteStatus::Announced);

        // Approaching threshold (86% of 90.0 is > 85%) -> prepends path
        let changed = AnycastBgpEngine::evaluate_health_and_steer(80.0, 90.0, &mut route);
        assert!(changed);
        assert_eq!(route.status, AnycastRouteStatus::PrependPath);

        // Overloaded (92% >= 90%) -> withdrawn
        let changed = AnycastBgpEngine::evaluate_health_and_steer(92.0, 90.0, &mut route);
        assert!(changed);
        assert_eq!(route.status, AnycastRouteStatus::Withdrawn);
        assert!(!route.active);

        // Recovers to normal -> announced again
        let changed = AnycastBgpEngine::evaluate_health_and_steer(30.0, 90.0, &mut route);
        assert!(changed);
        assert_eq!(route.status, AnycastRouteStatus::Announced);
        assert!(route.active);
    }
}
