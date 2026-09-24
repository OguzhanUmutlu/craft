use craft_core::pqc::{
    MlDsa65, MlDsa65PublicKey, MlDsa65SecretKey, MlKem768, MlKem768PublicKey, MlKem1024,
    MlKem1024PublicKey, PqcCipherSuite, PqcEnforcementMode, PqcPolicy,
    HybridCiphertext, HybridKeyExchange, HybridPublicKey, HybridSecretKey,
};
use craft_core::{CraftError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CRAFT_PQC_MAGIC: [u8; 4] = [0x50, 0x51, 0x43, 0x53]; // "PQCS"
pub const CRAFT_PQC_VERSION: u32 = 1;

/// Handshake proposal emitted by client node
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcHandshakeProposal {
    pub protocol_version: u32,
    pub client_id: String,
    pub cipher_suites: Vec<PqcCipherSuite>,
    pub ephemeral_hybrid_pk: Option<HybridPublicKey>,
    pub ephemeral_mlkem768_pk: Option<MlKem768PublicKey>,
    pub ephemeral_mlkem1024_pk: Option<MlKem1024PublicKey>,
    pub timestamp_nanos: u64,
    pub nonce: [u8; 32],
}

impl PqcHandshakeProposal {
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(&self.protocol_version.to_le_bytes());
        hasher.update(self.client_id.as_bytes());
        for suite in &self.cipher_suites {
            hasher.update(suite.to_string().as_bytes());
        }
        hasher.update(&self.timestamp_nanos.to_le_bytes());
        hasher.update(&self.nonce);
        hasher.finalize().to_vec()
    }
}

/// Handshake response emitted by server node
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcHandshakeResponse {
    pub protocol_version: u32,
    pub server_id: String,
    pub selected_suite: PqcCipherSuite,
    pub ciphertext: Vec<u8>,
    pub timestamp_nanos: u64,
    pub signature: Vec<u8>,
}

impl PqcHandshakeResponse {
    pub fn signable_bytes(&self, proposal_nonce: &[u8; 32]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(&self.protocol_version.to_le_bytes());
        hasher.update(self.server_id.as_bytes());
        hasher.update(self.selected_suite.to_string().as_bytes());
        hasher.update(&self.ciphertext);
        hasher.update(&self.timestamp_nanos.to_le_bytes());
        hasher.update(proposal_nonce);
        hasher.finalize().to_vec()
    }
}

/// Established post-quantum or hybrid cryptographic session
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcSessionContext {
    pub session_id: String,
    pub cipher_suite: PqcCipherSuite,
    pub shared_secret: [u8; 32],
    pub established_at: u64,
    pub peer_id: String,
    pub post_quantum_secure: bool,
}

/// State tracking for client initiating a handshake
pub struct PqcClientHandshake {
    pub client_id: String,
    pub cipher_suites: Vec<PqcCipherSuite>,
    pub nonce: [u8; 32],
    pub ephemeral_hybrid_sk: Option<HybridSecretKey>,
    pub ephemeral_mlkem768_sk: Option<Vec<u8>>,
    pub ephemeral_mlkem1024_sk: Option<Vec<u8>>,
}

impl PqcClientHandshake {
    pub fn initiate(
        client_id: &str,
        preferred_suites: Vec<PqcCipherSuite>,
    ) -> (Self, PqcHandshakeProposal) {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let mut nonce = [0u8; 32];
        let mut nh = Sha256::new();
        nh.update(b"CRAFT-PQC-CLIENT-NONCE");
        nh.update(client_id.as_bytes());
        nh.update(&now_nanos.to_le_bytes());
        nonce.copy_from_slice(&nh.finalize());

        let mut hybrid_pk = None;
        let mut hybrid_sk = None;
        let mut ml768_pk = None;
        let mut ml768_sk = None;
        let mut ml1024_pk = None;
        let mut ml1024_sk = None;

        for suite in &preferred_suites {
            match suite {
                PqcCipherSuite::HybridX25519MlKem768 => {
                    let (pk, sk) = HybridKeyExchange::keypair(None);
                    hybrid_pk = Some(pk);
                    hybrid_sk = Some(sk);
                }
                PqcCipherSuite::PureMlKem768 => {
                    let (pk, sk) = MlKem768::keypair(None);
                    ml768_pk = Some(pk);
                    ml768_sk = Some(sk.raw);
                }
                PqcCipherSuite::PureMlKem1024 => {
                    let (pk, sk) = MlKem1024::keypair(None);
                    ml1024_pk = Some(pk);
                    ml1024_sk = Some(sk.raw);
                }
                _ => {}
            }
        }

        let proposal = PqcHandshakeProposal {
            protocol_version: CRAFT_PQC_VERSION,
            client_id: client_id.to_string(),
            cipher_suites: preferred_suites.clone(),
            ephemeral_hybrid_pk: hybrid_pk,
            ephemeral_mlkem768_pk: ml768_pk,
            ephemeral_mlkem1024_pk: ml1024_pk,
            timestamp_nanos: now_nanos,
            nonce,
        };

        let state = Self {
            client_id: client_id.to_string(),
            cipher_suites: preferred_suites,
            nonce,
            ephemeral_hybrid_sk: hybrid_sk,
            ephemeral_mlkem768_sk: ml768_sk,
            ephemeral_mlkem1024_sk: ml1024_sk,
        };

        (state, proposal)
    }

    pub fn complete(
        self,
        response: &PqcHandshakeResponse,
        server_mldsa_pk: &MlDsa65PublicKey,
        mode: PqcEnforcementMode,
    ) -> Result<PqcSessionContext> {
        if response.protocol_version != CRAFT_PQC_VERSION {
            return Err(CraftError::Config(format!(
                "Incompatible PQC protocol version: expected {}, got {}",
                CRAFT_PQC_VERSION, response.protocol_version
            )));
        }

        // Verify enforcement mode permits this suite
        if !mode.is_suite_permitted(response.selected_suite) {
            return Err(CraftError::Config(format!(
                "Security policy violation: suite {} is prohibited in enforcement mode {:?}",
                response.selected_suite, mode
            )));
        }

        // Verify server's digital signature over response
        let signable = response.signable_bytes(&self.nonce);
        if !MlDsa65::verify(server_mldsa_pk, &signable, &response.signature) {
            return Err(CraftError::Config(
                "PQC handshake verification failed: invalid ML-DSA-65 signature on response"
                    .to_string(),
            ));
        }

        let shared_secret = match response.selected_suite {
            PqcCipherSuite::HybridX25519MlKem768 => {
                let sk = self.ephemeral_hybrid_sk.ok_or_else(|| {
                    CraftError::Config("Missing hybrid ephemeral secret key".to_string())
                })?;
                let ct: HybridCiphertext = serde_json::from_slice(&response.ciphertext)
                    .map_err(|e| CraftError::Config(format!("Invalid hybrid ciphertext: {}", e)))?;
                HybridKeyExchange::decapsulate(&sk, &ct)?
            }
            PqcCipherSuite::PureMlKem768 => {
                let sk_bytes = self.ephemeral_mlkem768_sk.ok_or_else(|| {
                    CraftError::Config("Missing ML-KEM-768 secret key".to_string())
                })?;
                let sk = craft_core::pqc::MlKem768SecretKey { raw: sk_bytes };
                MlKem768::decapsulate(&sk, &response.ciphertext)?
            }
            PqcCipherSuite::PureMlKem1024 => {
                let sk_bytes = self.ephemeral_mlkem1024_sk.ok_or_else(|| {
                    CraftError::Config("Missing ML-KEM-1024 secret key".to_string())
                })?;
                let sk = craft_core::pqc::MlKem1024SecretKey { raw: sk_bytes };
                MlKem1024::decapsulate(&sk, &response.ciphertext)?
            }
            PqcCipherSuite::ClassicX25519 => {
                return Err(CraftError::Config(
                    "Pure classic X25519 without quantum encapsulation rejected".to_string(),
                ));
            }
        };

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session_id = format!(
            "pqc-{}-{}",
            &hex::encode(&self.nonce[..8]),
            &hex::encode(&shared_secret[..8])
        );

        Ok(PqcSessionContext {
            session_id,
            cipher_suite: response.selected_suite,
            shared_secret,
            established_at: now_sec,
            peer_id: response.server_id.clone(),
            post_quantum_secure: response.selected_suite.is_quantum_resistant(),
        })
    }
}

/// Server endpoint handling inbound PQC handshake proposals
pub struct PqcServerTransport {
    pub server_id: String,
    pub server_mldsa_pk: MlDsa65PublicKey,
    pub server_mldsa_sk: MlDsa65SecretKey,
    pub policy: PqcPolicy,
}

impl PqcServerTransport {
    pub fn new(
        server_id: &str,
        mldsa_pk: MlDsa65PublicKey,
        mldsa_sk: MlDsa65SecretKey,
        policy: PqcPolicy,
    ) -> Self {
        Self {
            server_id: server_id.to_string(),
            server_mldsa_pk: mldsa_pk,
            server_mldsa_sk: mldsa_sk,
            policy,
        }
    }

    pub fn handle_proposal(
        &self,
        proposal: &PqcHandshakeProposal,
    ) -> Result<(PqcHandshakeResponse, PqcSessionContext)> {
        if proposal.protocol_version != CRAFT_PQC_VERSION {
            return Err(CraftError::Config(format!(
                "Incompatible PQC protocol version: proposal version {}, supported {}",
                proposal.protocol_version, CRAFT_PQC_VERSION
            )));
        }

        // Select suite in priority order that is permitted by server policy
        let selected_suite = proposal
            .cipher_suites
            .iter()
            .find(|suite| self.policy.enforcement_mode.is_suite_permitted(**suite))
            .copied()
            .ok_or_else(|| {
                CraftError::Config(format!(
                    "No mutually acceptable cipher suites. Peer offered {:?}, server mode is {:?}",
                    proposal.cipher_suites, self.policy.enforcement_mode
                ))
            })?;

        let (ct_bytes, shared_secret) = match selected_suite {
            PqcCipherSuite::HybridX25519MlKem768 => {
                let pk = proposal.ephemeral_hybrid_pk.as_ref().ok_or_else(|| {
                    CraftError::Config("Proposal missing ephemeral hybrid public key".to_string())
                })?;
                let (ct, ss) = HybridKeyExchange::encapsulate(pk)?;
                let ct_ser = serde_json::to_vec(&ct).map_err(|e| {
                    CraftError::Config(format!("Failed to serialize hybrid ciphertext: {}", e))
                })?;
                (ct_ser, ss)
            }
            PqcCipherSuite::PureMlKem768 => {
                let pk = proposal.ephemeral_mlkem768_pk.as_ref().ok_or_else(|| {
                    CraftError::Config(
                        "Proposal missing ephemeral ML-KEM-768 public key".to_string(),
                    )
                })?;
                let (ct, ss) = MlKem768::encapsulate(pk, None)?;
                (ct, ss)
            }
            PqcCipherSuite::PureMlKem1024 => {
                let pk = proposal.ephemeral_mlkem1024_pk.as_ref().ok_or_else(|| {
                    CraftError::Config(
                        "Proposal missing ephemeral ML-KEM-1024 public key".to_string(),
                    )
                })?;
                let (ct, ss) = MlKem1024::encapsulate(pk, None)?;
                (ct, ss)
            }
            PqcCipherSuite::ClassicX25519 => {
                return Err(CraftError::Config(
                    "Server security policy prohibits non-quantum-resistant cipher suite"
                        .to_string(),
                ));
            }
        };

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let mut response = PqcHandshakeResponse {
            protocol_version: CRAFT_PQC_VERSION,
            server_id: self.server_id.clone(),
            selected_suite,
            ciphertext: ct_bytes,
            timestamp_nanos: now_nanos,
            signature: Vec::new(),
        };

        let signable = response.signable_bytes(&proposal.nonce);
        let sig = MlDsa65::sign(&self.server_mldsa_sk, &signable);
        response.signature = sig;

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session_id = format!(
            "pqc-{}-{}",
            &hex::encode(&proposal.nonce[..8]),
            &hex::encode(&shared_secret[..8])
        );

        let context = PqcSessionContext {
            session_id,
            cipher_suite: selected_suite,
            shared_secret,
            established_at: now_sec,
            peer_id: proposal.client_id.clone(),
            post_quantum_secure: selected_suite.is_quantum_resistant(),
        };

        Ok((response, context))
    }
}

// ============================================================================
// Wire Framing Framing: [MAGIC: 4 bytes][PAYLOAD_LEN: 4 bytes BE][JSON]
// ============================================================================

pub fn encode_pqc_proposal(proposal: &PqcHandshakeProposal) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(proposal)
        .map_err(|e| CraftError::Config(format!("Failed to encode PQC proposal: {}", e)))?;
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&CRAFT_PQC_MAGIC);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_pqc_proposal(buf: &[u8]) -> Result<PqcHandshakeProposal> {
    if buf.len() < 8 {
        return Err(CraftError::Config("Buffer too small for PQC header".to_string()));
    }
    if &buf[..4] != CRAFT_PQC_MAGIC {
        return Err(CraftError::Config("Invalid PQC wire magic bytes".to_string()));
    }
    let len = u32::from_be_bytes(buf[4..8].try_into().unwrap()) as usize;
    if buf.len() < 8 + len {
        return Err(CraftError::Config("Truncated PQC proposal message".to_string()));
    }
    serde_json::from_slice(&buf[8..8 + len])
        .map_err(|e| CraftError::Config(format!("Failed to decode PQC proposal: {}", e)))
}

pub fn encode_pqc_response(response: &PqcHandshakeResponse) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| CraftError::Config(format!("Failed to encode PQC response: {}", e)))?;
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&CRAFT_PQC_MAGIC);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_pqc_response(buf: &[u8]) -> Result<PqcHandshakeResponse> {
    if buf.len() < 8 {
        return Err(CraftError::Config("Buffer too small for PQC header".to_string()));
    }
    if &buf[..4] != CRAFT_PQC_MAGIC {
        return Err(CraftError::Config("Invalid PQC wire magic bytes".to_string()));
    }
    let len = u32::from_be_bytes(buf[4..8].try_into().unwrap()) as usize;
    if buf.len() < 8 + len {
        return Err(CraftError::Config("Truncated PQC response message".to_string()));
    }
    serde_json::from_slice(&buf[8..8 + len])
        .map_err(|e| CraftError::Config(format!("Failed to decode PQC response: {}", e)))
}

/// Node post-quantum hybrid certificate representation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcNodeCertBundle {
    pub node_id: String,
    pub mldsa_public_key: Vec<u8>,
    pub hybrid_public_key: Vec<u8>,
    pub ca_signature: Vec<u8>,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pqc_handshake_roundtrip() {
        let (server_mldsa_pk, server_mldsa_sk) = MlDsa65::keypair(None);
        let server = PqcServerTransport::new(
            "node-alpha-us-east",
            server_mldsa_pk.clone(),
            server_mldsa_sk,
            PqcPolicy::default(),
        );

        let (client, proposal) = PqcClientHandshake::initiate(
            "node-beta-eu-central",
            vec![
                PqcCipherSuite::HybridX25519MlKem768,
                PqcCipherSuite::PureMlKem1024,
            ],
        );

        // Framing test
        let wire_proposal = encode_pqc_proposal(&proposal).unwrap();
        let decoded_proposal = decode_pqc_proposal(&wire_proposal).unwrap();
        assert_eq!(proposal, decoded_proposal);

        let (response, server_ctx) = server.handle_proposal(&decoded_proposal).unwrap();

        let wire_response = encode_pqc_response(&response).unwrap();
        let decoded_response = decode_pqc_response(&wire_response).unwrap();
        assert_eq!(response, decoded_response);

        let client_ctx = client
            .complete(
                &decoded_response,
                &server_mldsa_pk,
                PqcEnforcementMode::Hybrid,
            )
            .unwrap();

        assert_eq!(client_ctx.shared_secret, server_ctx.shared_secret);
        assert_eq!(client_ctx.cipher_suite, PqcCipherSuite::HybridX25519MlKem768);
        assert!(client_ctx.post_quantum_secure);
    }

    #[test]
    fn test_pqc_downgrade_attack_rejection() {
        let (server_mldsa_pk, server_mldsa_sk) = MlDsa65::keypair(None);
        let mut policy = PqcPolicy::default();
        policy.enforcement_mode = PqcEnforcementMode::PostQuantumOnly;
        let server = PqcServerTransport::new(
            "node-alpha-us-east",
            server_mldsa_pk,
            server_mldsa_sk,
            policy,
        );

        let (_, proposal) = PqcClientHandshake::initiate(
            "adversary-node",
            vec![PqcCipherSuite::ClassicX25519],
        );

        let result = server.handle_proposal(&proposal);
        assert!(result.is_err(), "Server must reject non-PQC suite under PostQuantumOnly mode");
    }

    #[test]
    fn test_pqc_tampered_signature_rejection() {
        let (server_mldsa_pk, server_mldsa_sk) = MlDsa65::keypair(None);
        let server = PqcServerTransport::new(
            "node-alpha-us-east",
            server_mldsa_pk.clone(),
            server_mldsa_sk,
            PqcPolicy::default(),
        );

        let (client, proposal) = PqcClientHandshake::initiate(
            "node-beta-eu-central",
            vec![PqcCipherSuite::HybridX25519MlKem768],
        );

        let (mut response, _) = server.handle_proposal(&proposal).unwrap();
        // Tamper with ciphertext
        if let Some(byte) = response.ciphertext.get_mut(10) {
            *byte ^= 0x5a;
        }

        let result = client.complete(&response, &server_mldsa_pk, PqcEnforcementMode::Hybrid);
        assert!(result.is_err(), "Client must reject response with invalid signature");
    }
}
