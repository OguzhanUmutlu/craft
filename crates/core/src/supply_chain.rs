use crate::audit::compute_hmac_sha256;
use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub const IN_TOTO_STATEMENT_V1: &str = "https://in-toto.io/Statement/v1";
pub const SLSA_PREDICATE_V02: &str = "https://slsa.dev/provenance/v0.2";
pub const DSSE_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SlsaLevel {
    None = 0,
    Slsa1 = 1,
    Slsa2 = 2,
    Slsa3 = 3,
    Slsa4 = 4,
}

impl fmt::Display for SlsaLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlsaLevel::None => write!(f, "SLSA-0"),
            SlsaLevel::Slsa1 => write!(f, "SLSA-1"),
            SlsaLevel::Slsa2 => write!(f, "SLSA-2"),
            SlsaLevel::Slsa3 => write!(f, "SLSA-3"),
            SlsaLevel::Slsa4 => write!(f, "SLSA-4"),
        }
    }
}

impl std::str::FromStr for SlsaLevel {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "0" | "NONE" | "SLSA-0" | "SLSA0" => Ok(SlsaLevel::None),
            "1" | "SLSA-1" | "SLSA1" => Ok(SlsaLevel::Slsa1),
            "2" | "SLSA-2" | "SLSA2" => Ok(SlsaLevel::Slsa2),
            "3" | "SLSA-3" | "SLSA3" => Ok(SlsaLevel::Slsa3),
            "4" | "SLSA-4" | "SLSA4" => Ok(SlsaLevel::Slsa4),
            _ => Err(CraftError::Config(format!("Invalid SLSA level: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementMode {
    AuditOnly,
    StrictBlock,
    Disabled,
}

impl fmt::Display for EnforcementMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnforcementMode::AuditOnly => write!(f, "AuditOnly"),
            EnforcementMode::StrictBlock => write!(f, "StrictBlock"),
            EnforcementMode::Disabled => write!(f, "Disabled"),
        }
    }
}

impl std::str::FromStr for EnforcementMode {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "audit" | "auditonly" | "audit-only" => Ok(EnforcementMode::AuditOnly),
            "strict" | "strictblock" | "strict-block" | "block" => Ok(EnforcementMode::StrictBlock),
            "disabled" | "off" | "none" => Ok(EnforcementMode::Disabled),
            _ => Err(CraftError::Config(format!(
                "Invalid enforcement mode: {}",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub name: String,
    pub digest: HashMap<String, String>,
}

impl Subject {
    pub fn new(name: impl Into<String>, sha256_hex: impl Into<String>) -> Self {
        let mut digest = HashMap::new();
        digest.insert("sha256".to_string(), sha256_hex.into());
        Self {
            name: name.into(),
            digest,
        }
    }

    pub fn sha256(&self) -> Option<&str> {
        self.digest.get("sha256").map(|s| s.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuilderInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSource {
    pub uri: String,
    pub digest: HashMap<String, String>,
    pub entry_point: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildInvocation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_source: Option<ConfigSource>,
    #[serde(default)]
    pub parameters: HashMap<String, String>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildMaterial {
    pub uri: String,
    pub digest: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCompleteness {
    pub parameters: bool,
    pub environment: bool,
    pub materials: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub invocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_on: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_on: Option<DateTime<Utc>>,
    pub reproducible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness: Option<BuildCompleteness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlsaPredicate {
    pub builder: BuilderInfo,
    pub build_type: String,
    pub invocation: BuildInvocation,
    #[serde(default)]
    pub materials: Vec<BuildMaterial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BuildMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InTotoStatement {
    #[serde(rename = "_type")]
    pub statement_type: String,
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: SlsaPredicate,
}

impl InTotoStatement {
    pub fn new(subject: Vec<Subject>, predicate: SlsaPredicate) -> Self {
        Self {
            statement_type: IN_TOTO_STATEMENT_V1.to_string(),
            subject,
            predicate_type: SLSA_PREDICATE_V02.to_string(),
            predicate,
        }
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(CraftError::Json)
    }

    pub fn determine_slsa_level(&self) -> SlsaLevel {
        if self.subject.is_empty() {
            return SlsaLevel::None;
        }

        // SLSA 1 requires provenance with identified builder and materials
        if self.predicate.builder.id.is_empty() {
            return SlsaLevel::None;
        }

        let mut level = SlsaLevel::Slsa1;

        // SLSA 2 requires hosted build service and authenticated provenance
        if self.predicate.invocation.config_source.is_some() {
            level = SlsaLevel::Slsa2;
        }

        // SLSA 3 requires hermetic isolated environment and reproducible metadata
        if let Some(ref meta) = self.predicate.metadata {
            if meta.reproducible {
                if let Some(ref comp) = meta.completeness {
                    if comp.parameters && comp.environment && comp.materials {
                        level = SlsaLevel::Slsa3;
                    }
                }
            }
        }

        level
    }
}

/// DSSE Pre-Authentication Encoding (PAE) according to DSSE v1 spec
pub fn dsse_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut pae = Vec::new();
    let header = format!("DSSEv1 {} {} {} ", payload_type.len(), payload_type, payload.len());
    pae.extend_from_slice(header.as_bytes());
    pae.extend_from_slice(payload);
    pae
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsseSignature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsseEnvelope {
    pub payload_type: String,
    pub payload: String,
    pub signatures: Vec<DsseSignature>,
}

impl DsseEnvelope {
    pub fn extract_statement(&self) -> Result<InTotoStatement> {
        let payload_bytes = if let Ok(decoded) = hex::decode(&self.payload) {
            decoded
        } else {
            self.payload.as_bytes().to_vec()
        };

        serde_json::from_slice(&payload_bytes).map_err(CraftError::Json)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InclusionProof {
    pub log_index: u64,
    pub root_hash: String,
    pub tree_size: u64,
    pub hashes: Vec<String>,
}

impl InclusionProof {
    pub fn verify_inclusion(&self, leaf_hash: &[u8; 32]) -> bool {
        if self.hashes.is_empty() {
            return hex::encode(leaf_hash) == self.root_hash;
        }

        let mut curr = *leaf_hash;
        let mut idx = self.log_index;

        for sibling_hex in &self.hashes {
            let Ok(sibling) = hex::decode(sibling_hex) else {
                return false;
            };
            let mut hasher = Sha256::new();
            hasher.update([0x01]);
            if idx % 2 == 0 {
                hasher.update(&curr);
                hasher.update(&sibling);
            } else {
                hasher.update(&sibling);
                hasher.update(&curr);
            }
            curr = hasher.finalize().into();
            idx /= 2;
        }

        hex::encode(curr).to_lowercase() == self.root_hash.to_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransparencyLogEntry {
    pub log_index: u64,
    pub log_id: String,
    pub integrated_time: i64,
    pub body: String,
    pub inclusion_proof: InclusionProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationMaterial {
    #[serde(default)]
    pub public_keys: Vec<String>,
    #[serde(default)]
    pub tlog_entries: Vec<TransparencyLogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigstoreBundle {
    pub media_type: String,
    pub verification_material: VerificationMaterial,
    pub dsse_envelope: DsseEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustAnchor {
    pub id: String,
    pub key_type: String,
    pub public_key_or_secret: String,
    pub issuer: String,
    pub identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<DateTime<Utc>>,
    pub is_revoked: bool,
}

impl TrustAnchor {
    pub fn new(
        id: impl Into<String>,
        key_type: impl Into<String>,
        public_key_or_secret: impl Into<String>,
        issuer: impl Into<String>,
        identity: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            key_type: key_type.into(),
            public_key_or_secret: public_key_or_secret.into(),
            issuer: issuer.into(),
            identity: identity.into(),
            not_after: None,
            is_revoked: false,
        }
    }

    pub fn verify_signature(&self, pae_bytes: &[u8], signature_str: &str) -> bool {
        if self.is_revoked {
            return false;
        }
        if let Some(expiry) = self.not_after {
            if Utc::now() > expiry {
                return false;
            }
        }

        match self.key_type.as_str() {
            "hmac-sha256" => {
                let computed = compute_hmac_sha256(
                    self.public_key_or_secret.as_bytes(),
                    pae_bytes,
                );
                let computed_hex = hex::encode(computed);
                computed_hex.eq_ignore_ascii_case(signature_str)
            }
            "sha256-fingerprint" => {
                let digest = Sha256::digest(pae_bytes);
                let computed_hex = hex::encode(digest);
                computed_hex.eq_ignore_ascii_case(signature_str)
            }
            _ => {
                // Generic signature verification fallback
                false
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyChainPolicy {
    pub enforcement_mode: EnforcementMode,
    pub minimum_slsa_level: SlsaLevel,
    #[serde(default)]
    pub allowed_builders: Vec<String>,
    pub require_reproducible: bool,
    pub require_transparency_log: bool,
    #[serde(default)]
    pub trusted_keys: Vec<String>,
    #[serde(default)]
    pub exempt_artifacts: Vec<String>,
}

impl Default for SupplyChainPolicy {
    fn default() -> Self {
        Self {
            enforcement_mode: EnforcementMode::AuditOnly,
            minimum_slsa_level: SlsaLevel::Slsa1,
            allowed_builders: vec![
                "craft-hermetic-builder".to_string(),
                "craft-cli".to_string(),
                "https://github.com/craft-cli/build-pipeline".to_string(),
            ],
            require_reproducible: false,
            require_transparency_log: false,
            trusted_keys: Vec::new(),
            exempt_artifacts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationVerdict {
    pub verified: bool,
    pub artifact_sha256: String,
    pub artifact_name: String,
    pub slsa_level: SlsaLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_id: Option<String>,
    pub transparency_log_verified: bool,
    pub reproducible_match: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HermeticBuildManifest {
    pub source_repo: String,
    pub commit_hash: String,
    pub source_date_epoch: u64,
    #[serde(default)]
    pub env_allowlist: Vec<String>,
    pub network_isolated: bool,
    #[serde(default)]
    pub output_artifacts: Vec<Subject>,
    pub build_digest: String,
}

pub fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| CraftError::Io(e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(|e| CraftError::Io(e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sign_in_toto_statement(
    statement: &InTotoStatement,
    key_id: &str,
    secret: &[u8],
) -> Result<DsseEnvelope> {
    let statement_json = statement.to_json_bytes()?;
    let pae = dsse_pae(DSSE_PAYLOAD_TYPE, &statement_json);
    let signature = hex::encode(compute_hmac_sha256(secret, &pae));

    Ok(DsseEnvelope {
        payload_type: DSSE_PAYLOAD_TYPE.to_string(),
        payload: hex::encode(statement_json),
        signatures: vec![DsseSignature {
            keyid: key_id.to_string(),
            sig: signature,
        }],
    })
}

pub fn create_inclusion_proof(
    log_index: u64,
    leaf_hash: &[u8; 32],
    tree_size: u64,
    siblings: Vec<[u8; 32]>,
) -> InclusionProof {
    let mut curr = *leaf_hash;
    let mut idx = log_index;
    let hashes = siblings
        .iter()
        .map(|s| hex::encode(s))
        .collect::<Vec<_>>();

    for sibling in &siblings {
        let mut hasher = Sha256::new();
        hasher.update([0x01]);
        if idx % 2 == 0 {
            hasher.update(&curr);
            hasher.update(sibling);
        } else {
            hasher.update(sibling);
            hasher.update(&curr);
        }
        curr = hasher.finalize().into();
        idx /= 2;
    }

    InclusionProof {
        log_index,
        root_hash: hex::encode(curr),
        tree_size,
        hashes,
    }
}

pub fn evaluate_supply_chain(
    artifact_path: &Path,
    statement_opt: Option<&InTotoStatement>,
    envelope_opt: Option<&DsseEnvelope>,
    tlog_entry_opt: Option<&TransparencyLogEntry>,
    policy: &SupplyChainPolicy,
    trust_anchors: &[TrustAnchor],
) -> Result<VerificationVerdict> {
    let artifact_name = artifact_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let actual_sha256 = if artifact_path.exists() {
        compute_file_sha256(artifact_path)?
    } else {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    };

    let mut reasons = Vec::new();
    let evaluated_at = Utc::now();

    if policy.enforcement_mode == EnforcementMode::Disabled {
        return Ok(VerificationVerdict {
            verified: true,
            artifact_sha256: actual_sha256,
            artifact_name,
            slsa_level: SlsaLevel::None,
            signer_identity: None,
            builder_id: None,
            transparency_log_verified: false,
            reproducible_match: false,
            reasons: vec!["Supply chain policy enforcement is disabled".to_string()],
            evaluated_at,
        });
    }

    if policy.exempt_artifacts.contains(&artifact_name)
        || policy.exempt_artifacts.contains(&actual_sha256)
    {
        return Ok(VerificationVerdict {
            verified: true,
            artifact_sha256: actual_sha256,
            artifact_name,
            slsa_level: SlsaLevel::None,
            signer_identity: None,
            builder_id: None,
            transparency_log_verified: false,
            reproducible_match: false,
            reasons: vec!["Artifact is explicitly exempt from policy".to_string()],
            evaluated_at,
        });
    }

    let statement = match statement_opt {
        Some(s) => s,
        None => {
            let msg = if policy.enforcement_mode == EnforcementMode::StrictBlock {
                "Artifact missing required cryptographic provenance attestation"
            } else {
                "Provenance attestation not found (audit only)"
            };
            reasons.push(msg.to_string());
            let verified = policy.enforcement_mode == EnforcementMode::AuditOnly;
            return Ok(VerificationVerdict {
                verified,
                artifact_sha256: actual_sha256,
                artifact_name,
                slsa_level: SlsaLevel::None,
                signer_identity: None,
                builder_id: None,
                transparency_log_verified: false,
                reproducible_match: false,
                reasons,
                evaluated_at,
            });
        }
    };

    // Verify Subject hash match
    let subject_matched = statement.subject.iter().any(|subj| {
        subj.sha256()
            .map(|s| s.eq_ignore_ascii_case(&actual_sha256))
            .unwrap_or(false)
    });

    if !subject_matched {
        reasons.push(format!(
            "Artifact SHA-256 ({}) does not match provenance subjects",
            actual_sha256
        ));
    }

    let level = statement.determine_slsa_level();
    if level < policy.minimum_slsa_level {
        reasons.push(format!(
            "Artifact SLSA level {} is below required minimum {}",
            level, policy.minimum_slsa_level
        ));
    }

    let builder_id = statement.predicate.builder.id.clone();
    if !policy.allowed_builders.is_empty() && !policy.allowed_builders.contains(&builder_id) {
        reasons.push(format!(
            "Builder ID '{}' is not in allowed builders list",
            builder_id
        ));
    }

    let reproducible = statement
        .predicate
        .metadata
        .as_ref()
        .map(|m| m.reproducible)
        .unwrap_or(false);
    if policy.require_reproducible && !reproducible {
        reasons.push("Artifact build metadata is not marked reproducible".to_string());
    }

    // Verify Signature if envelope is present
    let mut signer_identity = None;
    let mut sig_verified = false;

    if let Some(envelope) = envelope_opt {
        let payload_bytes = if let Ok(decoded) = hex::decode(&envelope.payload) {
            decoded
        } else {
            envelope.payload.as_bytes().to_vec()
        };
        let pae = dsse_pae(&envelope.payload_type, &payload_bytes);

        for sig in &envelope.signatures {
            for anchor in trust_anchors {
                if anchor.id == sig.keyid || policy.trusted_keys.contains(&anchor.id) {
                    if anchor.verify_signature(&pae, &sig.sig) {
                        sig_verified = true;
                        signer_identity = Some(anchor.identity.clone());
                        break;
                    }
                }
            }
            if sig_verified {
                break;
            }
        }

        if !sig_verified && !trust_anchors.is_empty() {
            reasons.push("Cryptographic DSSE envelope signature could not be verified by any trust anchor".to_string());
        }
    } else if !trust_anchors.is_empty() {
        reasons.push("No signed DSSE envelope provided to verify against trust anchors".to_string());
    }

    // Verify Transparency Log proof if present / required
    let mut tlog_verified = false;
    if let Some(tlog) = tlog_entry_opt {
        let statement_bytes = statement.to_json_bytes()?;
        let leaf_hash: [u8; 32] = Sha256::digest(&statement_bytes).into();
        tlog_verified = tlog.inclusion_proof.verify_inclusion(&leaf_hash);
        if !tlog_verified {
            reasons.push("Transparency log inclusion proof verification failed".to_string());
        }
    } else if policy.require_transparency_log {
        reasons.push("Transparency log inclusion proof required by policy but missing".to_string());
    }

    let is_clean = reasons.is_empty()
        && subject_matched
        && (envelope_opt.is_none() || sig_verified || trust_anchors.is_empty())
        && (!policy.require_transparency_log || tlog_verified);

    let verified = if policy.enforcement_mode == EnforcementMode::StrictBlock {
        is_clean
    } else {
        // AuditOnly
        true
    };

    Ok(VerificationVerdict {
        verified,
        artifact_sha256: actual_sha256,
        artifact_name,
        slsa_level: level,
        signer_identity,
        builder_id: Some(builder_id),
        transparency_log_verified: tlog_verified,
        reproducible_match: reproducible,
        reasons,
        evaluated_at,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyChainRegistry {
    pub policy: SupplyChainPolicy,
    #[serde(default)]
    pub trust_anchors: Vec<TrustAnchor>,
    #[serde(default)]
    pub verdicts: HashMap<String, VerificationVerdict>,
}

impl Default for SupplyChainRegistry {
    fn default() -> Self {
        Self {
            policy: SupplyChainPolicy::default(),
            trust_anchors: Vec::new(),
            verdicts: HashMap::new(),
        }
    }
}

impl SupplyChainRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let lock_file = File::create(&paths.supply_chain_lock)
            .map_err(CraftError::Io)?;
        lock_file
            .lock_shared()
            .map_err(CraftError::Io)?;

        let policy = if paths.supply_chain_policy_file.exists() {
            let content = fs::read_to_string(&paths.supply_chain_policy_file)
                .map_err(CraftError::Io)?;
            toml::from_str(&content)
                .map_err(CraftError::TomlDe)?
        } else {
            SupplyChainPolicy::default()
        };

        let trust_anchors = if paths.supply_chain_trust_anchors_file.exists() {
            let content = fs::read_to_string(&paths.supply_chain_trust_anchors_file)
                .map_err(CraftError::Io)?;
            serde_json::from_str(&content)
                .map_err(CraftError::Json)?
        } else {
            Vec::new()
        };

        let verdicts = if paths.supply_chain_registry_file.exists() {
            let content = fs::read_to_string(&paths.supply_chain_registry_file)
                .map_err(CraftError::Io)?;
            toml::from_str(&content)
                .map_err(CraftError::TomlDe)?
        } else {
            HashMap::new()
        };

        let _ = lock_file.unlock();

        Ok(Self {
            policy,
            trust_anchors,
            verdicts,
        })
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_file = File::create(&paths.supply_chain_lock)
            .map_err(CraftError::Io)?;
        lock_file
            .lock_exclusive()
            .map_err(CraftError::Io)?;

        if !paths.supply_chain_dir.exists() {
            fs::create_dir_all(&paths.supply_chain_dir).map_err(CraftError::Io)?;
        }

        // Save policy
        let policy_str = toml::to_string_pretty(&self.policy)
            .map_err(CraftError::Toml)?;
        let mut pf = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.supply_chain_policy_file)
            .map_err(CraftError::Io)?;
        pf.write_all(policy_str.as_bytes())
            .map_err(CraftError::Io)?;

        // Save trust anchors
        let anchors_str = serde_json::to_string_pretty(&self.trust_anchors)
            .map_err(CraftError::Json)?;
        let mut af = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.supply_chain_trust_anchors_file)
            .map_err(CraftError::Io)?;
        af.write_all(anchors_str.as_bytes())
            .map_err(CraftError::Io)?;

        // Save verdicts registry
        let reg_str = toml::to_string_pretty(&self.verdicts)
            .map_err(CraftError::Toml)?;
        let mut rf = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.supply_chain_registry_file)
            .map_err(CraftError::Io)?;
        rf.write_all(reg_str.as_bytes())
            .map_err(CraftError::Io)?;

        let _ = lock_file.unlock();
        Ok(())
    }

    pub fn add_trust_anchor(&mut self, anchor: TrustAnchor) {
        self.trust_anchors.retain(|a| a.id != anchor.id);
        self.trust_anchors.push(anchor);
    }

    pub fn revoke_trust_anchor(&mut self, id: &str) -> bool {
        if let Some(anchor) = self.trust_anchors.iter_mut().find(|a| a.id == id) {
            anchor.is_revoked = true;
            true
        } else {
            false
        }
    }

    pub fn record_verdict(&mut self, verdict: VerificationVerdict) {
        self.verdicts.insert(verdict.artifact_sha256.clone(), verdict);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_slsa_level_ordering_and_parsing() {
        assert!(SlsaLevel::Slsa3 > SlsaLevel::Slsa2);
        assert!(SlsaLevel::Slsa4 > SlsaLevel::Slsa3);
        assert_eq!("slsa-3".parse::<SlsaLevel>().unwrap(), SlsaLevel::Slsa3);
        assert_eq!("2".parse::<SlsaLevel>().unwrap(), SlsaLevel::Slsa2);
        assert_eq!(SlsaLevel::Slsa1.to_string(), "SLSA-1");
    }

    #[test]
    fn test_in_toto_statement_generation_and_dsse_signing() {
        let subject = vec![Subject::new("server.jar", "abcd1234ef5678")];
        let predicate = SlsaPredicate {
            builder: BuilderInfo {
                id: "craft-hermetic-builder".to_string(),
                version: Some("1.0.0".to_string()),
            },
            build_type: "https://craft.rs/build/v1".to_string(),
            invocation: BuildInvocation {
                config_source: Some(ConfigSource {
                    uri: "git+https://github.com/example/repo".to_string(),
                    digest: HashMap::new(),
                    entry_point: "build.sh".to_string(),
                }),
                parameters: HashMap::new(),
                environment: HashMap::new(),
            },
            materials: vec![],
            metadata: Some(BuildMetadata {
                invocation_id: "build-001".to_string(),
                started_on: Some(Utc::now()),
                finished_on: Some(Utc::now()),
                reproducible: true,
                completeness: Some(BuildCompleteness {
                    parameters: true,
                    environment: true,
                    materials: true,
                }),
            }),
        };

        let statement = InTotoStatement::new(subject, predicate);
        assert_eq!(statement.determine_slsa_level(), SlsaLevel::Slsa3);

        let secret = b"my-secure-signing-key";
        let envelope = sign_in_toto_statement(&statement, "builder-key-1", secret).unwrap();

        assert_eq!(envelope.payload_type, DSSE_PAYLOAD_TYPE);
        assert_eq!(envelope.signatures.len(), 1);
        assert_eq!(envelope.signatures[0].keyid, "builder-key-1");

        let anchor = TrustAnchor::new(
            "builder-key-1",
            "hmac-sha256",
            "my-secure-signing-key",
            "craft-ca",
            "builder@craft.rs",
        );

        let payload_bytes = hex::decode(&envelope.payload).unwrap();
        let pae = dsse_pae(&envelope.payload_type, &payload_bytes);
        assert!(anchor.verify_signature(&pae, &envelope.signatures[0].sig));

        // Wrong secret should fail
        let bad_anchor = TrustAnchor::new(
            "builder-key-1",
            "hmac-sha256",
            "wrong-key",
            "craft-ca",
            "builder@craft.rs",
        );
        assert!(!bad_anchor.verify_signature(&pae, &envelope.signatures[0].sig));
    }

    #[test]
    fn test_transparency_log_merkle_inclusion_proof() {
        let leaf = [0x42u8; 32];
        let sibling1 = [0x11u8; 32];
        let sibling2 = [0x22u8; 32];

        let proof = create_inclusion_proof(2, &leaf, 4, vec![sibling1, sibling2]);
        assert!(proof.verify_inclusion(&leaf));

        // Tampered leaf should fail
        let bad_leaf = [0x43u8; 32];
        assert!(!proof.verify_inclusion(&bad_leaf));
    }

    #[test]
    fn test_supply_chain_registry_persistence() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        fs::create_dir_all(&paths.run_dir.join("locks")).unwrap();

        let mut reg = SupplyChainRegistry::default();
        reg.policy.enforcement_mode = EnforcementMode::StrictBlock;
        reg.add_trust_anchor(TrustAnchor::new(
            "key-1",
            "hmac-sha256",
            "secret",
            "issuer-1",
            "admin@example.com",
        ));

        reg.save(&paths).unwrap();

        let loaded = SupplyChainRegistry::load(&paths).unwrap();
        assert_eq!(loaded.policy.enforcement_mode, EnforcementMode::StrictBlock);
        assert_eq!(loaded.trust_anchors.len(), 1);
        assert_eq!(loaded.trust_anchors[0].id, "key-1");
    }
}
