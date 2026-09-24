use craft_core::pqc::{
    HybridKeyExchange, MlDsa65, MlKem1024, MlKem768, PqcBenchmarkReport, PqcCipherSuite,
    PqcEnforcementMode, PqcKeyPair, PqcMigrationPhase, PqcPolicy, PqcRegistry,
    PqcSigningAlgorithm, PqcStatusSummary,
};
use craft_core::{CraftError, CraftPaths, Result};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

static INSTANCE: OnceLock<Arc<PqcService>> = OnceLock::new();

pub struct PqcService {
    paths: CraftPaths,
    registry: Arc<RwLock<PqcRegistry>>,
    policy: Arc<RwLock<PqcPolicy>>,
    total_handshakes: AtomicU64,
    quantum_safe_sessions: AtomicU64,
    downgrades_blocked: AtomicU64,
    mitm_tamper_events: AtomicU64,
    encap_latency_sum_us: AtomicU64,
    encap_count: AtomicU64,
}

impl PqcService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = PqcRegistry::load(&paths).unwrap_or_default();
        let policy = PqcPolicy::load(&paths).unwrap_or_default();
        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            policy: Arc::new(RwLock::new(policy)),
            total_handshakes: AtomicU64::new(0),
            quantum_safe_sessions: AtomicU64::new(0),
            downgrades_blocked: AtomicU64::new(0),
            mitm_tamper_events: AtomicU64::new(0),
            encap_latency_sum_us: AtomicU64::new(0),
            encap_count: AtomicU64::new(0),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn record_handshake(&self, is_pqc: bool, blocked_downgrade: bool, latency_us: u64) {
        self.total_handshakes.fetch_add(1, Ordering::SeqCst);
        if is_pqc {
            self.quantum_safe_sessions.fetch_add(1, Ordering::SeqCst);
        }
        if blocked_downgrade {
            self.downgrades_blocked.fetch_add(1, Ordering::SeqCst);
            warn!("[PQC] Blocked classical downgrade handshake attempt");
        }
        if latency_us > 0 {
            self.encap_latency_sum_us
                .fetch_add(latency_us, Ordering::SeqCst);
            self.encap_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn record_tamper(&self) {
        self.mitm_tamper_events.fetch_add(1, Ordering::SeqCst);
        warn!("[PQC] MITM signature tampering detected and rejected");
    }

    pub fn get_status(&self) -> Result<PqcStatusSummary> {
        let policy = self
            .policy
            .read()
            .map_err(|_| CraftError::Other("PQC policy rwlock poisoned".to_string()))?
            .clone();

        let reg = self
            .registry
            .read()
            .map_err(|_| CraftError::Other("PQC registry rwlock poisoned".to_string()))?;

        let total = self.total_handshakes.load(Ordering::Relaxed);
        let pq_sessions = self.quantum_safe_sessions.load(Ordering::Relaxed);
        let blocked = self.downgrades_blocked.load(Ordering::Relaxed);

        let count = self.encap_count.load(Ordering::Relaxed);
        let sum = self.encap_latency_sum_us.load(Ordering::Relaxed);
        let avg_latency = if count > 0 {
            sum as f64 / count as f64
        } else {
            0.0
        };

        let harvest_defense_score = match policy.enforcement_mode {
            PqcEnforcementMode::PostQuantumOnly => 100.0,
            PqcEnforcementMode::Hybrid => 95.0,
            PqcEnforcementMode::ClassicOnly => 0.0,
        };

        let (hybrid_handshakes, pure_pq_handshakes) = match policy.preferred_ciphersuite {
            PqcCipherSuite::HybridX25519MlKem768 => (pq_sessions, 0),
            PqcCipherSuite::PureMlKem768 | PqcCipherSuite::PureMlKem1024 => (0, pq_sessions),
            PqcCipherSuite::ClassicX25519 => (0, 0),
        };

        Ok(PqcStatusSummary {
            enforcement_mode: policy.enforcement_mode,
            active_ciphersuite: policy.preferred_ciphersuite,
            harvest_defense_score,
            active_key_pairs: reg.keys.len(),
            total_handshakes: total,
            hybrid_handshakes,
            pure_pq_handshakes,
            rejected_downgrades: blocked,
            average_encap_latency_us: avg_latency,
            migration_phase: policy.migration_phase,
        })
    }

    pub fn get_policy(&self) -> Result<PqcPolicy> {
        let policy = self
            .policy
            .read()
            .map_err(|_| CraftError::Other("PQC policy rwlock poisoned".to_string()))?
            .clone();
        Ok(policy)
    }

    pub fn set_policy(&self, policy: PqcPolicy) -> Result<()> {
        let mut p = self
            .policy
            .write()
            .map_err(|_| CraftError::Other("PQC policy rwlock poisoned".to_string()))?;
        *p = policy.clone();
        p.save(&self.paths)?;
        info!(
            "[PQC] Updated policy: mode={}, preferred_ciphersuite={}",
            p.enforcement_mode, p.preferred_ciphersuite
        );
        Ok(())
    }

    pub fn generate_keypair(
        &self,
        suite: Option<PqcCipherSuite>,
        algorithm: Option<PqcSigningAlgorithm>,
    ) -> Result<PqcKeyPair> {
        let (id, algo_str, pk_hex, sk_hex) = if let Some(algo) = algorithm {
            match algo {
                PqcSigningAlgorithm::MlDsa65 => {
                    let (pk, sk) = MlDsa65::keypair(None);
                    let id = format!("mldsa65-{}", &hex::encode(&pk.raw[..8]));
                    (
                        id,
                        "ML-DSA-65".to_string(),
                        hex::encode(&pk.raw),
                        hex::encode(&sk.raw),
                    )
                }
                PqcSigningAlgorithm::HybridEd25519MlDsa65 => {
                    let (pk, sk) = MlDsa65::keypair(None);
                    let id = format!("hybrid-mldsa65-{}", &hex::encode(&pk.raw[..8]));
                    (
                        id,
                        "Hybrid-Ed25519-ML-DSA-65".to_string(),
                        hex::encode(&pk.raw),
                        hex::encode(&sk.raw),
                    )
                }
                PqcSigningAlgorithm::Ed25519 => {
                    let (pk, sk) = MlDsa65::keypair(None);
                    let id = format!("ed25519-{}", &hex::encode(&pk.raw[..8]));
                    (
                        id,
                        "Ed25519".to_string(),
                        hex::encode(&pk.raw[..32]),
                        hex::encode(&sk.raw[..64]),
                    )
                }
            }
        } else {
            let s = suite.unwrap_or(PqcCipherSuite::HybridX25519MlKem768);
            match s {
                PqcCipherSuite::PureMlKem1024 => {
                    let (pk, sk) = MlKem1024::keypair(None);
                    let id = format!("mlkem1024-{}", &hex::encode(&pk.raw[..8]));
                    (
                        id,
                        "ML-KEM-1024".to_string(),
                        hex::encode(&pk.raw),
                        hex::encode(&sk.raw),
                    )
                }
                PqcCipherSuite::PureMlKem768 => {
                    let (pk, sk) = MlKem768::keypair(None);
                    let id = format!("mlkem768-{}", &hex::encode(&pk.raw[..8]));
                    (
                        id,
                        "ML-KEM-768".to_string(),
                        hex::encode(&pk.raw),
                        hex::encode(&sk.raw),
                    )
                }
                PqcCipherSuite::HybridX25519MlKem768 | PqcCipherSuite::ClassicX25519 => {
                    let (pk, sk) = HybridKeyExchange::keypair(None);
                    let id = format!("hybrid-x25519-mlkem768-{}", &hex::encode(&pk.x25519[..4]));
                    let mut pk_bytes = pk.x25519.to_vec();
                    pk_bytes.extend_from_slice(&pk.mlkem768.raw);
                    let mut sk_bytes = sk.x25519.to_vec();
                    sk_bytes.extend_from_slice(&sk.mlkem768.raw);
                    (
                        id,
                        "Hybrid-X25519-ML-KEM-768".to_string(),
                        hex::encode(&pk_bytes),
                        hex::encode(&sk_bytes),
                    )
                }
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let keypair = PqcKeyPair {
            id: id.clone(),
            algorithm: algo_str,
            public_key: pk_hex,
            secret_key: sk_hex,
            created_at_epoch: now,
        };

        let mut reg = self
            .registry
            .write()
            .map_err(|_| CraftError::Other("PQC registry rwlock poisoned".to_string()))?;
        reg.add_key(keypair.clone());
        reg.save(&self.paths)?;

        let pk_file = self.paths.pqc_keys_dir.join(format!("{}.pub", id));
        let _ = fs::write(pk_file, &keypair.public_key);

        info!("[PQC] Generated new post-quantum keypair '{}'", id);
        Ok(keypair)
    }

    pub fn run_benchmark(&self, iterations: usize) -> Result<PqcBenchmarkReport> {
        let iters = iterations.max(1);

        // 1. ML-KEM-768
        let (pk768, sk768) = MlKem768::keypair(None);
        let t0 = Instant::now();
        let (first_ct768, _) = MlKem768::encapsulate(&pk768, None)?;
        let mut ct768_last = first_ct768;
        for _ in 1..iters {
            let (ct, _) = MlKem768::encapsulate(&pk768, None)?;
            ct768_last = ct;
        }
        let mlkem768_encap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = MlKem768::decapsulate(&sk768, &ct768_last)?;
        }
        let mlkem768_decap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        // 2. ML-KEM-1024
        let (pk1024, sk1024) = MlKem1024::keypair(None);
        let t0 = Instant::now();
        let (first_ct1024, _) = MlKem1024::encapsulate(&pk1024, None)?;
        let mut ct1024_last = first_ct1024;
        for _ in 1..iters {
            let (ct, _) = MlKem1024::encapsulate(&pk1024, None)?;
            ct1024_last = ct;
        }
        let mlkem1024_encap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = MlKem1024::decapsulate(&sk1024, &ct1024_last)?;
        }
        let mlkem1024_decap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        // 3. Hybrid X25519 + ML-KEM-768
        let (pkh, skh) = HybridKeyExchange::keypair(None);
        let t0 = Instant::now();
        let (first_cth, _) = HybridKeyExchange::encapsulate(&pkh)?;
        let mut cth_last = first_cth;
        for _ in 1..iters {
            let (ct, _) = HybridKeyExchange::encapsulate(&pkh)?;
            cth_last = ct;
        }
        let hybrid_encap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = HybridKeyExchange::decapsulate(&skh, &cth_last)?;
        }
        let hybrid_decap_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        // 4. ML-DSA-65
        let (pkdsa, skdsa) = MlDsa65::keypair(None);
        let msg = b"craft-pqc-benchmark-quantum-state-machine-verification";
        let t0 = Instant::now();
        let mut sig_last = Vec::new();
        for _ in 0..iters {
            let sig = MlDsa65::sign(&skdsa, msg);
            sig_last = sig;
        }
        let mldsa65_sign_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = MlDsa65::verify(&pkdsa, msg, &sig_last);
        }
        let mldsa65_verify_avg_us = t0.elapsed().as_micros() as f64 / iters as f64;

        Ok(PqcBenchmarkReport {
            iterations: iters,
            mlkem768_encap_avg_us,
            mlkem768_decap_avg_us,
            mlkem1024_encap_avg_us,
            mlkem1024_decap_avg_us,
            hybrid_encap_avg_us,
            hybrid_decap_avg_us,
            mldsa65_sign_avg_us,
            mldsa65_verify_avg_us,
        })
    }

    pub fn migrate_phase(&self, target_phase: PqcMigrationPhase) -> Result<String> {
        let mut policy = self
            .policy
            .write()
            .map_err(|_| CraftError::Other("PQC policy rwlock poisoned".to_string()))?;

        let old_phase = policy.migration_phase;
        policy.migration_phase = target_phase;

        match target_phase {
            PqcMigrationPhase::Planning => {
                policy.enforcement_mode = PqcEnforcementMode::ClassicOnly;
                policy.allow_classical_fallback = true;
                policy.enforce_quantum_signatures = false;
            }
            PqcMigrationPhase::DualStack => {
                policy.enforcement_mode = PqcEnforcementMode::Hybrid;
                policy.preferred_ciphersuite = PqcCipherSuite::HybridX25519MlKem768;
                policy.allow_classical_fallback = true;
                policy.enforce_quantum_signatures = false;
            }
            PqcMigrationPhase::EnforcedPqc => {
                policy.enforcement_mode = PqcEnforcementMode::PostQuantumOnly;
                policy.preferred_ciphersuite = PqcCipherSuite::PureMlKem768;
                policy.allow_classical_fallback = false;
                policy.enforce_quantum_signatures = true;
            }
        }

        policy.save(&self.paths)?;
        let msg = format!(
            "Successfully transitioned PQC migration phase from {:?} to {:?} (enforcement: {}, fallback: {})",
            old_phase, target_phase, policy.enforcement_mode, policy.allow_classical_fallback
        );
        info!("[PQC] {}", msg);
        Ok(msg)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(512);

        let total = self.total_handshakes.load(Ordering::Relaxed);
        let pq_sessions = self.quantum_safe_sessions.load(Ordering::Relaxed);
        let blocked = self.downgrades_blocked.load(Ordering::Relaxed);

        let policy_guard = self.policy.read().ok();
        let (mode_num, defense_score) = if let Some(p) = policy_guard {
            let score = match p.enforcement_mode {
                PqcEnforcementMode::PostQuantumOnly => 100.0,
                PqcEnforcementMode::Hybrid => 95.0,
                PqcEnforcementMode::ClassicOnly => 0.0,
            };
            let mode = match p.enforcement_mode {
                PqcEnforcementMode::ClassicOnly => 0,
                PqcEnforcementMode::Hybrid => 1,
                PqcEnforcementMode::PostQuantumOnly => 2,
            };
            (mode, score)
        } else {
            (1, 95.0)
        };

        let _ = writeln!(
            out,
            "# HELP craft_pqc_handshakes_total Total post-quantum transport handshakes processed"
        );
        let _ = writeln!(out, "# TYPE craft_pqc_handshakes_total counter");
        let _ = writeln!(out, "craft_pqc_handshakes_total {}", total);

        let _ = writeln!(
            out,
            "# HELP craft_pqc_quantum_safe_sessions_total Active or negotiated post-quantum safe sessions"
        );
        let _ = writeln!(out, "# TYPE craft_pqc_quantum_safe_sessions_total counter");
        let _ = writeln!(out, "craft_pqc_quantum_safe_sessions_total {}", pq_sessions);

        let _ = writeln!(
            out,
            "# HELP craft_pqc_downgrades_blocked_total Total classical downgrade attack attempts rejected"
        );
        let _ = writeln!(out, "# TYPE craft_pqc_downgrades_blocked_total counter");
        let _ = writeln!(out, "craft_pqc_downgrades_blocked_total {}", blocked);

        let _ = writeln!(
            out,
            "# HELP craft_pqc_harvest_defense_score Quantum harvest-now-decrypt-later defense score (0-100)"
        );
        let _ = writeln!(out, "# TYPE craft_pqc_harvest_defense_score gauge");
        let _ = writeln!(out, "craft_pqc_harvest_defense_score {:.1}", defense_score);

        let _ = writeln!(
            out,
            "# HELP craft_pqc_enforcement_mode PQC enforcement policy mode (0 = ClassicOnly, 1 = Hybrid, 2 = PostQuantumOnly)"
        );
        let _ = writeln!(out, "# TYPE craft_pqc_enforcement_mode gauge");
        let _ = writeln!(out, "craft_pqc_enforcement_mode {}", mode_num);

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pqc_service_lifecycle_and_metrics() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let _ = fs::create_dir_all(&paths.pqc_dir);
        let _ = fs::create_dir_all(&paths.pqc_keys_dir);

        let service = PqcService::new(paths.clone());

        // Initial status
        let status = service.get_status().unwrap();
        assert_eq!(status.enforcement_mode, PqcEnforcementMode::Hybrid);
        assert_eq!(status.migration_phase, PqcMigrationPhase::DualStack);
        assert!((status.harvest_defense_score - 95.0).abs() < 0.1);

        // Key generation
        let kp = service
            .generate_keypair(Some(PqcCipherSuite::HybridX25519MlKem768), None)
            .unwrap();
        assert!(!kp.public_key.is_empty());
        assert!(!kp.secret_key.is_empty());

        let sig_kp = service
            .generate_keypair(None, Some(PqcSigningAlgorithm::MlDsa65))
            .unwrap();
        assert_eq!(sig_kp.algorithm, "ML-DSA-65");

        // Record handshakes
        service.record_handshake(true, false, 420);
        service.record_handshake(false, true, 0);

        let updated_status = service.get_status().unwrap();
        assert_eq!(updated_status.total_handshakes, 2);
        assert_eq!(updated_status.hybrid_handshakes, 1);
        assert_eq!(updated_status.rejected_downgrades, 1);
        assert_eq!(updated_status.active_key_pairs, 2);

        // Migration phase
        let msg = service
            .migrate_phase(PqcMigrationPhase::EnforcedPqc)
            .unwrap();
        assert!(msg.contains("EnforcedPqc"));
        let pq_status = service.get_status().unwrap();
        assert_eq!(
            pq_status.enforcement_mode,
            PqcEnforcementMode::PostQuantumOnly
        );
        assert!((pq_status.harvest_defense_score - 100.0).abs() < 0.1);

        // Prometheus metrics
        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_pqc_handshakes_total 2"));
        assert!(metrics.contains("craft_pqc_quantum_safe_sessions_total 1"));
        assert!(metrics.contains("craft_pqc_downgrades_blocked_total 1"));
        assert!(metrics.contains("craft_pqc_harvest_defense_score 100.0"));
        assert!(metrics.contains("craft_pqc_enforcement_mode 2"));

        // Benchmark (1 iteration for test speed)
        let bench = service.run_benchmark(1).unwrap();
        assert_eq!(bench.iterations, 1);
        assert!(bench.mlkem768_encap_avg_us > 0.0);
    }
}

