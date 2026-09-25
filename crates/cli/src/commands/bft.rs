// crates/cli/src/commands/bft.rs
//
// CLI command handler for Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus,
// Zero-Knowledge State Attestation & BFT Cluster Quorum.
// Strictly zero emojis.

use crate::cli::BftCommands;
use craft_core::bft::{
    render_validators_table, BftBenchmarkMetrics, BftStatusSummary, BftValidator,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::BftConsensusService;
use serde_json::json;

pub async fn handle_bft(paths: &CraftPaths, action: Option<BftCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(BftCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(BftCommands::Validators { server, json }) => handle_validators(paths, server, json).await,
        Some(BftCommands::ValidatorAdd {
            id,
            voting_weight,
            public_key,
            role: _,
            json,
        }) => handle_validator_add(paths, id, voting_weight, public_key, json).await,
        Some(BftCommands::ValidatorRm { id, reason, json }) => {
            handle_validator_rm(paths, id, reason, json).await
        }
        Some(BftCommands::TxSubmit {
            tx_type,
            payload,
            sender,
            json,
        }) => handle_tx_submit(paths, tx_type, payload, sender, json).await,
        Some(BftCommands::ViewChange { reason, json }) => {
            handle_view_change(paths, reason, json).await
        }
        Some(BftCommands::ZkVerify { proof_path, json }) => {
            handle_zk_verify(paths, proof_path, json).await
        }
        Some(BftCommands::Bench {
            transactions,
            validators,
            json,
        }) => handle_bench(paths, transactions, validators, json).await,
        Some(BftCommands::ResetMetrics { json }) => handle_reset_metrics(paths, json).await,
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<(BftStatusSummary, Vec<BftValidator>)> {
    let summary = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_bft_status(server.clone()).await {
            Ok(res) => res,
            Err(_) => {
                let service = BftConsensusService::global(paths);
                service.get_status(server.as_deref())?
            }
        },
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.get_status(server.as_deref())?
        }
    };

    let validators = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_bft_validators(server.clone()).await {
            Ok(res) => res,
            Err(_) => {
                let service = BftConsensusService::global(paths);
                service.list_validators(server.as_deref())?
            }
        },
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.list_validators(server.as_deref())?
        }
    };

    Ok((summary, validators))
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (summary, validators) = fetch_status(paths, server).await?;

    if json {
        let output = json!({
            "status": summary,
            "summary": summary,
            "validators": validators,
            "current_view": summary.current_view,
            "block_height": summary.block_height,
            "current_proposer": summary.current_proposer,
            "active_validators": summary.active_validators,
            "total_validators": summary.total_validators,
            "quorum_size": summary.quorum_size,
            "byzantine_threshold_f": summary.byzantine_threshold_f,
            "committed_blocks": summary.committed_blocks,
            "slashed_validators": summary.slashed_validators,
            "avg_commit_latency_ms": summary.avg_commit_latency_ms,
            "last_committed_block_hash": summary.last_committed_block_hash,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", summary.render_text_table());
        println!();
        println!("{}", render_validators_table(&validators));
    }

    Ok(())
}

async fn handle_validators(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let validators = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_bft_validators(server.clone()).await {
            Ok(res) => res,
            Err(_) => {
                let service = BftConsensusService::global(paths);
                service.list_validators(server.as_deref())?
            }
        },
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.list_validators(server.as_deref())?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&validators)?);
    } else {
        println!("{}", render_validators_table(&validators));
    }

    Ok(())
}

async fn handle_validator_add(
    paths: &CraftPaths,
    id: String,
    voting_weight: u64,
    public_key: Option<String>,
    json: bool,
) -> Result<()> {
    let pk = public_key.unwrap_or_else(|| {
        let kp = craft_core::bft::BlsKeypair::generate(&id, None).unwrap();
        kp.public_key.key_hex
    });
    let addr = "127.0.0.1:9090".to_string();

    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .add_bft_validator(id.clone(), addr.clone(), pk.clone(), voting_weight)
                .await?
        }
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.add_validator(&id, &addr, &pk, voting_weight)?;
            format!("Validator '{}' added successfully", id)
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "success": true,
            "node_id": id,
            "address": addr,
            "public_key": pk,
            "stake": voting_weight,
            "message": msg,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("[OK] Registered BFT validator '{}' (stake: {})", id, voting_weight);
    }

    Ok(())
}

async fn handle_validator_rm(
    paths: &CraftPaths,
    id: String,
    reason: Option<String>,
    json: bool,
) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.remove_bft_validator(id.clone()).await?,
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.remove_validator(&id)?;
            format!("Validator '{}' removed successfully", id)
        }
    };

    if json {
        let output = json!({
            "status": "removed",
            "success": true,
            "node_id": id,
            "reason": reason,
            "message": msg,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let r_str = reason
            .map(|r| format!(" (reason: {})", r))
            .unwrap_or_default();
        println!("[OK] Removed BFT validator '{}'{}", id, r_str);
    }

    Ok(())
}

async fn handle_tx_submit(
    paths: &CraftPaths,
    tx_type: String,
    payload: String,
    sender: Option<String>,
    json: bool,
) -> Result<()> {
    let (tx_id, view, status) = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .submit_bft_transaction(tx_type.clone(), payload.clone(), sender.clone())
                .await?
        }
        Err(_) => {
            let service = BftConsensusService::global(paths);
            let (id, view) = service.submit_transaction(&tx_type, &payload, sender.as_deref())?;
            (id, view, "Accepted".to_string())
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "success": true,
            "tx_id": tx_id,
            "view": view,
            "tx_status": status,
            "tx_type": tx_type,
            "sender": sender,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "[OK] Submitted transaction {} (type: {}, payload: {} bytes, status: {})",
            tx_id,
            tx_type,
            payload.len(),
            status
        );
    }

    Ok(())
}

async fn handle_view_change(
    paths: &CraftPaths,
    reason: Option<String>,
    json: bool,
) -> Result<()> {
    let r_str = reason.unwrap_or_else(|| "manual-timeout".to_string());
    let (from_view, to_view, new_proposer) = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.trigger_bft_view_change(r_str.clone()).await?,
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.trigger_view_change(&r_str)?
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "success": true,
            "from_view": from_view,
            "to_view": to_view,
            "new_proposer": new_proposer,
            "reason": r_str,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "[OK] Triggered BFT view change from view {} to {} (new proposer: '{}')",
            from_view, to_view, new_proposer
        );
    }

    Ok(())
}

async fn handle_zk_verify(
    paths: &CraftPaths,
    proof_path: String,
    json: bool,
) -> Result<()> {
    let proof_json = std::fs::read_to_string(&proof_path)
        .map_err(CraftError::Io)?;

    let (valid, message) = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.verify_bft_zk_proof(proof_json.clone()).await?,
        Err(_) => {
            let service = BftConsensusService::global(paths);
            let valid = service.verify_zk_proof(&proof_json)?;
            (valid, if valid { "ZK state proof verified successfully".to_string() } else { "ZK verification failed".to_string() })
        }
    };

    if json {
        let output = json!({
            "status": if valid { "valid" } else { "invalid" },
            "valid": valid,
            "proof_path": proof_path,
            "message": message,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if valid {
        println!("[OK] Zero-knowledge state proof verified successfully");
    } else {
        println!("[ERROR] Zero-knowledge state proof verification failed: {}", message);
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    transactions: usize,
    validators: usize,
    json: bool,
) -> Result<()> {
    let iters = transactions as u32;
    let vals = validators as u32;

    let metrics: BftBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.run_bft_bench(iters, vals).await?,
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.run_bench(iters, vals)?
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "metrics": metrics,
            "iterations": metrics.iterations,
            "validators_count": metrics.validators_count,
            "tps": metrics.tps,
            "avg_commit_latency_ms": metrics.avg_commit_latency_ms,
            "p99_commit_latency_ms": metrics.p99_commit_latency_ms,
            "aggregate_verify_micros": metrics.aggregate_verify_micros,
            "zk_verify_micros": metrics.zk_verify_micros,
            "equivocations_detected": metrics.equivocations_detected,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", metrics.render_text_table());
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, json: bool) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.reset_bft_metrics().await?,
        Err(_) => {
            let service = BftConsensusService::global(paths);
            service.reset_metrics()?;
            "BFT consensus telemetry metrics reset successfully".to_string()
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "message": msg,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
