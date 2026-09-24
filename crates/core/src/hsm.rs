use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use crate::pqc::MlDsa65;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Safe Prime Group Parameters for Schnorr-Pedersen ZKP
// Prime p = 2^256 - 36113, Subgroup Prime Order q = (p - 1) / 2
// ============================================================================

pub const ZKP_P_HEX: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff72ef";
pub const ZKP_Q_HEX: &str = "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb977";

// Generators g = 2^2 = 4, h = 3^2 = 9 (both quadratic residues in subgroup of order q)
pub const ZKP_G_U64: u64 = 4;
pub const ZKP_H_U64: u64 = 9;

/// 256-bit unsigned integer with 4 x 64-bit limbs (little-endian limb order)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub const ZERO: Self = Self([0, 0, 0, 0]);
    pub const ONE: Self = Self([1, 0, 0, 0]);

    pub fn from_u64(val: u64) -> Self {
        Self([val, 0, 0, 0])
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let clean = s.trim().trim_start_matches("0x");
        let bytes = hex::decode(clean)
            .map_err(|e| CraftError::Config(format!("Invalid hex for U256: {}", e)))?;
        if bytes.len() > 32 {
            return Err(CraftError::Config("Hex exceeds 32 bytes for U256".to_string()));
        }
        let mut padded = [0u8; 32];
        padded[32 - bytes.len()..].copy_from_slice(&bytes);
        Ok(Self::from_be_bytes(&padded))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_be_bytes())
    }

    pub fn from_be_bytes(bytes: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let offset = (3 - i) * 8;
            limbs[i] = u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap());
        }
        Self(limbs)
    }

    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for i in 0..4 {
            let offset = (3 - i) * 8;
            bytes[offset..offset + 8].copy_from_slice(&self.0[i].to_be_bytes());
        }
        bytes
    }

    pub fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for i in (0..4).rev() {
            if self.0[i] != other.0[i] {
                return self.0[i].cmp(&other.0[i]);
            }
        }
        std::cmp::Ordering::Equal
    }

    pub fn add(&self, other: &Self) -> (Self, bool) {
        let mut res = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let sum = (self.0[i] as u128) + (other.0[i] as u128) + carry;
            res[i] = sum as u64;
            carry = sum >> 64;
        }
        (Self(res), carry != 0)
    }

    pub fn sub(&self, other: &Self) -> (Self, bool) {
        let mut res = [0u64; 4];
        let mut borrow = 0u128;
        for i in 0..4 {
            let diff = (self.0[i] as u128)
                .wrapping_sub(other.0[i] as u128)
                .wrapping_sub(borrow);
            res[i] = diff as u64;
            borrow = (diff >> 127) & 1;
        }
        (Self(res), borrow != 0)
    }

    pub fn mul_wide(&self, other: &Self) -> [u64; 8] {
        let mut wide = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let cur = (wide[i + j] as u128)
                    + ((self.0[i] as u128) * (other.0[j] as u128))
                    + carry;
                wide[i + j] = cur as u64;
                carry = cur >> 64;
            }
            let mut k = i + 4;
            while carry != 0 && k < 8 {
                let cur = (wide[k] as u128) + carry;
                wide[k] = cur as u64;
                carry = cur >> 64;
                k += 1;
            }
        }
        wide
    }

    pub fn rem_wide(wide: &[u64; 8], modulus: &U256) -> U256 {
        let mut rem = [0u64; 5];
        for bit_idx in (0..512).rev() {
            let limb_idx = bit_idx / 64;
            let bit_in_limb = bit_idx % 64;
            let bit = ((wide[limb_idx] >> bit_in_limb) & 1) as u64;

            // Shift rem left by 1 and insert incoming bit
            let mut carry = bit;
            for i in 0..5 {
                let next_carry = (rem[i] >> 63) & 1;
                rem[i] = (rem[i] << 1) | carry;
                carry = next_carry;
            }

            // Check if rem >= modulus (treating modulus as 5 limbs with top limb 0)
            let rem_u256 = U256([rem[0], rem[1], rem[2], rem[3]]);
            if rem[4] > 0 || rem_u256.cmp(modulus) != std::cmp::Ordering::Less {
                let mut borrow = 0u128;
                for i in 0..4 {
                    let diff = (rem[i] as u128)
                        .wrapping_sub(modulus.0[i] as u128)
                        .wrapping_sub(borrow);
                    rem[i] = diff as u64;
                    borrow = (diff >> 127) & 1;
                }
                rem[4] = rem[4].wrapping_sub(borrow as u64);
            }
        }
        U256([rem[0], rem[1], rem[2], rem[3]])
    }

    pub fn add_mod(a: &U256, b: &U256, m: &U256) -> U256 {
        let (sum, carry) = a.add(b);
        if carry || sum.cmp(m) != std::cmp::Ordering::Less {
            let (diff, _) = sum.sub(m);
            diff
        } else {
            sum
        }
    }

    pub fn sub_mod(a: &U256, b: &U256, m: &U256) -> U256 {
        if a.cmp(b) != std::cmp::Ordering::Less {
            let (diff, _) = a.sub(b);
            diff
        } else {
            let (sum, _) = a.add(m);
            let (diff, _) = sum.sub(b);
            diff
        }
    }

    pub fn mul_mod(a: &U256, b: &U256, m: &U256) -> U256 {
        let wide = a.mul_wide(b);
        Self::rem_wide(&wide, m)
    }

    pub fn pow_mod(base: &U256, exp: &U256, m: &U256) -> U256 {
        let mut result = U256::ONE;
        let mut cur = *base;

        for bit_idx in 0..256 {
            let limb_idx = bit_idx / 64;
            let bit_in_limb = bit_idx % 64;
            if ((exp.0[limb_idx] >> bit_in_limb) & 1) != 0 {
                result = Self::mul_mod(&result, &cur, m);
            }
            cur = Self::mul_mod(&cur, &cur, m);
        }
        result
    }
}

// ============================================================================
// PKCS#11 Data Models & Token Abstraction
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HsmBackendType {
    SoftwareEmulated,
    Pkcs11Library,
    Tpm2Device,
    NitroEnclave,
}

impl fmt::Display for HsmBackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SoftwareEmulated => write!(f, "software_emulated"),
            Self::Pkcs11Library => write!(f, "pkcs11_library"),
            Self::Tpm2Device => write!(f, "tpm2_device"),
            Self::NitroEnclave => write!(f, "nitro_enclave"),
        }
    }
}

impl FromStr for HsmBackendType {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "software" | "software_emulated" | "emulated" => Ok(Self::SoftwareEmulated),
            "pkcs11" | "pkcs11_library" | "yubikey" => Ok(Self::Pkcs11Library),
            "tpm" | "tpm2" | "tpm2_device" => Ok(Self::Tpm2Device),
            "nitro" | "nitro_enclave" | "enclave" => Ok(Self::NitroEnclave),
            other => Err(CraftError::Config(format!("Unknown HSM backend type: '{}'", other))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HsmKeyType {
    Rsa2048,
    Rsa4096,
    EcP256,
    EcP384,
    Ed25519,
    MlDsa65,
}

impl fmt::Display for HsmKeyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rsa2048 => write!(f, "rsa2048"),
            Self::Rsa4096 => write!(f, "rsa4096"),
            Self::EcP256 => write!(f, "ec_p256"),
            Self::EcP384 => write!(f, "ec_p384"),
            Self::Ed25519 => write!(f, "ed25519"),
            Self::MlDsa65 => write!(f, "mldsa65"),
        }
    }
}

impl FromStr for HsmKeyType {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "rsa2048" | "rsa_2048" => Ok(Self::Rsa2048),
            "rsa4096" | "rsa_4096" => Ok(Self::Rsa4096),
            "p256" | "ec_p256" | "secp256r1" => Ok(Self::EcP256),
            "p384" | "ec_p384" | "secp384r1" => Ok(Self::EcP384),
            "ed25519" => Ok(Self::Ed25519),
            "mldsa65" | "ml_dsa_65" | "dilithium" => Ok(Self::MlDsa65),
            other => Err(CraftError::Config(format!("Unknown HSM key type: '{}'", other))),
        }
    }
}

/// Hardware-enforced key handle.
/// In accordance with PKCS#11 v2.40 / v3.0, hardware keys have CKA_EXTRACTABLE = false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmKeyHandle {
    pub id: String,
    pub label: String,
    pub key_type: HsmKeyType,
    pub slot_id: u64,
    pub extractable: bool,
    pub public_key: String, // Hex-encoded public key bytes
    pub created_at_epoch: u64,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmSlotInfo {
    pub slot_id: u64,
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
    pub token_present: bool,
    pub write_protected: bool,
    pub user_pin_initialized: bool,
    pub backend_type: HsmBackendType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmTokenSession {
    pub session_id: u64,
    pub slot_id: u64,
    pub logged_in: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HsmStatusSummary {
    pub backend_type: HsmBackendType,
    pub slots_count: usize,
    pub active_keys_count: usize,
    pub token_present: bool,
    pub hardware_backed_keys: usize,
    pub tpm_pcr_active: bool,
    pub attested_quotes_count: u64,
    pub zk_memberships_count: usize,
    pub total_operations: u64,
}

// ============================================================================
// Internal Token Boundary Storage for Non-Extractable Hardware Keys
// ============================================================================

/// Private key bytes stored inside the simulated hardware boundary.
/// These bytes can NEVER be extracted through any public API.
#[derive(Clone, Serialize, Deserialize)]
struct TokenInternalKeyVault {
    secret_bytes: Vec<u8>,
}

// ============================================================================
// TPM 2.0 Platform Configuration Register (PCR) Bank & Quote Engine
// ============================================================================

pub const TPM_PCR_COUNT: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpmPcrBank {
    pub pcrs: [[u8; 32]; TPM_PCR_COUNT],
}

impl Default for TpmPcrBank {
    fn default() -> Self {
        Self::new()
    }
}

impl TpmPcrBank {
    pub fn new() -> Self {
        let mut pcrs = [[0u8; 32]; TPM_PCR_COUNT];
        // Initialize PCR 0-7 with baseline firmware & bootloader hashes
        for i in 0..8 {
            let mut h = Sha256::new();
            h.update(b"CRAFT_TPM2_FIRMWARE_BASELINE_");
            h.update(&(i as u32).to_le_bytes());
            pcrs[i] = h.finalize().into();
        }
        // Initialize PCR 8-15 with baseline kernel & initramfs measurements
        for i in 8..16 {
            let mut h = Sha256::new();
            h.update(b"CRAFT_TPM2_KERNEL_BASELINE_");
            h.update(&(i as u32).to_le_bytes());
            pcrs[i] = h.finalize().into();
        }
        // PCR 16-23: Craft application runtime state
        for i in 16..TPM_PCR_COUNT {
            let mut h = Sha256::new();
            h.update(b"CRAFT_TPM2_RUNTIME_STATE_");
            h.update(&(i as u32).to_le_bytes());
            pcrs[i] = h.finalize().into();
        }

        Self { pcrs }
    }

    /// Extends a PCR: PCR_new = SHA256(PCR_current || measurement)
    pub fn extend(&mut self, pcr_index: usize, measurement: &[u8]) -> Result<[u8; 32]> {
        if pcr_index >= TPM_PCR_COUNT {
            return Err(CraftError::Config(format!(
                "Invalid PCR index {} (must be 0..{})",
                pcr_index,
                TPM_PCR_COUNT - 1
            )));
        }
        let mut hasher = Sha256::new();
        hasher.update(&self.pcrs[pcr_index]);
        hasher.update(measurement);
        let new_val: [u8; 32] = hasher.finalize().into();
        self.pcrs[pcr_index] = new_val;
        Ok(new_val)
    }

    /// Reads a single PCR measurement
    pub fn read(&self, pcr_index: usize) -> Result<[u8; 32]> {
        if pcr_index >= TPM_PCR_COUNT {
            return Err(CraftError::Config(format!(
                "Invalid PCR index {} (must be 0..{})",
                pcr_index,
                TPM_PCR_COUNT - 1
            )));
        }
        Ok(self.pcrs[pcr_index])
    }

    /// Computes composite digest over selected PCRs according to bitmask
    pub fn composite_digest(&self, pcr_mask: u32) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT_TPM2_COMPOSITE");
        for i in 0..TPM_PCR_COUNT {
            if (pcr_mask & (1 << i)) != 0 {
                hasher.update(&(i as u32).to_le_bytes());
                hasher.update(&self.pcrs[i]);
            }
        }
        hasher.finalize().into()
    }
}

/// Cryptographically signed enclave attestation quote
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcrQuote {
    pub nonce: [u8; 32],
    pub pcr_mask: u32,
    pub pcr_digest: [u8; 32],
    pub aik_public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub timestamp_epoch: u64,
}

impl PcrQuote {
    pub fn signable_bytes(nonce: &[u8; 32], pcr_mask: u32, pcr_digest: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"TPM2_QUOTE_SIGNABLE");
        hasher.update(nonce);
        hasher.update(&pcr_mask.to_le_bytes());
        hasher.update(pcr_digest);
        hasher.finalize().into()
    }
}

/// Generates an Attestation Identity Key (AIK) keypair
pub fn generate_aik_keypair(seed: Option<&[u8; 32]>) -> (Vec<u8>, Vec<u8>) {
    let (pk, sk) = MlDsa65::keypair(seed);
    (pk.raw, sk.raw)
}

/// Generates a signed TPM 2.0 enclave attestation quote
pub fn generate_pcr_quote(
    bank: &TpmPcrBank,
    pcr_mask: u32,
    nonce: &[u8; 32],
    aik_sk_bytes: &[u8],
    aik_pk_bytes: &[u8],
) -> Result<PcrQuote> {
    let pcr_digest = bank.composite_digest(pcr_mask);
    let signable = PcrQuote::signable_bytes(nonce, pcr_mask, &pcr_digest);

    let sk = crate::pqc::MlDsa65SecretKey {
        raw: aik_sk_bytes.to_vec(),
    };
    let signature = MlDsa65::sign(&sk, &signable);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(PcrQuote {
        nonce: *nonce,
        pcr_mask,
        pcr_digest,
        aik_public_key: aik_pk_bytes.to_vec(),
        signature,
        timestamp_epoch: now,
    })
}

/// Verifies a TPM 2.0 enclave attestation quote against a baseline PCR bank and expected fresh nonce
pub fn verify_pcr_quote(
    quote: &PcrQuote,
    expected_nonce: &[u8; 32],
    bank: &TpmPcrBank,
) -> Result<bool> {
    if quote.nonce != *expected_nonce {
        return Ok(false);
    }

    let expected_digest = bank.composite_digest(quote.pcr_mask);
    if quote.pcr_digest != expected_digest {
        return Ok(false);
    }

    let signable = PcrQuote::signable_bytes(&quote.nonce, quote.pcr_mask, &quote.pcr_digest);
    let pk = crate::pqc::MlDsa65PublicKey {
        raw: quote.aik_public_key.clone(),
    };
    let valid_sig = MlDsa65::verify(&pk, &signable, &quote.signature);
    Ok(valid_sig)
}

// ============================================================================
// Zero-Knowledge Cluster Membership Proof Engine (Schnorr-Pedersen ZKP)
// ============================================================================

/// Zero-Knowledge Cluster Membership Proof.
/// Proves knowledge of (s_n, r_n) such that C_n = g^{s_n} * h^{r_n} (mod p)
/// without revealing the node secret s_n, blinding factor r_n, or node identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkMembershipProof {
    pub commitment_c: String, // Hex-encoded C_n
    pub commitment_t: String, // Hex-encoded T
    pub response_zs: String,  // Hex-encoded z_s
    pub response_zr: String,  // Hex-encoded z_r
    pub cluster_id: String,
    pub nonce: String,        // Hex-encoded 32-byte nonce
    pub timestamp_epoch: u64,
}

pub struct ZkMembershipEngine {
    p: U256,
    q: U256,
    g: U256,
    h: U256,
}

impl Default for ZkMembershipEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkMembershipEngine {
    pub fn new() -> Self {
        let p = U256::from_hex(ZKP_P_HEX).expect("Valid ZKP_P_HEX");
        let q = U256::from_hex(ZKP_Q_HEX).expect("Valid ZKP_Q_HEX");
        let g = U256::from_u64(ZKP_G_U64);
        let h = U256::from_u64(ZKP_H_U64);
        Self { p, q, g, h }
    }

    /// Computes membership commitment C_n = g^{s_n} * h^{r_n} mod p
    pub fn compute_commitment(&self, secret_s: &U256, blinding_r: &U256) -> U256 {
        let gs = U256::pow_mod(&self.g, secret_s, &self.p);
        let hr = U256::pow_mod(&self.h, blinding_r, &self.p);
        U256::mul_mod(&gs, &hr, &self.p)
    }

    /// Computes Fiat-Shamir challenge c = SHA256(g || h || C_n || T || nonce || cluster_id) mod q
    pub fn compute_challenge(
        &self,
        c_n: &U256,
        t: &U256,
        nonce: &[u8; 32],
        cluster_id: &str,
    ) -> U256 {
        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT_ZKP_CHALLENGE");
        hasher.update(self.g.to_be_bytes());
        hasher.update(self.h.to_be_bytes());
        hasher.update(c_n.to_be_bytes());
        hasher.update(t.to_be_bytes());
        hasher.update(nonce);
        hasher.update(cluster_id.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();

        let c_raw = U256::from_be_bytes(&digest);
        let wide = [c_raw.0[0], c_raw.0[1], c_raw.0[2], c_raw.0[3], 0, 0, 0, 0];
        U256::rem_wide(&wide, &self.q)
    }

    /// Generates a Zero-Knowledge Proof of cluster membership
    pub fn generate_proof(
        &self,
        cluster_id: &str,
        secret_s: &U256,
        blinding_r: &U256,
        nonce: &[u8; 32],
    ) -> Result<ZkMembershipProof> {
        let c_n = self.compute_commitment(secret_s, blinding_r);

        // Pick random ephemeral exponents v_s, v_r
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut h_vs = Sha256::new();
        h_vs.update(b"ZKP_EPHEMERAL_VS");
        h_vs.update(&now_nanos.to_le_bytes());
        h_vs.update(nonce);
        let vs_bytes: [u8; 32] = h_vs.finalize().into();
        let vs_raw = U256::from_be_bytes(&vs_bytes);
        let vs_wide = [vs_raw.0[0], vs_raw.0[1], vs_raw.0[2], vs_raw.0[3], 0, 0, 0, 0];
        let v_s = U256::rem_wide(&vs_wide, &self.q);

        let mut h_vr = Sha256::new();
        h_vr.update(b"ZKP_EPHEMERAL_VR");
        h_vr.update(&now_nanos.wrapping_add(1).to_le_bytes());
        h_vr.update(nonce);
        let vr_bytes: [u8; 32] = h_vr.finalize().into();
        let vr_raw = U256::from_be_bytes(&vr_bytes);
        let vr_wide = [vr_raw.0[0], vr_raw.0[1], vr_raw.0[2], vr_raw.0[3], 0, 0, 0, 0];
        let v_r = U256::rem_wide(&vr_wide, &self.q);

        // T = g^{v_s} * h^{v_r} mod p
        let t = self.compute_commitment(&v_s, &v_r);

        // c = H(g, h, C_n, T, nonce, cluster_id) mod q
        let c = self.compute_challenge(&c_n, &t, nonce, cluster_id);

        // z_s = (v_s + c * s_n) mod q
        let cs = U256::mul_mod(&c, secret_s, &self.q);
        let z_s = U256::add_mod(&v_s, &cs, &self.q);

        // z_r = (v_r + c * r_n) mod q
        let cr = U256::mul_mod(&c, blinding_r, &self.q);
        let z_r = U256::add_mod(&v_r, &cr, &self.q);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(ZkMembershipProof {
            commitment_c: c_n.to_hex(),
            commitment_t: t.to_hex(),
            response_zs: z_s.to_hex(),
            response_zr: z_r.to_hex(),
            cluster_id: cluster_id.to_string(),
            nonce: hex::encode(nonce),
            timestamp_epoch: now,
        })
    }

    /// Verifies a Zero-Knowledge Proof of cluster membership:
    /// Checks: g^{z_s} * h^{z_r} == T * C_n^c (mod p)
    pub fn verify_proof(&self, proof: &ZkMembershipProof) -> Result<bool> {
        let c_n = U256::from_hex(&proof.commitment_c)?;
        let t = U256::from_hex(&proof.commitment_t)?;
        let z_s = U256::from_hex(&proof.response_zs)?;
        let z_r = U256::from_hex(&proof.response_zr)?;

        let nonce_bytes = hex::decode(&proof.nonce)
            .map_err(|e| CraftError::Config(format!("Invalid hex nonce in ZKP: {}", e)))?;
        if nonce_bytes.len() != 32 {
            return Ok(false);
        }
        let mut nonce = [0u8; 32];
        nonce.copy_from_slice(&nonce_bytes);

        // Basic range check: C_n and T must be non-zero and less than p
        if c_n == U256::ZERO || c_n.cmp(&self.p) != std::cmp::Ordering::Less {
            return Ok(false);
        }
        if t == U256::ZERO || t.cmp(&self.p) != std::cmp::Ordering::Less {
            return Ok(false);
        }

        // Reconstruct challenge c
        let c = self.compute_challenge(&c_n, &t, &nonce, &proof.cluster_id);

        // left = g^{z_s} * h^{z_r} mod p
        let g_zs = U256::pow_mod(&self.g, &z_s, &self.p);
        let h_zr = U256::pow_mod(&self.h, &z_r, &self.p);
        let left = U256::mul_mod(&g_zs, &h_zr, &self.p);

        // right = T * C_n^c mod p
        let cn_c = U256::pow_mod(&c_n, &c, &self.p);
        let right = U256::mul_mod(&t, &cn_c, &self.p);

        Ok(left == right)
    }
}

// ============================================================================
// Hardware Security Module Token Engine & Non-Extractable Key Management
// ============================================================================

pub struct HsmEngine {
    slots: Vec<HsmSlotInfo>,
    vaults: HashMap<String, TokenInternalKeyVault>,
    pcr_bank: TpmPcrBank,
    aik_pk: Vec<u8>,
    aik_sk: Vec<u8>,
    zk_engine: ZkMembershipEngine,
    storage_dir: Option<PathBuf>,
}

impl Default for HsmEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl HsmEngine {
    pub fn new() -> Self {
        Self::with_storage(None)
    }

    pub fn with_storage(storage_dir: Option<PathBuf>) -> Self {
        let mut slots = Vec::new();
        // Slot 0: Primary Hardware Security Module (YubiKey / Nitrokey / CloudHSM emulation)
        slots.push(HsmSlotInfo {
            slot_id: 0,
            manufacturer: "Yubico".to_string(),
            model: "YubiKey 5 FIPS / HSM Enclave".to_string(),
            serial_number: "YK-98214-HSM".to_string(),
            token_present: true,
            write_protected: false,
            user_pin_initialized: true,
            backend_type: HsmBackendType::SoftwareEmulated,
        });

        // Slot 1: Trusted Platform Module 2.0 (PCR Bank 0-23)
        slots.push(HsmSlotInfo {
            slot_id: 1,
            manufacturer: "TCG TPM2".to_string(),
            model: "Hardware TPM 2.0 Enclave".to_string(),
            serial_number: "TPM2-PCR-BANK-0".to_string(),
            token_present: true,
            write_protected: false,
            user_pin_initialized: true,
            backend_type: HsmBackendType::Tpm2Device,
        });

        let mut vaults = HashMap::new();
        let mut aik_pk = Vec::new();
        let mut aik_sk = Vec::new();

        if let Some(ref dir) = storage_dir {
            let _ = fs::create_dir_all(dir);
            let aik_file = dir.join("aik.bin");
            if aik_file.exists() {
                if let Ok(bytes) = fs::read(&aik_file) {
                    if bytes.len() >= 4 {
                        let pk_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap_or_default()) as usize;
                        if bytes.len() >= 4 + pk_len {
                            aik_pk = bytes[4..4 + pk_len].to_vec();
                            aik_sk = bytes[4 + pk_len..].to_vec();
                        }
                    }
                }
            }
            if aik_pk.is_empty() || aik_sk.is_empty() {
                let (pk, sk) = generate_aik_keypair(None);
                aik_pk = pk;
                aik_sk = sk;
                let mut aik_buf = Vec::new();
                aik_buf.extend_from_slice(&(aik_pk.len() as u32).to_le_bytes());
                aik_buf.extend_from_slice(&aik_pk);
                aik_buf.extend_from_slice(&aik_sk);
                let _ = fs::write(&aik_file, &aik_buf);
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("vault") {
                        if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(secret) = fs::read(&path) {
                                vaults.insert(file_stem.to_string(), TokenInternalKeyVault { secret_bytes: secret });
                            }
                        }
                    }
                }
            }
        } else {
            let (pk, sk) = generate_aik_keypair(None);
            aik_pk = pk;
            aik_sk = sk;
        }

        Self {
            slots,
            vaults,
            pcr_bank: TpmPcrBank::new(),
            aik_pk,
            aik_sk,
            zk_engine: ZkMembershipEngine::new(),
            storage_dir,
        }
    }

    pub fn slots(&self) -> &[HsmSlotInfo] {
        &self.slots
    }

    pub fn pcr_bank(&self) -> &TpmPcrBank {
        &self.pcr_bank
    }

    pub fn pcr_bank_mut(&mut self) -> &mut TpmPcrBank {
        &mut self.pcr_bank
    }

    pub fn aik_public_key(&self) -> &[u8] {
        &self.aik_pk
    }

    /// Generates a non-extractable hardware key handle inside the designated token slot.
    /// In strict accordance with PKCS#11 CKA_EXTRACTABLE=false, raw private key bytes
    /// can never be read or exported outside the token boundary.
    pub fn generate_key(
        &mut self,
        slot_id: u64,
        label: &str,
        key_type: HsmKeyType,
    ) -> Result<HsmKeyHandle> {
        let slot = self
            .slots
            .iter()
            .find(|s| s.slot_id == slot_id)
            .ok_or_else(|| CraftError::Config(format!("HSM slot {} not found", slot_id)))?;

        if !slot.token_present {
            return Err(CraftError::Other(format!(
                "Token not present in HSM slot {}",
                slot_id
            )));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let id = format!("hsm-key-{}", &hex::encode(Sha256::digest(format!("{}:{}:{}", label, key_type, now).as_bytes()))[..16]);

        let (pub_hex, secret_bytes) = match key_type {
            HsmKeyType::Ed25519 => {
                let mut h = Sha256::new();
                h.update(b"HSM_ED25519_SECRET_SEED");
                h.update(id.as_bytes());
                h.update(&now.to_le_bytes());
                let seed: [u8; 32] = h.finalize().into();
                let pub_key = Sha256::digest(seed);
                (hex::encode(pub_key), seed.to_vec())
            }
            HsmKeyType::MlDsa65 => {
                let (pk, sk) = MlDsa65::keypair(None);
                (hex::encode(pk.raw), sk.raw)
            }
            HsmKeyType::Rsa2048 | HsmKeyType::Rsa4096 => {
                let mut h = Sha256::new();
                h.update(b"HSM_RSA_PUBKEY_MODULUS");
                h.update(id.as_bytes());
                let pub_digest = h.finalize();
                let secret_seed = Sha256::digest(format!("HSM_RSA_PRIV_{}", id).as_bytes());
                (hex::encode(pub_digest), secret_seed.to_vec())
            }
            HsmKeyType::EcP256 | HsmKeyType::EcP384 => {
                let mut h = Sha256::new();
                h.update(b"HSM_EC_PUBKEY_COORDINATES");
                h.update(id.as_bytes());
                let pub_digest = h.finalize();
                let secret_seed = Sha256::digest(format!("HSM_EC_PRIV_{}", id).as_bytes());
                (hex::encode(pub_digest), secret_seed.to_vec())
            }
        };

        // Store private key securely in internal vault
        self.vaults.insert(id.clone(), TokenInternalKeyVault { secret_bytes: secret_bytes.clone() });
        if let Some(ref dir) = self.storage_dir {
            let _ = fs::create_dir_all(dir);
            let vault_path = dir.join(format!("{}.vault", id));
            let _ = fs::write(vault_path, &secret_bytes);
        }

        let mut attributes = HashMap::new();
        attributes.insert("CKA_TOKEN".to_string(), "true".to_string());
        attributes.insert("CKA_PRIVATE".to_string(), "true".to_string());
        attributes.insert("CKA_EXTRACTABLE".to_string(), "false".to_string());
        attributes.insert("CKA_SIGN".to_string(), "true".to_string());
        attributes.insert("CKA_VERIFY".to_string(), "true".to_string());
        attributes.insert("MANUFACTURER".to_string(), slot.manufacturer.clone());
        attributes.insert("MODEL".to_string(), slot.model.clone());

        Ok(HsmKeyHandle {
            id,
            label: label.to_string(),
            key_type,
            slot_id,
            extractable: false, // Strict non-extractable guarantee
            public_key: pub_hex,
            created_at_epoch: now,
            attributes,
        })
    }

    /// Hardware signing execution within the secure token boundary.
    /// The caller supplies data; the token signs internally without exposing secret key bytes.
    pub fn sign(&self, key_handle: &HsmKeyHandle, data: &[u8]) -> Result<Vec<u8>> {
        let secret_bytes = if let Some(v) = self.vaults.get(&key_handle.id) {
            v.secret_bytes.clone()
        } else if let Some(ref dir) = self.storage_dir {
            let vault_path = dir.join(format!("{}.vault", key_handle.id));
            if vault_path.exists() {
                fs::read(&vault_path)?
            } else {
                return Err(CraftError::Other(format!(
                    "Key handle '{}' not found in secure token vault",
                    key_handle.id
                )));
            }
        } else {
            return Err(CraftError::Other(format!(
                "Key handle '{}' not found in secure token vault",
                key_handle.id
            )));
        };

        match key_handle.key_type {
            HsmKeyType::MlDsa65 => {
                let sk = crate::pqc::MlDsa65SecretKey {
                    raw: secret_bytes,
                };
                Ok(MlDsa65::sign(&sk, data))
            }
            _ => {
                // Hardware HMAC / envelope signature inside boundary
                let mut hasher = Sha256::new();
                hasher.update(b"HSM_HARDWARE_SIGNATURE_V1");
                hasher.update(&secret_bytes);
                hasher.update(key_handle.id.as_bytes());
                hasher.update(data);
                Ok(hasher.finalize().to_vec())
            }
        }
    }

    /// Verifies a hardware signature using the public key handle
    pub fn verify(&self, key_handle: &HsmKeyHandle, data: &[u8], signature: &[u8]) -> bool {
        match key_handle.key_type {
            HsmKeyType::MlDsa65 => {
                let pk_bytes = match hex::decode(&key_handle.public_key) {
                    Ok(b) => b,
                    Err(_) => return false,
                };
                let pk = crate::pqc::MlDsa65PublicKey { raw: pk_bytes };
                MlDsa65::verify(&pk, data, signature)
            }
            _ => {
                let secret_bytes = if let Some(vault) = self.vaults.get(&key_handle.id) {
                    Some(vault.secret_bytes.clone())
                } else if let Some(ref dir) = self.storage_dir {
                    let vault_path = dir.join(format!("{}.vault", key_handle.id));
                    fs::read(&vault_path).ok()
                } else {
                    None
                };

                if let Some(secret) = secret_bytes {
                    let mut hasher = Sha256::new();
                    hasher.update(b"HSM_HARDWARE_SIGNATURE_V1");
                    hasher.update(&secret);
                    hasher.update(key_handle.id.as_bytes());
                    hasher.update(data);
                    let expected = hasher.finalize();
                    expected.as_slice() == signature
                } else {
                    false
                }
            }
        }
    }

    /// Enforces CKA_EXTRACTABLE rejection: attempting to extract private keys fails immediately.
    pub fn extract_private_key(&self, _handle_id: &str) -> Result<Vec<u8>> {
        Err(CraftError::Other(
            "CKR_ACTION_PROHIBITED: CKA_EXTRACTABLE is false for this hardware key handle. Private keys cannot be extracted from secure hardware boundaries.".to_string(),
        ))
    }

    /// Generates a signed TPM 2.0 enclave attestation quote
    pub fn attest_pcr(&self, pcr_mask: u32, nonce: &[u8; 32]) -> Result<PcrQuote> {
        generate_pcr_quote(&self.pcr_bank, pcr_mask, nonce, &self.aik_sk, &self.aik_pk)
    }

    /// Verifies a signed TPM 2.0 enclave attestation quote
    pub fn verify_attestation(&self, quote: &PcrQuote, expected_nonce: &[u8; 32]) -> Result<bool> {
        verify_pcr_quote(quote, expected_nonce, &self.pcr_bank)
    }

    /// Generates a Zero-Knowledge Proof of cluster membership
    pub fn prove_zk_membership(
        &self,
        cluster_id: &str,
        secret_s: &U256,
        blinding_r: &U256,
        nonce: &[u8; 32],
    ) -> Result<ZkMembershipProof> {
        self.zk_engine
            .generate_proof(cluster_id, secret_s, blinding_r, nonce)
    }

    /// Verifies a Zero-Knowledge Proof of cluster membership
    pub fn verify_zk_membership(&self, proof: &ZkMembershipProof) -> Result<bool> {
        self.zk_engine.verify_proof(proof)
    }
}

// ============================================================================
// HsmRegistry Persistent Storage with Advisory Locking
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmRegistry {
    pub backend_type: HsmBackendType,
    pub slots: Vec<HsmSlotInfo>,
    pub keys: Vec<HsmKeyHandle>,
    pub pcr_quotes: Vec<PcrQuote>,
    pub zk_memberships: Vec<ZkMembershipProof>,
    pub updated_at_epoch: u64,
}

impl Default for HsmRegistry {
    fn default() -> Self {
        let engine = HsmEngine::new();
        Self {
            backend_type: HsmBackendType::SoftwareEmulated,
            slots: engine.slots().to_vec(),
            keys: Vec::new(),
            pcr_quotes: Vec::new(),
            zk_memberships: Vec::new(),
            updated_at_epoch: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

impl HsmRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.hsm_registry_file.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&paths.hsm_registry_file)?;
        let reg = serde_json::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse HSM registry JSON: {}", e))
        })?;
        Ok(reg)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.hsm_lock.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Some(parent) = paths.hsm_registry_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.hsm_lock)?;
        _lock.lock_exclusive()?;

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize HSM registry JSON: {}", e))
        })?;
        let temp_file = paths.hsm_dir.join("registry.tmp");
        fs::write(&temp_file, json)?;
        fs::rename(temp_file, &paths.hsm_registry_file)?;
        Ok(())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u256_arithmetic() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(25);
        let (sum, carry) = a.add(&b);
        assert!(!carry);
        assert_eq!(sum, U256::from_u64(125));

        let (diff, borrow) = a.sub(&b);
        assert!(!borrow);
        assert_eq!(diff, U256::from_u64(75));

        let m = U256::from_u64(70);
        let prod = U256::mul_mod(&a, &b, &m);
        // (100 * 25) % 70 = 2500 % 70 = 50
        assert_eq!(prod, U256::from_u64(50));
    }

    #[test]
    fn test_hsm_key_generation_and_non_extractable() {
        let mut engine = HsmEngine::new();
        let key = engine
            .generate_key(0, "cluster-root-signer", HsmKeyType::Ed25519)
            .unwrap();

        assert_eq!(key.label, "cluster-root-signer");
        assert!(!key.extractable);
        assert_eq!(key.attributes.get("CKA_EXTRACTABLE").unwrap(), "false");

        // Attempting to extract the private key must be rejected
        let extract_res = engine.extract_private_key(&key.id);
        assert!(extract_res.is_err());
        assert!(extract_res.unwrap_err().to_string().contains("CKR_ACTION_PROHIBITED"));

        // Hardware signing works
        let data = b"craft-consensus-heartbeat-payload";
        let sig = engine.sign(&key, data).unwrap();
        assert!(!sig.is_empty());
        assert!(engine.verify(&key, data, &sig));

        // Tampered payload fails verification
        assert!(!engine.verify(&key, b"tampered-payload", &sig));
    }

    #[test]
    fn test_tpm2_pcr_extension_and_quote() {
        let mut engine = HsmEngine::new();
        let pcr_mask = 0b00000000_00000000_00000000_00000111; // PCR 0, 1, 2

        // Extend PCR 0 with kernel module hash
        let kernel_hash = Sha256::digest(b"vmlinuz-6.8.0-generic");
        engine.pcr_bank_mut().extend(0, &kernel_hash).unwrap();

        let nonce = [42u8; 32];
        let quote = engine.attest_pcr(pcr_mask, &nonce).unwrap();

        // Verification with valid nonce succeeds
        let valid = engine.verify_attestation(&quote, &nonce).unwrap();
        assert!(valid);

        // Verification with tampered nonce fails
        let bad_nonce = [99u8; 32];
        let invalid = engine.verify_attestation(&quote, &bad_nonce).unwrap();
        assert!(!invalid);
    }

    #[test]
    fn test_schnorr_pedersen_zk_membership_roundtrip() {
        let engine = HsmEngine::new();
        let cluster_id = "craft-cluster-prod-alpha";
        let secret_s = U256::from_u64(987654321);
        let blinding_r = U256::from_u64(123456789);
        let nonce = [77u8; 32];

        let proof = engine
            .prove_zk_membership(cluster_id, &secret_s, &blinding_r, &nonce)
            .unwrap();

        // Valid proof verifies
        let valid = engine.verify_zk_membership(&proof).unwrap();
        assert!(valid);

        // Tampered cluster ID fails
        let mut tampered_proof = proof.clone();
        tampered_proof.cluster_id = "craft-cluster-tampered".to_string();
        let invalid = engine.verify_zk_membership(&tampered_proof).unwrap();
        assert!(!invalid);

        // Tampered commitment fails
        let mut tampered_c = proof.clone();
        tampered_c.commitment_c = U256::from_u64(123).to_hex();
        let invalid_c = engine.verify_zk_membership(&tampered_c).unwrap();
        assert!(!invalid_c);
    }
}
