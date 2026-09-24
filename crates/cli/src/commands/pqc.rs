use crate::cli::PqcCommands;
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_core::pqc::{
    PqcCipherSuite, PqcEnforcementMode, PqcMigrationPhase, PqcSigningAlgorithm,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::PqcService;
use std::str::FromStr;

pub async fn handle_pqc(action: Option<PqcCommands>, paths: &CraftPaths) -> Result<()> {
    match action {
        None | Some(PqcCommands::Status { json: false }) => handle_status(paths, false).await,
        Some(PqcCommands::Status { json: true }) => handle_status(paths, true).await,
        Some(PqcCommands::Policy {
            set_mode,
            ciphersuite,
            allow_fallback,
            json,
        }) => handle_policy(paths, set_mode, ciphersuite, allow_fallback, json).await,
        Some(PqcCommands::Keygen { suite, algo, json }) => {
            handle_keygen(paths, suite, algo, json).await
        }
        Some(PqcCommands::Bench { iterations, json }) => {
            handle_bench(paths, iterations, json).await
        }
        Some(PqcCommands::Migrate { phase, json }) => handle_migrate(paths, &phase, json).await,
    }
}

async fn handle_status(paths: &CraftPaths, json: bool) -> Result<()> {
    let summary = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_pqc_status().await {
            Ok(s) => s,
            Err(_) => {
                let service = PqcService::global(paths);
                service.get_status()?
            }
        },
        Err(_) => {
            let service = PqcService::global(paths);
            service.get_status()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("[PQC] Post-Quantum Cryptographic Hardening & Key Exchange Status");
        println!("  Enforcement Mode:       {}", summary.enforcement_mode);
        println!("  Active Ciphersuite:     {}", summary.active_ciphersuite);
        println!(
            "  Harvest Defense Score:  {:.1}%",
            summary.harvest_defense_score
        );
        println!("  Active PQC Keypairs:    {}", summary.active_key_pairs);
        println!("  Total Handshakes:       {}", summary.total_handshakes);
        println!("  Hybrid PQ Handshakes:   {}", summary.hybrid_handshakes);
        println!("  Pure PQ Handshakes:     {}", summary.pure_pq_handshakes);
        println!("  Blocked Downgrades:     {}", summary.rejected_downgrades);
        println!(
            "  Avg Encap Latency:      {:.2} us",
            summary.average_encap_latency_us
        );
        println!("  Migration Phase:        {:?}", summary.migration_phase);
    }
    Ok(())
}

async fn handle_policy(
    paths: &CraftPaths,
    set_mode: Option<String>,
    ciphersuite: Option<String>,
    allow_fallback: Option<bool>,
    json: bool,
) -> Result<()> {
    let mut current_policy = {
        let service = PqcService::global(paths);
        service.get_policy()?
    };

    let mut changed = false;
    if let Some(m) = set_mode {
        current_policy.enforcement_mode = PqcEnforcementMode::from_str(&m)?;
        changed = true;
    }
    if let Some(cs) = ciphersuite {
        current_policy.preferred_ciphersuite = PqcCipherSuite::from_str(&cs)?;
        changed = true;
    }
    if let Some(af) = allow_fallback {
        current_policy.allow_classical_fallback = af;
        changed = true;
    }

    if changed {
        match DaemonClient::connect(paths).await {
            Ok(mut client) => {
                if let Err(_) = client.set_pqc_policy(current_policy.clone()).await {
                    let service = PqcService::global(paths);
                    service.set_policy(current_policy.clone())?;
                }
            }
            Err(_) => {
                let service = PqcService::global(paths);
                service.set_policy(current_policy.clone())?;
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&current_policy).unwrap());
    } else {
        println!("[PQC] Cryptographic Policy Configuration");
        println!(
            "  Enforcement Mode:       {}",
            current_policy.enforcement_mode
        );
        println!(
            "  Preferred Ciphersuite:  {}",
            current_policy.preferred_ciphersuite
        );
        println!(
            "  Min Security Category:  {}",
            current_policy.min_security_category
        );
        println!(
            "  Classical Fallback:     {}",
            current_policy.allow_classical_fallback
        );
        println!(
            "  Enforce Signatures:     {}",
            current_policy.enforce_quantum_signatures
        );
        println!(
            "  Migration Phase:        {:?}",
            current_policy.migration_phase
        );
    }
    Ok(())
}

async fn handle_keygen(
    paths: &CraftPaths,
    suite: Option<String>,
    algo: Option<String>,
    json: bool,
) -> Result<()> {
    let parsed_suite = match suite.as_deref() {
        Some("hybrid" | "hybrid_x25519_mlkem768" | "x25519-mlkem768") => {
            Some(PqcCipherSuite::HybridX25519MlKem768)
        }
        Some("mlkem768" | "pure_mlkem768" | "kyber768") => Some(PqcCipherSuite::PureMlKem768),
        Some("mlkem1024" | "pure_mlkem1024" | "kyber1024") => Some(PqcCipherSuite::PureMlKem1024),
        Some(other) => Some(PqcCipherSuite::from_str(other)?),
        None => None,
    };

    let parsed_algo = match algo.as_deref() {
        Some("mldsa65" | "dilithium3" | "mldsa") => Some(PqcSigningAlgorithm::MlDsa65),
        Some("hybrid_mldsa65" | "hybrid_dilithium") => {
            Some(PqcSigningAlgorithm::HybridEd25519MlDsa65)
        }
        Some("ed25519") => Some(PqcSigningAlgorithm::Ed25519),
        Some(other) => {
            return Err(CraftError::Config(format!(
                "Unknown signing algorithm: '{}'",
                other
            )))
        }
        None => None,
    };

    let keypair = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .generate_pqc_keypair(parsed_suite, parsed_algo)
                .await
            {
                Ok(kp) => kp,
                Err(_) => {
                    let service = PqcService::global(paths);
                    service.generate_keypair(parsed_suite, parsed_algo)?
                }
            }
        }
        Err(_) => {
            let service = PqcService::global(paths);
            service.generate_keypair(parsed_suite, parsed_algo)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&keypair).unwrap());
    } else {
        println!(
            "[OK] Generated NIST FIPS 203/204 Post-Quantum Keypair '{}'",
            keypair.id
        );
        println!("  Algorithm:         {}", keypair.algorithm);
        let preview_pk = if keypair.public_key.len() > 64 {
            format!("{}...", &keypair.public_key[..64])
        } else {
            keypair.public_key.clone()
        };
        println!("  Public Key (hex):  {}", preview_pk);
        println!(
            "  Public Key File:   {}",
            paths.pqc_keys_dir.join(format!("{}.pub", keypair.id)).display()
        );
        println!("  Created Epoch:     {}", keypair.created_at_epoch);
    }
    Ok(())
}

async fn handle_bench(paths: &CraftPaths, iterations: usize, json: bool) -> Result<()> {
    if !json {
        println!(
            "[PQC] Running pure-Rust Post-Quantum benchmarks (iterations: {})...",
            iterations
        );
    }

    let report = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.benchmark_pqc(iterations).await {
            Ok(r) => r,
            Err(_) => {
                let service = PqcService::global(paths);
                service.run_benchmark(iterations)?
            }
        },
        Err(_) => {
            let service = PqcService::global(paths);
            service.run_benchmark(iterations)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("[PQC] Micro-Benchmark Execution Results");
        println!("  Iterations:                 {}", report.iterations);
        println!(
            "  ML-KEM-768 Encap Avg:       {:.2} us",
            report.mlkem768_encap_avg_us
        );
        println!(
            "  ML-KEM-768 Decap Avg:       {:.2} us",
            report.mlkem768_decap_avg_us
        );
        println!(
            "  ML-KEM-1024 Encap Avg:      {:.2} us",
            report.mlkem1024_encap_avg_us
        );
        println!(
            "  ML-KEM-1024 Decap Avg:      {:.2} us",
            report.mlkem1024_decap_avg_us
        );
        println!(
            "  Hybrid X25519/KEM Encap:    {:.2} us",
            report.hybrid_encap_avg_us
        );
        println!(
            "  Hybrid X25519/KEM Decap:    {:.2} us",
            report.hybrid_decap_avg_us
        );
        println!(
            "  ML-DSA-65 Sign Avg:         {:.2} us",
            report.mldsa65_sign_avg_us
        );
        println!(
            "  ML-DSA-65 Verify Avg:       {:.2} us",
            report.mldsa65_verify_avg_us
        );
    }
    Ok(())
}

async fn handle_migrate(paths: &CraftPaths, phase_str: &str, json: bool) -> Result<()> {
    let target_phase = match phase_str.to_lowercase().replace('-', "_").as_str() {
        "planning" | "plan" => PqcMigrationPhase::Planning,
        "dual_stack" | "dual" | "hybrid" => PqcMigrationPhase::DualStack,
        "enforced_pqc" | "enforced" | "post_quantum" | "pure_pqc" => PqcMigrationPhase::EnforcedPqc,
        other => {
            return Err(CraftError::Config(format!(
                "Invalid migration phase '{}'. Options: planning, dual_stack, enforced_pqc",
                other
            )))
        }
    };

    let (new_phase, message) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.migrate_pqc_node(target_phase).await {
            Ok(res) => res,
            Err(_) => {
                let service = PqcService::global(paths);
                let msg = service.migrate_phase(target_phase)?;
                (target_phase, msg)
            }
        },
        Err(_) => {
            let service = PqcService::global(paths);
            let msg = service.migrate_phase(target_phase)?;
            (target_phase, msg)
        }
    };

    if json {
        let resp = serde_json::json!({
            "target_phase": target_phase,
            "new_phase": new_phase,
            "message": message,
        });
        println!("{}", serde_json::to_string_pretty(&resp).unwrap());
    } else {
        println!("[OK] {}", message);
    }
    Ok(())
}
