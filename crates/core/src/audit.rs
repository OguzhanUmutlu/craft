use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const DEFAULT_AUDIT_SECRET: &[u8] = b"craft-audit-master-hmac-key";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub actor: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub previous_hash: String,
    pub entry_hash: String,
    pub signature: String,
}

impl AuditLogEntry {
    pub fn new(
        session_id: impl Into<String>,
        actor: impl Into<String>,
        action: impl Into<String>,
        resource: Option<String>,
        client_ip: Option<String>,
        status: impl Into<String>,
        details: Option<String>,
        previous_hash: &str,
        secret: &[u8],
    ) -> Self {
        let timestamp = Utc::now();
        let session_id = session_id.into();
        let actor = actor.into();
        let action = action.into();
        let status = status.into();

        let entry_hash = Self::calculate_hash(
            previous_hash,
            &timestamp,
            &session_id,
            &actor,
            &action,
            resource.as_deref(),
            client_ip.as_deref(),
            &status,
            details.as_deref(),
        );

        let signature = hex::encode(compute_hmac_sha256(secret, entry_hash.as_bytes()));

        Self {
            timestamp,
            session_id,
            actor,
            action,
            resource,
            client_ip,
            status,
            details,
            previous_hash: previous_hash.to_string(),
            entry_hash,
            signature,
        }
    }

    pub fn calculate_hash(
        previous_hash: &str,
        timestamp: &DateTime<Utc>,
        session_id: &str,
        actor: &str,
        action: &str,
        resource: Option<&str>,
        client_ip: Option<&str>,
        status: &str,
        details: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(previous_hash.as_bytes());
        hasher.update(b"|");
        hasher.update(timestamp.to_rfc3339().as_bytes());
        hasher.update(b"|");
        hasher.update(session_id.as_bytes());
        hasher.update(b"|");
        hasher.update(actor.as_bytes());
        hasher.update(b"|");
        hasher.update(action.as_bytes());
        hasher.update(b"|");
        hasher.update(resource.unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(client_ip.unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(status.as_bytes());
        hasher.update(b"|");
        hasher.update(details.unwrap_or("").as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn verify_integrity(&self, secret: &[u8]) -> bool {
        let expected_hash = Self::calculate_hash(
            &self.previous_hash,
            &self.timestamp,
            &self.session_id,
            &self.actor,
            &self.action,
            self.resource.as_deref(),
            self.client_ip.as_deref(),
            &self.status,
            self.details.as_deref(),
        );

        if self.entry_hash != expected_hash {
            return false;
        }

        let expected_signature =
            hex::encode(compute_hmac_sha256(secret, self.entry_hash.as_bytes()));
        self.signature == expected_signature
    }
}

pub fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditVerificationResult {
    pub is_valid: bool,
    pub total_entries: usize,
    pub verified_entries: usize,
    pub corrupted_index: Option<usize>,
    pub error_message: Option<String>,
}

pub struct AuditLedger;

impl AuditLedger {
    pub fn append(
        paths: &CraftPaths,
        session_id: &str,
        actor: &str,
        action: &str,
        resource: Option<String>,
        client_ip: Option<String>,
        status: &str,
        details: Option<String>,
        secret: &[u8],
    ) -> Result<AuditLogEntry> {
        let lock_path = paths.locks_dir.join("audit.lock");
        if let Some(p) = lock_path.parent() {
            fs::create_dir_all(p)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        lock_file.lock_exclusive()?;

        let res = Self::append_internal(
            paths, session_id, actor, action, resource, client_ip, status, details, secret,
        );

        let _ = lock_file.unlock();
        res
    }

    fn append_internal(
        paths: &CraftPaths,
        session_id: &str,
        actor: &str,
        action: &str,
        resource: Option<String>,
        client_ip: Option<String>,
        status: &str,
        details: Option<String>,
        secret: &[u8],
    ) -> Result<AuditLogEntry> {
        let previous_hash = if paths.audit_file.exists() {
            let file = File::open(&paths.audit_file)?;
            let reader = BufReader::new(file);
            let last_line = reader.lines().filter_map(|l| l.ok()).last();
            if let Some(line) = last_line {
                if let Ok(entry) = serde_json::from_str::<AuditLogEntry>(&line) {
                    entry.entry_hash
                } else {
                    GENESIS_HASH.to_string()
                }
            } else {
                GENESIS_HASH.to_string()
            }
        } else {
            GENESIS_HASH.to_string()
        };

        let entry = AuditLogEntry::new(
            session_id,
            actor,
            action,
            resource,
            client_ip,
            status,
            details,
            &previous_hash,
            secret,
        );

        let serialized = serde_json::to_string(&entry)
            .map_err(|e| CraftError::Other(format!("Failed to serialize audit entry: {}", e)))?;

        if let Some(parent) = paths.audit_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.audit_file)?;

        writeln!(file, "{}", serialized)?;
        file.flush()?;

        Ok(entry)
    }

    pub fn read_entries(paths: &CraftPaths, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        if !paths.audit_file.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&paths.audit_file)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<AuditLogEntry>(trimmed) {
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

    pub fn verify_chain(paths: &CraftPaths, secret: &[u8]) -> Result<AuditVerificationResult> {
        let entries = Self::read_entries(paths, None)?;
        let total = entries.len();

        if total == 0 {
            return Ok(AuditVerificationResult {
                is_valid: true,
                total_entries: 0,
                verified_entries: 0,
                corrupted_index: None,
                error_message: None,
            });
        }

        let mut expected_prev_hash = GENESIS_HASH.to_string();

        for (idx, entry) in entries.iter().enumerate() {
            if entry.previous_hash != expected_prev_hash {
                return Ok(AuditVerificationResult {
                    is_valid: false,
                    total_entries: total,
                    verified_entries: idx,
                    corrupted_index: Some(idx),
                    error_message: Some(format!(
                        "Chain link broken at entry {}: previous_hash mismatch",
                        idx
                    )),
                });
            }

            if !entry.verify_integrity(secret) {
                return Ok(AuditVerificationResult {
                    is_valid: false,
                    total_entries: total,
                    verified_entries: idx,
                    corrupted_index: Some(idx),
                    error_message: Some(format!(
                        "Integrity signature verification failed at entry {}",
                        idx
                    )),
                });
            }

            expected_prev_hash = entry.entry_hash.clone();
        }

        Ok(AuditVerificationResult {
            is_valid: true,
            total_entries: total,
            verified_entries: total,
            corrupted_index: None,
            error_message: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_hash_chain_and_verification() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());
        let secret = b"super-secure-audit-hmac-key";

        // Append 3 records
        let e1 = AuditLedger::append(
            &paths,
            "session-1",
            "admin",
            "server.start",
            Some("lobby".to_string()),
            Some("127.0.0.1".to_string()),
            "SUCCESS",
            None,
            secret,
        )
        .unwrap();

        assert_eq!(e1.previous_hash, GENESIS_HASH);

        let e2 = AuditLedger::append(
            &paths,
            "session-1",
            "admin",
            "server.stop",
            Some("lobby".to_string()),
            Some("127.0.0.1".to_string()),
            "SUCCESS",
            None,
            secret,
        )
        .unwrap();

        assert_eq!(e2.previous_hash, e1.entry_hash);

        let e3 = AuditLedger::append(
            &paths,
            "session-2",
            "operator",
            "backup.create",
            Some("survival".to_string()),
            None,
            "SUCCESS",
            Some("zstd snapshot".to_string()),
            secret,
        )
        .unwrap();

        assert_eq!(e3.previous_hash, e2.entry_hash);

        // Verify chain
        let res = AuditLedger::verify_chain(&paths, secret).unwrap();
        assert!(res.is_valid);
        assert_eq!(res.total_entries, 3);
        assert_eq!(res.verified_entries, 3);

        // Tamper with audit.log
        let content = fs::read_to_string(&paths.audit_file).unwrap();
        let tampered = content.replace("server.start", "server.destroy");
        fs::write(&paths.audit_file, tampered).unwrap();

        // Verification should immediately detect corruption!
        let tampered_res = AuditLedger::verify_chain(&paths, secret).unwrap();
        assert!(!tampered_res.is_valid);
        assert_eq!(tampered_res.corrupted_index, Some(0));
    }
}
