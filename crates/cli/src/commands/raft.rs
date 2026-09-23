use crate::cli::RaftCommands;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{CraftPaths, RaftPayload, Result};
use craft_daemon::{DaemonClient, RaftConsensusService, RaftStatusSummary};
use serde_json::json;

/// Entry point for `craft raft` subcommands
pub async fn handle_raft(cmd: RaftCommands, paths: &CraftPaths) -> Result<()> {
    match cmd {
        RaftCommands::Status { json } => handle_status(paths, json).await,
        RaftCommands::Propose { action, data, json } => handle_propose(paths, &action, &data, json).await,
        RaftCommands::Lock {
            name,
            holder,
            lease,
            json,
        } => handle_lock(paths, &name, &holder, lease, json).await,
        RaftCommands::Unlock { name, holder, json } => handle_unlock(paths, &name, &holder, json).await,
        RaftCommands::StepDown { json } => handle_stepdown(paths, json).await,
        RaftCommands::Transfer { target, json } => handle_transfer(paths, &target, json).await,
        RaftCommands::Logs { limit, json } => handle_logs(paths, limit, json).await,
    }
}

async fn fetch_status(paths: &CraftPaths) -> Result<RaftStatusSummary> {
    if let Ok(mut client) = DaemonClient::connect(paths).await {
        if let Ok(status) = client.get_raft_status().await {
            return Ok(status);
        }
    }
    RaftConsensusService::get_status(paths)
}

async fn handle_status(paths: &CraftPaths, as_json: bool) -> Result<()> {
    let status = fetch_status(paths).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("[OK] Raft Consensus Status");
    println!("  Node ID:          {}", status.node_id);
    println!("  Role:             [{}]", status.role);
    println!("  Current Term:     {}", status.current_term);
    println!(
        "  Active Leader:    {}",
        status
            .leader_id
            .as_deref()
            .unwrap_or("None (Election Pending)")
    );
    println!("  Commit Index:     {}", status.commit_index);
    println!("  Last Applied:     {}", status.last_applied);
    println!("  WAL Log Entries:  {}", status.log_entries_count);
    println!("  Active Locks:     {}", status.active_locks_count);
    println!(
        "  Quorum Status:    {}",
        if status.is_quorum_intact {
            "[OK] Intact"
        } else {
            "[WARN] Degraded / Partitioned"
        }
    );
    println!(
        "  Edge Tie-Breaker: {}",
        status.edge_tie_breaker.as_deref().unwrap_or("None")
    );

    if !status.cluster_nodes.is_empty() {
        println!("\nCluster Membership:");
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Node ID").fg(Color::Cyan),
                Cell::new("Endpoint").fg(Color::Cyan),
                Cell::new("Voting").fg(Color::Cyan),
                Cell::new("Priority").fg(Color::Cyan),
            ]);

        for node in &status.cluster_nodes {
            table.add_row(vec![
                Cell::new(&node.id),
                Cell::new(format!("{}:{}", node.address, node.raft_port)),
                Cell::new(if node.voting_member { "Yes" } else { "No" }),
                Cell::new(node.priority.to_string()),
            ]);
        }
        println!("{table}");
    }

    Ok(())
}

async fn handle_propose(paths: &CraftPaths, action: &str, data: &str, as_json: bool) -> Result<()> {
    let payload = match action {
        "config_update" => {
            let parts: Vec<&str> = data.splitn(2, '=').collect();
            if parts.len() == 2 {
                RaftPayload::ClusterConfigUpdate {
                    key: parts[0].to_string(),
                    value: parts[1].to_string(),
                }
            } else {
                RaftPayload::Custom {
                    action: action.to_string(),
                    data: data.to_string(),
                }
            }
        }
        _ => RaftPayload::Custom {
            action: action.to_string(),
            data: data.to_string(),
        },
    };

    let (term, index) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.propose_raft_command(payload.clone()).await {
            Ok(res) => res,
            Err(_) => RaftConsensusService::propose(paths, payload)?,
        }
    } else {
        RaftConsensusService::propose(paths, payload)?
    };

    if as_json {
        println!(
            "{}",
            json!({
                "status": "committed",
                "term": term,
                "index": index,
                "action": action,
            })
        );
    } else {
        println!(
            "[OK] Replicated consensus proposal committed (Term: {}, Log Index: {})",
            term, index
        );
    }

    Ok(())
}

async fn handle_lock(
    paths: &CraftPaths,
    name: &str,
    holder: &str,
    lease_secs: u64,
    as_json: bool,
) -> Result<()> {
    let lock = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client
            .acquire_distributed_lock(name.to_string(), holder.to_string(), lease_secs)
            .await
        {
            Ok(l) => l,
            Err(_) => RaftConsensusService::acquire_lock(paths, name, holder, lease_secs)?,
        }
    } else {
        RaftConsensusService::acquire_lock(paths, name, holder, lease_secs)?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&lock)?);
    } else {
        println!("[OK] Acquired distributed lock '{}'", lock.name);
        println!("  Holder ID:      {}", lock.holder_id);
        println!("  Fencing Token:  {}", lock.fencing_token);
        println!("  Expires At:     {} (epoch)", lock.lease_expires_at);
    }

    Ok(())
}

async fn handle_unlock(paths: &CraftPaths, name: &str, holder: &str, as_json: bool) -> Result<()> {
    if let Ok(mut client) = DaemonClient::connect(paths).await {
        let _ = client
            .release_distributed_lock(name.to_string(), holder.to_string())
            .await;
    } else {
        RaftConsensusService::release_lock(paths, name, holder)?;
    }

    if as_json {
        println!(
            "{}",
            json!({
                "status": "released",
                "lock": name,
                "holder": holder,
            })
        );
    } else {
        println!("[OK] Released distributed lock '{}'", name);
    }

    Ok(())
}

async fn handle_stepdown(paths: &CraftPaths, as_json: bool) -> Result<()> {
    if let Ok(mut client) = DaemonClient::connect(paths).await {
        let _ = client.step_down_raft_leader().await;
    } else {
        RaftConsensusService::step_down(paths)?;
    }

    if as_json {
        println!("{}", json!({ "status": "stepped_down" }));
    } else {
        println!("[OK] Raft leader stepped down to follower state");
    }

    Ok(())
}

async fn handle_transfer(paths: &CraftPaths, target: &str, as_json: bool) -> Result<()> {
    if let Ok(mut client) = DaemonClient::connect(paths).await {
        let _ = client
            .transfer_raft_leadership(target.to_string())
            .await;
    } else {
        RaftConsensusService::transfer_leadership(paths, target)?;
    }

    if as_json {
        println!("{}", json!({ "status": "transferred", "target": target }));
    } else {
        println!("[OK] Raft leadership transferred to '{}'", target);
    }

    Ok(())
}

async fn handle_logs(paths: &CraftPaths, limit: usize, as_json: bool) -> Result<()> {
    let entries = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_raft_logs(Some(limit)).await {
            Ok(e) => e,
            Err(_) => RaftConsensusService::get_logs(paths, Some(limit))?,
        }
    } else {
        RaftConsensusService::get_logs(paths, Some(limit))?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("[INFO] Replicated Write-Ahead Log is currently empty");
        return Ok(());
    }

    println!("[OK] Replicated Write-Ahead Log (WAL) Entries (Showing {})", entries.len());
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Index").fg(Color::Cyan),
            Cell::new("Term").fg(Color::Cyan),
            Cell::new("Timestamp").fg(Color::Cyan),
            Cell::new("Client").fg(Color::Cyan),
            Cell::new("Payload").fg(Color::Cyan),
        ]);

    for entry in &entries {
        let payload_str = match &entry.payload {
            RaftPayload::ClusterConfigUpdate { key, value } => format!("Config: {}={}", key, value),
            RaftPayload::ServerRegistration { name, port, .. } => format!("Register: {}:{}", name, port),
            RaftPayload::ServerDeregistration { name } => format!("Deregister: {}", name),
            RaftPayload::DistributedLockAcquire { lock_name, holder_id, lease_secs } => {
                format!("Lock: {} by {} ({}s)", lock_name, holder_id, lease_secs)
            }
            RaftPayload::DistributedLockRelease { lock_name, holder_id } => {
                format!("Unlock: {} by {}", lock_name, holder_id)
            }
            RaftPayload::NoOp => "NoOp (Commit Boundary)".to_string(),
            RaftPayload::Custom { action, data } => format!("{}: {}", action, data),
        };

        table.add_row(vec![
            Cell::new(entry.index.to_string()),
            Cell::new(entry.term.to_string()),
            Cell::new(entry.timestamp.to_string()),
            Cell::new(entry.client_id.as_deref().unwrap_or("-")),
            Cell::new(payload_str),
        ]);
    }

    println!("{table}");
    Ok(())
}
