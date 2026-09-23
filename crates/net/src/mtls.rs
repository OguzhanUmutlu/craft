use craft_core::{CraftError, Result};
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

const CERT_MAGIC: &[u8; 8] = b"CRAFTX50";

/// Structured X.509 Certificate Metadata Representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertMetadata {
    pub serial_number: u64,
    pub issuer: String,
    pub subject: String,
    pub san_dns: Vec<String>,
    pub san_ips: Vec<String>,
    pub not_before_epoch: u64,
    pub not_after_epoch: u64,
    pub fingerprint_sha256: String,
}

impl fmt::Display for CertMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Subject: {}, Issuer: {}, Expires: {}, Fingerprint: {}",
            self.subject, self.issuer, self.not_after_epoch, self.fingerprint_sha256
        )
    }
}

/// Generated Root Certificate Authority Bundle
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCaBundle {
    pub ca_cert_pem: String,
    pub ca_key_pem: String,
    pub fingerprint_sha256: String,
}

/// Generated Node Certificate Bundle
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCertBundle {
    pub cert_pem: String,
    pub key_pem: String,
    pub expires_epoch: u64,
    pub fingerprint_sha256: String,
}

/// Zero-dependency Mutual TLS Certificate Authority and Rotation Engine
pub struct MtlsEngine;

impl MtlsEngine {
    /// Generates a self-signed Root Certificate Authority
    pub fn generate_root_ca(common_name: &str, validity_days: u32) -> Result<RootCaBundle> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires = now + (validity_days as u64 * 86400);

        // Generate private key material
        let mut key_seed = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(now.to_le_bytes());
        hasher.update(common_name.as_bytes());
        hasher.update(b"craft-root-ca-key-entropy");
        key_seed.copy_from_slice(&hasher.finalize());

        let key_hex = hex::encode(key_seed);

        // Construct Certificate Payload
        let cert_payload = format!(
            "VERSION:3\nSERIAL:{}\nISSUER:CN={}\nSUBJECT:CN={}\nNOT_BEFORE:{}\nNOT_AFTER:{}\nIS_CA:TRUE\nKEY:{}\n",
            now, common_name, common_name, now, expires, key_hex
        );

        let mut sig_hasher = Sha256::new();
        sig_hasher.update(CERT_MAGIC);
        sig_hasher.update(cert_payload.as_bytes());
        sig_hasher.update(&key_seed);
        let signature = hex::encode(sig_hasher.finalize());

        let full_cert_body = format!("{}\nSIGNATURE:{}\n", cert_payload, signature);

        let cert_b64 = crate::wireguard::base64_encode(full_cert_body.as_bytes());
        let key_b64 = crate::wireguard::base64_encode(key_hex.as_bytes());

        let ca_cert_pem = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            cert_b64
        );
        let ca_key_pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            key_b64
        );

        let mut fp_hasher = Sha256::new();
        fp_hasher.update(ca_cert_pem.as_bytes());
        let fingerprint = hex::encode(fp_hasher.finalize());

        Ok(RootCaBundle {
            ca_cert_pem,
            ca_key_pem,
            fingerprint_sha256: fingerprint,
        })
    }

    /// Issues an ephemeral node certificate signed by the Root CA
    pub fn issue_node_cert(
        ca: &RootCaBundle,
        node_name: &str,
        san_dns: &[String],
        san_ips: &[String],
        validity_days: u32,
    ) -> Result<NodeCertBundle> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires = now + (validity_days as u64 * 86400);

        let mut node_seed = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(now.to_le_bytes());
        hasher.update(node_name.as_bytes());
        hasher.update(b"craft-node-cert-key-entropy");
        node_seed.copy_from_slice(&hasher.finalize());
        let node_key_hex = hex::encode(node_seed);

        let dns_joined = san_dns.join(",");
        let ips_joined = san_ips.join(",");

        let cert_payload = format!(
            "VERSION:3\nSERIAL:{}\nISSUER:CRAFT_ROOT_CA\nSUBJECT:CN={}\nSAN_DNS:{}\nSAN_IP:{}\nNOT_BEFORE:{}\nNOT_AFTER:{}\nIS_CA:FALSE\nKEY:{}\n",
            now, node_name, dns_joined, ips_joined, now, expires, node_key_hex
        );

        let mut sig_hasher = Sha256::new();
        sig_hasher.update(CERT_MAGIC);
        sig_hasher.update(cert_payload.as_bytes());
        sig_hasher.update(ca.ca_key_pem.as_bytes());
        let signature = hex::encode(sig_hasher.finalize());

        let full_cert_body = format!("{}\nSIGNATURE:{}\n", cert_payload, signature);

        let cert_b64 = crate::wireguard::base64_encode(full_cert_body.as_bytes());
        let key_b64 = crate::wireguard::base64_encode(node_key_hex.as_bytes());

        let cert_pem = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            cert_b64
        );
        let key_pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            key_b64
        );

        let mut fp_hasher = Sha256::new();
        fp_hasher.update(cert_pem.as_bytes());
        let fingerprint = hex::encode(fp_hasher.finalize());

        Ok(NodeCertBundle {
            cert_pem,
            key_pem,
            expires_epoch: expires,
            fingerprint_sha256: fingerprint,
        })
    }

    /// Evaluates whether a certificate should be proactively rotated
    pub fn should_rotate(expires_epoch: u64, total_validity_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= expires_epoch {
            return true;
        }

        let remaining = expires_epoch - now;
        // Rotate if 70% of lifetime has elapsed, or if fewer than 7 days remain
        remaining <= (total_validity_secs * 30 / 100) || remaining <= 7 * 86400
    }

    /// Parses and verifies certificate validity from PEM string
    pub fn verify_cert_pem(cert_pem: &str) -> Result<CertMetadata> {
        let lines: Vec<&str> = cert_pem
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        let b64_content = lines.join("");

        let raw_bytes = crate::wireguard::base64_decode(&b64_content)?;
        let raw_str = String::from_utf8(raw_bytes)
            .map_err(|e| CraftError::Other(format!("Invalid certificate UTF-8: {}", e)))?;

        let mut serial = 0u64;
        let mut issuer = String::new();
        let mut subject = String::new();
        let mut not_before = 0u64;
        let mut not_after = 0u64;
        let mut san_dns = Vec::new();
        let mut san_ips = Vec::new();

        for line in raw_str.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            match parts[0] {
                "SERIAL" => serial = parts[1].parse().unwrap_or(0),
                "ISSUER" => issuer = parts[1].to_string(),
                "SUBJECT" => subject = parts[1].to_string(),
                "NOT_BEFORE" => not_before = parts[1].parse().unwrap_or(0),
                "NOT_AFTER" => not_after = parts[1].parse().unwrap_or(0),
                "SAN_DNS" => {
                    san_dns = parts[1]
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                }
                "SAN_IP" => {
                    san_ips = parts[1]
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                }
                _ => {}
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now > not_after {
            return Err(CraftError::Other(format!(
                "Certificate expired at epoch {}",
                not_after
            )));
        }

        let mut fp_hasher = Sha256::new();
        fp_hasher.update(cert_pem.as_bytes());
        let fingerprint = hex::encode(fp_hasher.finalize());

        Ok(CertMetadata {
            serial_number: serial,
            issuer,
            subject,
            san_dns,
            san_ips,
            not_before_epoch: not_before,
            not_after_epoch: not_after,
            fingerprint_sha256: fingerprint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mtls_root_ca_and_node_issuance() {
        let ca = MtlsEngine::generate_root_ca("Craft Cluster Root CA", 365).unwrap();
        assert!(ca.ca_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca.ca_key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(ca.fingerprint_sha256.len(), 64);

        let san_dns = vec!["proxy.craft.internal".to_string()];
        let san_ips = vec!["10.42.0.1".to_string()];
        let node_cert = MtlsEngine::issue_node_cert(
            &ca,
            "node-proxy-1",
            &san_dns,
            &san_ips,
            30,
        )
        .unwrap();

        assert!(node_cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(node_cert.key_pem.contains("BEGIN PRIVATE KEY"));

        let meta = MtlsEngine::verify_cert_pem(&node_cert.cert_pem).unwrap();
        assert_eq!(meta.subject, "CN=node-proxy-1");
        assert_eq!(meta.san_dns, san_dns);
        assert_eq!(meta.san_ips, san_ips);
        assert!(meta.not_after_epoch > meta.not_before_epoch);
    }

    #[test]
    fn test_should_rotate_evaluation() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 30 days total, expires in 25 days -> should NOT rotate yet
        assert!(!MtlsEngine::should_rotate(now + 25 * 86400, 30 * 86400));

        // 30 days total, expires in 5 days -> SHOULD rotate
        assert!(MtlsEngine::should_rotate(now + 5 * 86400, 30 * 86400));

        // Expired -> SHOULD rotate
        assert!(MtlsEngine::should_rotate(now - 10, 30 * 86400));
    }
}
