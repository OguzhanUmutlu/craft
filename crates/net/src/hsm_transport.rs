use craft_core::hsm::{
    verify_pcr_quote, HsmEngine, HsmKeyHandle, PcrQuote, TpmPcrBank, ZkMembershipEngine,
    ZkMembershipProof,
};
use craft_core::{CraftError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CRAFT_HSM_MAGIC: [u8; 4] = [0x48, 0x53, 0x4D, 0x53]; // "HSMS"
pub const CRAFT_HSM_VERSION: u32 = 1;

pub const FRAME_TYPE_ATTESTATION_CHALLENGE: u8 = 0x01;
pub const FRAME_TYPE_ATTESTATION_RESPONSE: u8 = 0x02;
pub const FRAME_TYPE_ZK_MEMBERSHIP_CHALLENGE: u8 = 0x03;
pub const FRAME_TYPE_ZK_MEMBERSHIP_RESPONSE: u8 = 0x04;
pub const FRAME_TYPE_SIGNED_ENVELOPE: u8 = 0x05;

/// Header for binary HSM wire transport
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HsmFrameHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub frame_type: u8,
    pub payload_len: u32,
}

/// Enclave Attestation Challenge emitted by the verifier / cluster coordinator
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationChallenge {
    pub nonce: [u8; 32],
    pub pcr_mask: u32,
    pub cluster_id: String,
    pub timestamp_epoch: u64,
}

/// Enclave Attestation Response emitted by the prover node
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationResponse {
    pub node_id: String,
    pub quote: PcrQuote,
    pub verified_secure_boot: bool,
}

/// Zero-Knowledge Membership Challenge emitted by the cluster coordinator
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkMembershipChallenge {
    pub nonce: [u8; 32],
    pub cluster_id: String,
    pub timestamp_epoch: u64,
}

/// Zero-Knowledge Membership Response emitted by joining or communicating node
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkMembershipResponse {
    pub node_id: String,
    pub proof: ZkMembershipProof,
}

/// Hardware-Signed Data Envelope for tamper-evident transit
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmSignedEnvelope {
    pub payload: Vec<u8>,
    pub key_handle_id: String,
    pub signature: Vec<u8>,
    pub timestamp_epoch: u64,
}

/// Encodes an HSM wire frame:
/// [MAGIC (4 bytes) | VERSION (4 bytes) | FRAME_TYPE (1 byte) | LENGTH (4 bytes) | PAYLOAD (N bytes)]
pub fn encode_hsm_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(13 + payload.len());
    buf.extend_from_slice(&CRAFT_HSM_MAGIC);
    buf.extend_from_slice(&CRAFT_HSM_VERSION.to_le_bytes());
    buf.push(frame_type);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Decodes an HSM wire frame from raw bytes
pub fn decode_hsm_frame(bytes: &[u8]) -> Result<(u8, Vec<u8>)> {
    if bytes.len() < 13 {
        return Err(CraftError::Other(format!(
            "HSM frame too short: {} bytes (minimum 13)",
            bytes.len()
        )));
    }

    if &bytes[0..4] != &CRAFT_HSM_MAGIC {
        return Err(CraftError::Other(format!(
            "Invalid HSM magic: {:?} (expected {:?})",
            &bytes[0..4],
            CRAFT_HSM_MAGIC
        )));
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != CRAFT_HSM_VERSION {
        return Err(CraftError::Other(format!(
            "Unsupported HSM protocol version: {} (supported: {})",
            version, CRAFT_HSM_VERSION
        )));
    }

    let frame_type = bytes[8];
    let payload_len = u32::from_le_bytes(bytes[9..13].try_into().unwrap()) as usize;

    if bytes.len() < 13 + payload_len {
        return Err(CraftError::Other(format!(
            "HSM frame payload truncated: expected {} bytes, got {}",
            payload_len,
            bytes.len() - 13
        )));
    }

    let payload = bytes[13..13 + payload_len].to_vec();
    Ok((frame_type, payload))
}

/// Verifier and coordinator for hardware attestation and zero-knowledge cluster membership
pub struct HsmTransportVerifier {
    zk_engine: ZkMembershipEngine,
}

impl Default for HsmTransportVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl HsmTransportVerifier {
    pub fn new() -> Self {
        Self {
            zk_engine: ZkMembershipEngine::new(),
        }
    }

    /// Creates a fresh attestation challenge with a 32-byte cryptographic nonce
    pub fn create_attestation_challenge(
        &self,
        cluster_id: &str,
        pcr_mask: u32,
    ) -> (AttestationChallenge, [u8; 32]) {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT_HSM_ATTEST_NONCE");
        hasher.update(cluster_id.as_bytes());
        hasher.update(&now_nanos.to_le_bytes());
        let nonce: [u8; 32] = hasher.finalize().into();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let challenge = AttestationChallenge {
            nonce,
            pcr_mask,
            cluster_id: cluster_id.to_string(),
            timestamp_epoch: now,
        };

        (challenge, nonce)
    }

    /// Verifies an attestation response against the expected fresh nonce and baseline PCR bank
    pub fn verify_attestation_response(
        &self,
        expected_nonce: &[u8; 32],
        response: &AttestationResponse,
        bank: &TpmPcrBank,
    ) -> Result<bool> {
        verify_pcr_quote(&response.quote, expected_nonce, bank)
    }

    /// Creates a fresh zero-knowledge membership challenge with a 32-byte cryptographic nonce
    pub fn create_zk_challenge(&self, cluster_id: &str) -> (ZkMembershipChallenge, [u8; 32]) {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT_HSM_ZK_NONCE");
        hasher.update(cluster_id.as_bytes());
        hasher.update(&now_nanos.to_le_bytes());
        let nonce: [u8; 32] = hasher.finalize().into();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let challenge = ZkMembershipChallenge {
            nonce,
            cluster_id: cluster_id.to_string(),
            timestamp_epoch: now,
        };

        (challenge, nonce)
    }

    /// Verifies a zero-knowledge membership response
    pub fn verify_zk_response(
        &self,
        expected_nonce: &[u8; 32],
        response: &ZkMembershipResponse,
    ) -> Result<bool> {
        let proof_nonce = hex::decode(&response.proof.nonce)
            .map_err(|e| CraftError::Config(format!("Invalid hex nonce in ZKP response: {}", e)))?;
        if proof_nonce.as_slice() != expected_nonce {
            return Ok(false);
        }
        self.zk_engine.verify_proof(&response.proof)
    }

    /// Verifies a hardware signed envelope
    pub fn verify_signed_envelope(
        &self,
        envelope: &HsmSignedEnvelope,
        key: &HsmKeyHandle,
        engine: &HsmEngine,
    ) -> bool {
        if envelope.key_handle_id != key.id {
            return false;
        }
        engine.verify(key, &envelope.payload, &envelope.signature)
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::hsm::{HsmKeyType, U256};

    #[test]
    fn test_wire_framing_roundtrip() {
        let payload = b"craft-enclave-attestation-payload-bytes";
        let framed = encode_hsm_frame(FRAME_TYPE_ATTESTATION_CHALLENGE, payload);
        assert_eq!(&framed[0..4], &CRAFT_HSM_MAGIC);
        assert_eq!(framed[8], FRAME_TYPE_ATTESTATION_CHALLENGE);

        let (frame_type, decoded) = decode_hsm_frame(&framed).unwrap();
        assert_eq!(frame_type, FRAME_TYPE_ATTESTATION_CHALLENGE);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_wire_framing_tamper_rejection() {
        let payload = b"hello";
        let mut framed = encode_hsm_frame(FRAME_TYPE_SIGNED_ENVELOPE, payload);
        // Tamper magic byte
        framed[0] = 0xFF;
        let res = decode_hsm_frame(&framed);
        assert!(res.is_err());
    }

    #[test]
    fn test_attestation_handshake_flow() {
        let engine = HsmEngine::new();
        let verifier = HsmTransportVerifier::new();

        let (challenge, nonce) = verifier.create_attestation_challenge("cluster-main", 0b111);
        let quote = engine.attest_pcr(challenge.pcr_mask, &challenge.nonce).unwrap();

        let response = AttestationResponse {
            node_id: "node-worker-01".to_string(),
            quote,
            verified_secure_boot: true,
        };

        // Verification passes
        let valid = verifier
            .verify_attestation_response(&nonce, &response, engine.pcr_bank())
            .unwrap();
        assert!(valid);

        // Verification with forged nonce fails
        let forged_nonce = [99u8; 32];
        let invalid = verifier
            .verify_attestation_response(&forged_nonce, &response, engine.pcr_bank())
            .unwrap();
        assert!(!invalid);
    }

    #[test]
    fn test_zk_membership_handshake_flow() {
        let engine = HsmEngine::new();
        let verifier = HsmTransportVerifier::new();
        let cluster_id = "cluster-secure-mesh";

        let (challenge, nonce) = verifier.create_zk_challenge(cluster_id);
        let secret_s = U256::from_u64(888123);
        let blinding_r = U256::from_u64(999456);

        let proof = engine
            .prove_zk_membership(cluster_id, &secret_s, &blinding_r, &challenge.nonce)
            .unwrap();

        let response = ZkMembershipResponse {
            node_id: "node-anonymous-42".to_string(),
            proof,
        };

        // Verification passes
        let valid = verifier.verify_zk_response(&nonce, &response).unwrap();
        assert!(valid);

        // Tampered nonce in response fails
        let forged_nonce = [11u8; 32];
        let invalid = verifier.verify_zk_response(&forged_nonce, &response).unwrap();
        assert!(!invalid);
    }

    #[test]
    fn test_signed_envelope_flow() {
        let mut engine = HsmEngine::new();
        let verifier = HsmTransportVerifier::new();
        let key = engine
            .generate_key(0, "envelope-signer", HsmKeyType::Ed25519)
            .unwrap();

        let payload = b"cluster-configuration-update-v2".to_vec();
        let signature = engine.sign(&key, &payload).unwrap();

        let envelope = HsmSignedEnvelope {
            payload: payload.clone(),
            key_handle_id: key.id.clone(),
            signature,
            timestamp_epoch: 1234567,
        };

        assert!(verifier.verify_signed_envelope(&envelope, &key, &engine));

        let mut tampered = envelope.clone();
        tampered.payload = b"forged-payload".to_vec();
        assert!(!verifier.verify_signed_envelope(&tampered, &key, &engine));
    }
}
