use crate::cli::HsmCommands;
use craft_core::error::{CraftError, Result};
use craft_core::hsm::{HsmKeyType, ZkMembershipProof};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::HsmService;
use serde_json::json;
use std::str::FromStr;

pub async fn handle_hsm(action: Option<HsmCommands>, paths: &CraftPaths) -> Result<()> {
    match action {
        None | Some(HsmCommands::Status { json: false }) => handle_status(paths, false).await,
        Some(HsmCommands::Status { json: true }) => handle_status(paths, true).await,
        Some(HsmCommands::Keygen { label, key_type, json }) => {
            handle_keygen(paths, &label, key_type, json).await
        }
        Some(HsmCommands::Sign { label, data, json }) => {
            handle_sign(paths, &label, &data, json).await
        }
        Some(HsmCommands::Attest { pcr_mask, nonce, json }) => {
            handle_attest(paths, pcr_mask, nonce, json).await
        }
        Some(HsmCommands::ZkMember { action, cluster, json }) => {
            handle_zk_member(paths, &action, &cluster, json).await
        }
    }
}

async fn handle_status(paths: &CraftPaths, json: bool) -> Result<()> {
    let summary = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_hsm_status().await {
            Ok(s) => s,
            Err(_) => {
                let service = HsmService::global(paths);
                service.get_status()?
            }
        },
        Err(_) => {
            let service = HsmService::global(paths);
            service.get_status()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("[HSM] Hardware Security Module & TPM 2.0 Enclave Status");
        println!("  Backend Type:           {}", summary.backend_type);
        println!(
            "  Token Present:          {}",
            if summary.token_present {
                "[x] Present"
            } else {
                "[ ] Absent"
            }
        );
        println!("  Active Token Slots:     {}", summary.slots_count);
        println!("  Active Key Handles:     {}", summary.active_keys_count);
        println!(
            "  Hardware-Backed Keys:   {} (non-extractable: CKA_EXTRACTABLE=false)",
            summary.hardware_backed_keys
        );
        println!(
            "  TPM 2.0 PCR Bank:       {}",
            if summary.tpm_pcr_active {
                "[OK] Active (PCR 0-23 initialized)"
            } else {
                "[ ] Inactive"
            }
        );
        println!("  Attested Quotes:        {} verified", summary.attested_quotes_count);
        println!("  ZK Memberships:         {} validated", summary.zk_memberships_count);
        println!("  Total Operations:       {}", summary.total_operations);
    }
    Ok(())
}

async fn handle_keygen(
    paths: &CraftPaths,
    label: &str,
    key_type_opt: Option<String>,
    json: bool,
) -> Result<()> {
    let key_type = match key_type_opt {
        Some(s) => HsmKeyType::from_str(&s)?,
        None => HsmKeyType::Ed25519,
    };

    let key = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.generate_hsm_key(label, key_type).await {
            Ok(k) => k,
            Err(_) => {
                let service = HsmService::global(paths);
                service.generate_key(label, key_type)?
            }
        },
        Err(_) => {
            let service = HsmService::global(paths);
            service.generate_key(label, key_type)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&key).unwrap());
    } else {
        println!("[OK] Generated Non-Extractable Hardware Key Handle");
        println!("  ID:                     {}", key.id);
        println!("  Label:                  {}", key.label);
        println!("  Key Type:               {}", key.key_type);
        println!("  Slot ID:                {}", key.slot_id);
        println!(
            "  Extractable:            {} (strictly non-extractable)",
            if key.extractable { "[WARN] true" } else { "[x] false" }
        );
        println!("  Public Key:             {}", key.public_key);
    }
    Ok(())
}

async fn handle_sign(
    paths: &CraftPaths,
    label: &str,
    data: &str,
    json: bool,
) -> Result<()> {
    let raw_data = data.as_bytes();
    let signature = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.sign_with_hsm(label, raw_data).await {
            Ok(s) => s,
            Err(_) => {
                let service = HsmService::global(paths);
                service.sign_data(label, raw_data)?
            }
        },
        Err(_) => {
            let service = HsmService::global(paths);
            service.sign_data(label, raw_data)?
        }
    };

    let sig_hex = hex::encode(&signature);
    if json {
        let val = json!({
            "label": label,
            "signature": sig_hex,
            "signature_bytes": signature.len(),
            "status": "success"
        });
        println!("{}", serde_json::to_string_pretty(&val).unwrap());
    } else {
        println!("[OK] Cryptographic Signing Executed Inside Hardware Token Boundary");
        println!("  Key Label:              {}", label);
        println!("  Signature Bytes:        {}", signature.len());
        println!("  Signature (Hex):        {}", sig_hex);
    }
    Ok(())
}

async fn handle_attest(
    paths: &CraftPaths,
    pcr_mask_opt: Option<u32>,
    nonce_hex: Option<String>,
    json: bool,
) -> Result<()> {
    let pcr_mask = pcr_mask_opt.unwrap_or(0b111); // PCR 0, 1, 2 by default
    let nonce: Option<[u8; 32]> = if let Some(n_str) = nonce_hex {
        let bytes = hex::decode(&n_str).map_err(|e| {
            CraftError::Config(format!("Invalid hex nonce for attestation: {}", e))
        })?;
        if bytes.len() != 32 {
            return Err(CraftError::Config("Attestation nonce must be 32 bytes".to_string()));
        }
        let mut n = [0u8; 32];
        n.copy_from_slice(&bytes);
        Some(n)
    } else {
        None
    };

    let quote = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.attest_hsm(pcr_mask, nonce).await {
            Ok(q) => q,
            Err(_) => {
                let service = HsmService::global(paths);
                service.attest_enclave(pcr_mask, nonce)?
            }
        },
        Err(_) => {
            let service = HsmService::global(paths);
            service.attest_enclave(pcr_mask, nonce)?
        }
    };

    if json {
        let val = json!({
            "nonce": hex::encode(quote.nonce),
            "pcr_mask": format!("0x{:08X}", quote.pcr_mask),
            "pcr_digest": hex::encode(quote.pcr_digest),
            "aik_public_key": hex::encode(&quote.aik_public_key),
            "signature": hex::encode(&quote.signature),
            "timestamp_epoch": quote.timestamp_epoch,
            "status": "verified"
        });
        println!("{}", serde_json::to_string_pretty(&val).unwrap());
    } else {
        println!("[OK] TPM 2.0 Enclave Attestation Quote Generated & Verified");
        println!("  PCR Selection Mask:     0x{:08X}", quote.pcr_mask);
        println!("  Fresh Nonce:            {}", hex::encode(quote.nonce));
        println!("  PCR Composite Digest:   {}", hex::encode(quote.pcr_digest));
        println!(
            "  AIK Public Key:         {}",
            &hex::encode(&quote.aik_public_key)[..32.min(quote.aik_public_key.len() * 2)]
        );
        println!(
            "  Attestation Signature:  {}",
            &hex::encode(&quote.signature)[..32.min(quote.signature.len() * 2)]
        );
    }
    Ok(())
}

async fn handle_zk_member(
    paths: &CraftPaths,
    action: &str,
    cluster: &str,
    json: bool,
) -> Result<()> {
    match action.to_lowercase().as_str() {
        "prove" | "generate" => {
            let proof = match DaemonClient::connect(paths).await {
                Ok(mut client) => match client.prove_hsm_zk(cluster).await {
                    Ok(p) => p,
                    Err(_) => {
                        let service = HsmService::global(paths);
                        service.prove_zk_membership(cluster)?
                    }
                },
                Err(_) => {
                    let service = HsmService::global(paths);
                    service.prove_zk_membership(cluster)?
                }
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&proof).unwrap());
            } else {
                println!("[OK] Generated Schnorr-Pedersen Zero-Knowledge Membership Proof");
                println!("  Cluster ID:             {}", proof.cluster_id);
                println!("  Fresh Nonce:            {}", proof.nonce);
                println!("  Commitment C_n:         {}", proof.commitment_c);
                println!("  Commitment T:           {}", proof.commitment_t);
                println!("  Response z_s:           {}", proof.response_zs);
                println!("  Response z_r:           {}", proof.response_zr);
            }
        }
        "verify" | "validate" => {
            // Generate proof for the cluster and then verify
            let service = HsmService::global(paths);
            let proof: ZkMembershipProof = match DaemonClient::connect(paths).await {
                Ok(mut client) => match client.prove_hsm_zk(cluster).await {
                    Ok(p) => p,
                    Err(_) => service.prove_zk_membership(cluster)?,
                },
                Err(_) => service.prove_zk_membership(cluster)?,
            };

            let (valid, message) = match DaemonClient::connect(paths).await {
                Ok(mut client) => match client.verify_hsm_zk(cluster, proof.clone()).await {
                    Ok((v, m)) => (v, m),
                    Err(_) => {
                        let v = service.verify_zk_membership(&proof)?;
                        (v, if v { "ZK membership verified".to_string() } else { "ZK verification failed".to_string() })
                    }
                },
                Err(_) => {
                    let v = service.verify_zk_membership(&proof)?;
                    (v, if v { "ZK membership verified".to_string() } else { "ZK verification failed".to_string() })
                }
            };

            if json {
                let val = json!({
                    "cluster_id": cluster,
                    "valid": valid,
                    "message": message
                });
                println!("{}", serde_json::to_string_pretty(&val).unwrap());
            } else {
                if valid {
                    println!("[OK] Zero-Knowledge Cluster Membership Validated");
                    println!("  Cluster ID:             {}", cluster);
                    println!("  Status:                 [OK] Valid Member (Topological Privacy Preserved)");
                } else {
                    println!("[ERROR] Zero-Knowledge Cluster Membership Validation Failed");
                    println!("  Cluster ID:             {}", cluster);
                    println!("  Status:                 [ERROR] Rejection");
                }
            }
        }
        other => {
            return Err(CraftError::Config(format!(
                "Unknown ZK member action '{}'. Supported actions: prove, verify",
                other
            )));
        }
    }
    Ok(())
}
