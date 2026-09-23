use crate::hermetic::HermeticBuildRunner;
use craft_core::{
    compute_file_sha256, evaluate_supply_chain, CraftError, CraftPaths, DsseEnvelope,
    EnforcementMode, HermeticBuildManifest, InTotoStatement, Result, SigstoreBundle,
    SupplyChainPolicy, SupplyChainRegistry, VerificationVerdict,
};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{debug, info, warn};

static INSTANCE: OnceLock<Arc<SupplyChainService>> = OnceLock::new();

pub struct SupplyChainService {
    paths: CraftPaths,
    registry: Arc<RwLock<SupplyChainRegistry>>,
    verification_counter: AtomicU64,
    violations_counter: AtomicU64,
    reproducible_builds_counter: AtomicU64,
}

impl SupplyChainService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = SupplyChainRegistry::load(&paths).unwrap_or_default();
        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            verification_counter: AtomicU64::new(0),
            violations_counter: AtomicU64::new(0),
            reproducible_builds_counter: AtomicU64::new(0),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn verify_artifact(
        &self,
        artifact_path: &Path,
        attestation_path_opt: Option<&Path>,
        strict: bool,
    ) -> Result<VerificationVerdict> {
        info!(
            "[SUPPLY-CHAIN] Verifying artifact '{}' (strict: {})",
            artifact_path.display(),
            strict
        );

        let sha256 = if artifact_path.exists() {
            compute_file_sha256(artifact_path)?
        } else {
            return Err(CraftError::InvalidPath(format!(
                "Artifact path does not exist: {}",
                artifact_path.display()
            )));
        };

        // Locate attestation file
        let mut resolved_attestation_path = None;
        if let Some(p) = attestation_path_opt {
            if p.exists() {
                resolved_attestation_path = Some(p.to_path_buf());
            }
        }

        if resolved_attestation_path.is_none() {
            // Check default paths
            let by_sha = self.paths.supply_chain_attestation_path(&sha256);
            if by_sha.exists() {
                resolved_attestation_path = Some(by_sha);
            } else {
                let sidecar1 = artifact_path.with_extension("attestation.json");
                let sidecar2 = Path::new(&format!("{}.json", artifact_path.display())).to_path_buf();
                if sidecar1.exists() {
                    resolved_attestation_path = Some(sidecar1);
                } else if sidecar2.exists() {
                    resolved_attestation_path = Some(sidecar2);
                }
            }
        }

        let mut statement = None;
        let mut envelope = None;
        let mut tlog_entry = None;

        if let Some(ref att_path) = resolved_attestation_path {
            if let Ok(content) = fs::read_to_string(att_path) {
                // Try parsing as SigstoreBundle
                if let Ok(bundle) = serde_json::from_str::<SigstoreBundle>(&content) {
                    if let Ok(extracted) = bundle.dsse_envelope.extract_statement() {
                        statement = Some(extracted);
                    }
                    if let Some(entry) = bundle.verification_material.tlog_entries.first() {
                        tlog_entry = Some(entry.clone());
                    }
                    envelope = Some(bundle.dsse_envelope);
                } else if let Ok(env) = serde_json::from_str::<DsseEnvelope>(&content) {
                    if let Ok(extracted) = env.extract_statement() {
                        statement = Some(extracted);
                    }
                    envelope = Some(env);
                } else if let Ok(stmt) = serde_json::from_str::<InTotoStatement>(&content) {
                    statement = Some(stmt);
                }
            }
        }

        let mut reg = self.registry.write().map_err(|_| {
            CraftError::Other("Supply chain registry rwlock poisoned".to_string())
        })?;

        let mut policy = reg.policy.clone();
        if strict {
            policy.enforcement_mode = EnforcementMode::StrictBlock;
        }

        let verdict = evaluate_supply_chain(
            artifact_path,
            statement.as_ref(),
            envelope.as_ref(),
            tlog_entry.as_ref(),
            &policy,
            &reg.trust_anchors,
        )?;

        self.verification_counter.fetch_add(1, Ordering::SeqCst);
        if !verdict.verified {
            self.violations_counter.fetch_add(1, Ordering::SeqCst);
            warn!(
                "[SUPPLY-CHAIN] Policy violation for artifact '{}': {:?}",
                artifact_path.display(),
                verdict.reasons
            );
        } else {
            debug!(
                "[SUPPLY-CHAIN] Artifact '{}' successfully verified at level {}",
                artifact_path.display(),
                verdict.slsa_level
            );
        }

        reg.record_verdict(verdict.clone());
        let _ = reg.save(&self.paths);

        Ok(verdict)
    }

    pub fn inspect_attestation(&self, identifier: &str) -> Result<Option<InTotoStatement>> {
        let path = Path::new(identifier);
        let target_file = if path.exists() {
            path.to_path_buf()
        } else {
            self.paths.supply_chain_attestation_path(identifier)
        };

        if !target_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&target_file).map_err(CraftError::Io)?;

        if let Ok(bundle) = serde_json::from_str::<SigstoreBundle>(&content) {
            if let Ok(extracted) = bundle.dsse_envelope.extract_statement() {
                return Ok(Some(extracted));
            }
        }
        if let Ok(env) = serde_json::from_str::<DsseEnvelope>(&content) {
            if let Ok(extracted) = env.extract_statement() {
                return Ok(Some(extracted));
            }
        }
        if let Ok(stmt) = serde_json::from_str::<InTotoStatement>(&content) {
            return Ok(Some(stmt));
        }

        Err(CraftError::Other(format!(
            "Failed to parse attestation format in '{}'",
            target_file.display()
        )))
    }

    pub fn get_policy(&self) -> Result<(SupplyChainPolicy, usize)> {
        let reg = self.registry.read().map_err(|_| {
            CraftError::Other("Supply chain registry rwlock poisoned".to_string())
        })?;
        Ok((reg.policy.clone(), reg.trust_anchors.len()))
    }

    pub fn set_policy(&self, policy: SupplyChainPolicy) -> Result<()> {
        let mut reg = self.registry.write().map_err(|_| {
            CraftError::Other("Supply chain registry rwlock poisoned".to_string())
        })?;
        reg.policy = policy;
        reg.save(&self.paths)?;
        info!("[SUPPLY-CHAIN] Updated policy: mode={}", reg.policy.enforcement_mode);
        Ok(())
    }

    pub async fn run_hermetic_build(
        &self,
        build_dir: &Path,
        command: &str,
        args: &[String],
        allow_network: bool,
    ) -> Result<HermeticBuildManifest> {
        let manifest = HermeticBuildRunner::run_build(
            build_dir,
            command,
            args,
            1704067200,
            allow_network,
        )
        .await?;

        self.reproducible_builds_counter
            .fetch_add(1, Ordering::SeqCst);

        Ok(manifest)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(512);

        let verifs = self.verification_counter.load(Ordering::Relaxed);
        let viols = self.violations_counter.load(Ordering::Relaxed);
        let hermetic = self.reproducible_builds_counter.load(Ordering::Relaxed);

        let _ = writeln!(
            out,
            "# HELP craft_supply_chain_verifications_total Total cryptographic supply chain verifications executed"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_supply_chain_verifications_total counter"
        );
        let _ = writeln!(out, "craft_supply_chain_verifications_total {}", verifs);

        let _ = writeln!(
            out,
            "# HELP craft_supply_chain_policy_violations_total Total supply chain policy violation blocks"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_supply_chain_policy_violations_total counter"
        );
        let _ = writeln!(out, "craft_supply_chain_policy_violations_total {}", viols);

        let _ = writeln!(
            out,
            "# HELP craft_supply_chain_reproducible_builds_total Total hermetic bit-for-bit reproducible builds completed"
        );
        let _ = writeln!(
            out,
            "# TYPE craft_supply_chain_reproducible_builds_total counter"
        );
        let _ = writeln!(
            out,
            "craft_supply_chain_reproducible_builds_total {}",
            hermetic
        );

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::{
        sign_in_toto_statement, BuilderInfo, InTotoStatement, SlsaLevel, SlsaPredicate, Subject,
        TrustAnchor,
    };
    use tempfile::tempdir;

    #[test]
    fn test_service_verification_audit_mode() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let jar_path = temp.path().join("server.jar");
        fs::write(&jar_path, b"test server jar bytes").unwrap();

        let service = SupplyChainService::new(paths);
        let verdict = service.verify_artifact(&jar_path, None, false).unwrap();

        // Under AuditOnly default policy, unverified artifacts are reported as not verified without throwing an error
        assert_eq!(verdict.slsa_level, SlsaLevel::None);
    }

    #[test]
    fn test_service_verification_strict_signed() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let jar_path = temp.path().join("plugin.jar");
        fs::write(&jar_path, b"plugin binary").unwrap();
        let jar_sha256 = compute_file_sha256(&jar_path).unwrap();

        let service = SupplyChainService::new(paths.clone());
        {
            let mut reg = service.registry.write().unwrap();
            reg.policy.enforcement_mode = EnforcementMode::StrictBlock;
            reg.policy.minimum_slsa_level = SlsaLevel::Slsa1;
            reg.policy.allowed_builders = vec!["craft-cli".to_string()];
            reg.add_trust_anchor(TrustAnchor::new(
                "key-1",
                "hmac-sha256",
                "secret-key",
                "craft-ca",
                "developer@craft.rs",
            ));
        }

        // Create statement & sign
        let stmt = InTotoStatement::new(
            vec![Subject::new("plugin.jar", &jar_sha256)],
            SlsaPredicate {
                builder: BuilderInfo {
                    id: "craft-cli".to_string(),
                    version: Some("1.0.0".to_string()),
                },
                build_type: "https://craft.rs/build/v1".to_string(),
                invocation: craft_core::BuildInvocation {
                    config_source: None,
                    parameters: std::collections::HashMap::new(),
                    environment: std::collections::HashMap::new(),
                },
                materials: vec![],
                metadata: None,
            },
        );

        let env = sign_in_toto_statement(&stmt, "key-1", b"secret-key").unwrap();
        let att_path = temp.path().join("plugin.jar.json");
        fs::write(&att_path, serde_json::to_string(&env).unwrap()).unwrap();

        let verdict = service
            .verify_artifact(&jar_path, Some(&att_path), true)
            .unwrap();
        assert!(verdict.verified);
        assert_eq!(verdict.slsa_level, SlsaLevel::Slsa1);
        assert_eq!(verdict.signer_identity.as_deref(), Some("developer@craft.rs"));
    }
}
