use crate::cli::{RaftCommands, RaftPartitionCommands};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{
    format_size, CraftPaths, JointConsensusPhase, MembershipChangeType, MultiRaftPartition,
    RaftNode, RaftPayload, Result,
};
use craft_daemon::{DaemonClient, MultiRaftService, RaftConsensusService, RaftStatusSummary};
use serde_json::json;

/// Entry point for `craft raft` subcommands
pub async fn handle_raft(cmd: RaftCommands, paths: &CraftPaths) -> Result<()> {
    match cmd {
        RaftCommands::Status { group, json } => handle_status(paths, group, json).await,
        RaftCommands::Propose {
            group,
            action,
            data,
            json,
        } => handle_propose(paths, &group, &action, &data, json).await,
        RaftCommands::Reconfigure {
            group,
            add,
            remove,
            learner,
            promote,
            demote,
            json,
        } => {
            handle_reconfigure(paths, &group, add, remove, learner, promote, demote, json).await
        }
        RaftCommands::Compact {
            group,
            index,
            force,
            json,
        } => handle_compact(paths, &group, index, force, json).await,
        RaftCommands::Partition { action } => handle_partition(paths, action).await,
        RaftCommands::Lock {
            name,
            holder,
            lease,
            json,
        } => handle_lock(paths, &name, &holder, lease, json).await,
        RaftCommands::Unlock { name, holder, json } => {
            handle_unlock(paths, &name, &holder, json).await
        }
        RaftCommands::StepDown { json } => handle_stepdown(paths, json).await,
        RaftCommands::Transfer { target, json } => handle_transfer(paths, &target, json).await,
        RaftCommands::Logs { limit, json } => handle_logs(paths, limit, json).await,
    }
}

fn parse_group_id(s: &str) -> u64 {
    match s.to_ascii_lowercase().as_str() {
        "default" | "control" | "root" => 0,
        other => other.parse::<u64>().unwrap_or(0),
    }
}

fn parse_node_spec(spec: &str, voting_member: bool) -> RaftNode {
    if let Some((id, endpoint)) = spec.split_once('@') {
        if let Some((addr, port_str)) = endpoint.rsplit_once(':') {
            let port = port_str.parse::<u16>().unwrap_or(25560);
            return RaftNode {
                id: id.to_string(),
                address: addr.to_string(),
                raft_port: port,
                voting_member,
                priority: if voting_member { 100 } else { 10 },
            };
        }
        return RaftNode {
            id: id.to_string(),
            address: endpoint.to_string(),
            raft_port: 25560,
            voting_member,
            priority: if voting_member { 100 } else { 10 },
        };
    }
    RaftNode {
        id: spec.to_string(),
        address: "127.0.0.1".to_string(),
        raft_port: 25560,
        voting_member,
        priority: if voting_member { 100 } else { 10 },
    }
}

async fn handle_status(
    paths: &CraftPaths,
    group_filter: Option<String>,
    as_json: bool,
) -> Result<()> {
    let group_id_opt = group_filter.as_deref().map(parse_group_id);
    let (reg, statuses, learners) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_multiraft_status(group_id_opt).await {
            Ok(res) => res,
            Err(_) => {
                let svc = MultiRaftService::new(paths.clone());
                svc.get_status(group_id_opt)?
            }
        }
    } else {
        let svc = MultiRaftService::new(paths.clone());
        svc.get_status(group_id_opt)?
    };

    if let Some(target_group) = group_id_opt {
        if let Some(status) = statuses.get(&target_group) {
            if as_json {
                println!("{}", serde_json::to_string_pretty(&status)?);
                return Ok(());
            }
            display_group_status(target_group, status);
            return Ok(());
        }
    }

    if as_json {
        println!(
            "{}",
            json!({
                "registry": reg,
                "statuses": statuses,
                "learner_progress": learners,
            })
        );
        return Ok(());
    }

    println!(
        "[OK] Multi-Raft Consensus Fleet Overview (Partitions: {})",
        reg.partitions.len()
    );
    let mut part_table = Table::new();
    part_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Group ID").fg(Color::Cyan),
            Cell::new("Partition Name").fg(Color::Cyan),
            Cell::new("Key Range").fg(Color::Cyan),
            Cell::new("Active Leader").fg(Color::Cyan),
            Cell::new("Term").fg(Color::Cyan),
            Cell::new("Commit").fg(Color::Cyan),
            Cell::new("Quorum").fg(Color::Cyan),
        ]);

    for part in &reg.partitions {
        let leader = statuses
            .get(&part.group_id)
            .and_then(|s| s.leader_id.as_deref())
            .unwrap_or(part.leader_node_id.as_deref().unwrap_or("None"));
        let term = statuses
            .get(&part.group_id)
            .map(|s| s.current_term)
            .unwrap_or(part.term);
        let commit = statuses
            .get(&part.group_id)
            .map(|s| s.commit_index)
            .unwrap_or(part.commit_index);
        let quorum = statuses
            .get(&part.group_id)
            .map(|s| {
                if s.is_quorum_intact {
                    "[OK] Intact"
                } else {
                    "[WARN] Degraded"
                }
            })
            .unwrap_or("[WARN] Unknown");

        part_table.add_row(vec![
            Cell::new(part.group_id.to_string()),
            Cell::new(&part.name),
            Cell::new(format!(
                "[{}..{}]",
                part.key_range_start, part.key_range_end
            )),
            Cell::new(leader),
            Cell::new(term.to_string()),
            Cell::new(commit.to_string()),
            Cell::new(quorum),
        ]);
    }
    println!("{part_table}");

    if !learners.is_empty() {
        println!("\nNon-Voting Learners & Log Catch-Up Sync:");
        let mut learn_table = Table::new();
        learn_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Node ID").fg(Color::Cyan),
                Cell::new("Match Index").fg(Color::Cyan),
                Cell::new("Leader Last Index").fg(Color::Cyan),
                Cell::new("Caught Up").fg(Color::Cyan),
                Cell::new("Sync %").fg(Color::Cyan),
            ]);

        for (node_id, prog) in &learners {
            learn_table.add_row(vec![
                Cell::new(node_id),
                Cell::new(prog.match_index.to_string()),
                Cell::new(prog.leader_last_index.to_string()),
                Cell::new(if prog.is_caught_up { "Yes" } else { "No" }),
                Cell::new(format!("{:.1}%", prog.sync_percentage)),
            ]);
        }
        println!("{learn_table}");
    }

    let primary_group = if statuses.contains_key(&0) {
        0
    } else {
        statuses.keys().next().copied().unwrap_or(0)
    };
    if let Some(status) = statuses.get(&primary_group) {
        println!();
        display_group_status(primary_group, status);
    }

    Ok(())
}

fn display_group_status(group_id: u64, status: &RaftStatusSummary) {
    println!("[OK] Raft Consensus Status (Group {})", group_id);
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
        println!("\nCluster Membership (Group {}):", group_id);
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
}

async fn reconfigure_one_node(
    paths: &CraftPaths,
    group_id: u64,
    change_type: MembershipChangeType,
    node: RaftNode,
) -> Result<(bool, JointConsensusPhase, String)> {
    if let Ok(mut client) = DaemonClient::connect(paths).await {
        if let Ok(res) = client
            .reconfigure_raft_membership(group_id, change_type, node.clone())
            .await
        {
            return Ok(res);
        }
    }
    let svc = MultiRaftService::new(paths.clone());
    svc.reconfigure_membership(group_id, change_type, node)
}

async fn handle_reconfigure(
    paths: &CraftPaths,
    group_str: &str,
    add: Vec<String>,
    remove: Vec<String>,
    learner: Vec<String>,
    promote: Option<String>,
    demote: Option<String>,
    as_json: bool,
) -> Result<()> {
    let group_id = parse_group_id(group_str);
    let mut results = Vec::new();

    for a in add {
        let node = parse_node_spec(&a, true);
        let res = reconfigure_one_node(paths, group_id, MembershipChangeType::AddNode, node).await?;
        results.push(res);
    }

    for r in remove {
        let node = parse_node_spec(&r, false);
        let res = reconfigure_one_node(paths, group_id, MembershipChangeType::RemoveNode, node).await?;
        results.push(res);
    }

    for l in learner {
        let node = parse_node_spec(&l, false);
        let res = reconfigure_one_node(paths, group_id, MembershipChangeType::AddNode, node).await?;
        results.push(res);
    }

    if let Some(p) = promote {
        let node = parse_node_spec(&p, true);
        let res = reconfigure_one_node(paths, group_id, MembershipChangeType::PromoteLearner, node).await?;
        results.push(res);
    }

    if let Some(d) = demote {
        let node = parse_node_spec(&d, false);
        let res = reconfigure_one_node(paths, group_id, MembershipChangeType::DemoteToLearner, node).await?;
        results.push(res);
    }

    if as_json {
        let out: Vec<_> = results
            .into_iter()
            .map(|(success, phase, msg)| {
                json!({
                    "success": success,
                    "phase": format!("{:?}", phase),
                    "message": msg,
                })
            })
            .collect();
        println!(
            "{}",
            json!({ "group_id": group_id, "reconfigurations": out })
        );
        return Ok(());
    }

    println!(
        "[OK] Raft Joint Consensus Membership Reconfiguration Applied (Group: {})",
        group_id
    );
    for (success, phase, msg) in results {
        println!(
            "  - [{}] Phase: {:?} | {}",
            if success { "OK" } else { "FAIL" },
            phase,
            msg
        );
    }

    Ok(())
}

async fn handle_compact(
    paths: &CraftPaths,
    group_str: &str,
    _index: Option<u64>,
    force: bool,
    as_json: bool,
) -> Result<()> {
    let group_id = parse_group_id(group_str);
    let (snap_idx, snap_term, pruned, bytes) =
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            match client.trigger_raft_compaction(group_id, force).await {
                Ok(res) => res,
                Err(_) => {
                    let svc = MultiRaftService::new(paths.clone());
                    svc.trigger_compaction(group_id, force)?
                }
            }
        } else {
            let svc = MultiRaftService::new(paths.clone());
            svc.trigger_compaction(group_id, force)?
        };

    if as_json {
        println!(
            "{}",
            json!({
                "status": "compacted",
                "group_id": group_id,
                "snapshot_index": snap_idx,
                "snapshot_term": snap_term,
                "pruned_entries": pruned,
                "snapshot_bytes": bytes,
                "snapshot_size_formatted": format_size(bytes),
            })
        );
        return Ok(());
    }

    println!("[OK] Raft WAL Compaction & Snapshot Generation Completed");
    println!("  Group ID:         {}", group_id);
    println!("  Snapshot Index:   {}", snap_idx);
    println!("  Snapshot Term:    {}", snap_term);
    println!("  Pruned Entries:   {}", pruned);
    println!("  Snapshot Size:    {}", format_size(bytes));

    Ok(())
}

async fn handle_partition(paths: &CraftPaths, action: RaftPartitionCommands) -> Result<()> {
    match action {
        RaftPartitionCommands::List { json: as_json } => {
            let (reg, statuses, _) = if let Ok(mut client) = DaemonClient::connect(paths).await {
                match client.get_multiraft_status(None).await {
                    Ok(res) => res,
                    Err(_) => {
                        let svc = MultiRaftService::new(paths.clone());
                        svc.get_status(None)?
                    }
                }
            } else {
                let svc = MultiRaftService::new(paths.clone());
                svc.get_status(None)?
            };

            if as_json {
                println!("{}", serde_json::to_string_pretty(&reg)?);
                return Ok(());
            }

            println!(
                "[OK] Multi-Raft Partitions (Total: {})",
                reg.partitions.len()
            );
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Group ID").fg(Color::Cyan),
                    Cell::new("Name").fg(Color::Cyan),
                    Cell::new("Key Range").fg(Color::Cyan),
                    Cell::new("Leader").fg(Color::Cyan),
                    Cell::new("Term").fg(Color::Cyan),
                    Cell::new("Commit").fg(Color::Cyan),
                    Cell::new("Replicas").fg(Color::Cyan),
                    Cell::new("Active").fg(Color::Cyan),
                ]);

            for p in &reg.partitions {
                let leader = statuses
                    .get(&p.group_id)
                    .and_then(|s| s.leader_id.as_deref())
                    .or(p.leader_node_id.as_deref())
                    .unwrap_or("None");
                let term = statuses
                    .get(&p.group_id)
                    .map(|s| s.current_term)
                    .unwrap_or(p.term);
                let commit = statuses
                    .get(&p.group_id)
                    .map(|s| s.commit_index)
                    .unwrap_or(p.commit_index);

                table.add_row(vec![
                    Cell::new(p.group_id.to_string()),
                    Cell::new(&p.name),
                    Cell::new(format!("[{}..{}]", p.key_range_start, p.key_range_end)),
                    Cell::new(leader),
                    Cell::new(term.to_string()),
                    Cell::new(commit.to_string()),
                    Cell::new(p.replica_nodes.len().to_string()),
                    Cell::new(if p.active { "Yes" } else { "No" }),
                ]);
            }
            println!("{table}");
        }
        RaftPartitionCommands::Route { key, json: as_json } => {
            let (matched_key, group_id, partition_name, leader_node_id) =
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    match client.route_raft_partition_key(key.clone()).await {
                        Ok(res) => res,
                        Err(_) => {
                            let svc = MultiRaftService::new(paths.clone());
                            svc.route_partition_key(&key)?
                        }
                    }
                } else {
                    let svc = MultiRaftService::new(paths.clone());
                    svc.route_partition_key(&key)?
                };

            if as_json {
                println!(
                    "{}",
                    json!({
                        "key": matched_key,
                        "group_id": group_id,
                        "partition_name": partition_name,
                        "leader": leader_node_id,
                    })
                );
                return Ok(());
            }

            println!("[OK] Key Successfully Routed to Raft Group");
            println!("  Input Key:        {}", matched_key);
            println!("  Target Group ID:  {}", group_id);
            println!("  Partition Name:   {}", partition_name);
            println!(
                "  Leader Node:      {}",
                leader_node_id.as_deref().unwrap_or("None")
            );
        }
        RaftPartitionCommands::Create {
            group,
            name,
            range_start,
            range_end,
            leader,
            peers,
            json: as_json,
        } => {
            let partition = MultiRaftPartition {
                group_id: group,
                name: name.clone(),
                key_range_start: range_start,
                key_range_end: range_end,
                leader_node_id: leader,
                replica_nodes: peers,
                term: 1,
                commit_index: 0,
                active: true,
            };

            if let Ok(mut client) = DaemonClient::connect(paths).await {
                let _ = client
                    .manage_raft_partition("create".into(), Some(partition.clone()), None)
                    .await;
            } else {
                let svc = MultiRaftService::new(paths.clone());
                let _ = svc.manage_partition("create", Some(partition.clone()), None)?;
            }

            if as_json {
                println!("{}", json!({ "status": "created", "partition": partition }));
            } else {
                println!(
                    "[OK] Created Multi-Raft Partition '{}' (Group ID: {})",
                    name, group
                );
            }
        }
        RaftPartitionCommands::Remove {
            group,
            json: as_json,
        } => {
            if let Ok(mut client) = DaemonClient::connect(paths).await {
                let _ = client
                    .manage_raft_partition("remove".into(), None, Some(group))
                    .await;
            } else {
                let svc = MultiRaftService::new(paths.clone());
                let _ = svc.manage_partition("remove", None, Some(group))?;
            }

            if as_json {
                println!("{}", json!({ "status": "removed", "group_id": group }));
            } else {
                println!("[OK] Removed Multi-Raft Partition Group ID {}", group);
            }
        }
    }
    Ok(())
}

async fn handle_propose(
    paths: &CraftPaths,
    _group: &str,
    action: &str,
    data: &str,
    as_json: bool,
) -> Result<()> {
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

async fn handle_unlock(
    paths: &CraftPaths,
    name: &str,
    holder: &str,
    as_json: bool,
) -> Result<()> {
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

    println!(
        "[OK] Replicated Write-Ahead Log (WAL) Entries (Showing {})",
        entries.len()
    );
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
            RaftPayload::ClusterConfigUpdate { key, value } => {
                format!("Config: {}={}", key, value)
            }
            RaftPayload::ServerRegistration { name, port, .. } => {
                format!("Register: {}:{}", name, port)
            }
            RaftPayload::ServerDeregistration { name } => format!("Deregister: {}", name),
            RaftPayload::DistributedLockAcquire {
                lock_name,
                holder_id,
                lease_secs,
            } => {
                format!("Lock: {} by {} ({}s)", lock_name, holder_id, lease_secs)
            }
            RaftPayload::DistributedLockRelease {
                lock_name,
                holder_id,
            } => {
                format!("Unlock: {} by {}", lock_name, holder_id)
            }
            RaftPayload::ConfigurationChange {
                change_type,
                node,
                phase,
            } => {
                format!(
                    "ConfigChange({:?}): node={} phase={:?}",
                    change_type, node.id, phase
                )
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
