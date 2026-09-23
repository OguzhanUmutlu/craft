use crate::cli::EdgeCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    BackboneStatus, ClustersRegistry, CraftError, CraftPaths, CrossRegionChatEnvelope, EdgeNode,
    EdgeRegistry, LatencyPlaybook, PlaybookPreset, PlayerSessionHandoff, Result, ServersRegistry,
    DEFAULT_CHAT_SECRET,
};
use craft_daemon::DaemonClient;
use craft_net::{EdgeLatencyProber, EdgeRouteGenerator};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub async fn handle_edge(action: EdgeCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        EdgeCommands::Status => handle_status(paths).await,
        EdgeCommands::Add {
            name,
            region,
            endpoint,
            weight,
            tags,
        } => handle_add(&name, &region, &endpoint, weight, tags, paths).await,
        EdgeCommands::Rm { name } => handle_rm(&name, paths).await,
        EdgeCommands::Probe { count, timeout_ms } => handle_probe(count, timeout_ms, paths).await,
        EdgeCommands::SyncRouting {
            cluster,
            dry_run,
            out_dir,
        } => handle_sync_routing(cluster, dry_run, out_dir, paths).await,
        EdgeCommands::Handoff {
            player,
            player_uuid,
            from,
            to,
            source_region,
            target_region,
            ttl,
        } => {
            handle_handoff(
                &player,
                player_uuid,
                &from,
                &to,
                source_region,
                target_region,
                ttl,
                paths,
            )
            .await
        }
        EdgeCommands::Chat {
            channel,
            message,
            sender,
            region,
        } => handle_chat(&channel, &message, sender, region, paths).await,
        EdgeCommands::Optimize {
            server,
            preset,
            dry_run,
        } => handle_optimize(&server, &preset, dry_run, paths).await,
    }
}

async fn handle_status(paths: &CraftPaths) -> Result<()> {
    println!("{}", "=== Craft Global Edge Mesh Topology ===".cyan().bold());

    let mut client = DaemonClient::connect(paths).await.ok();
    let (nodes, backbone) = if let Some(ref mut c) = client {
        c.get_edge_mesh_status().await.unwrap_or_else(|_| {
            let reg = EdgeRegistry::load(paths).unwrap_or_default();
            (reg.list_nodes().to_vec(), Vec::new())
        })
    } else {
        let reg = EdgeRegistry::load(paths).unwrap_or_default();
        (reg.list_nodes().to_vec(), Vec::new())
    };

    let reg = EdgeRegistry::load(paths).unwrap_or_default();
    println!("Routing Strategy: {}", reg.policy.strategy.to_string().yellow().bold());
    println!("Default Region  : {}", reg.policy.default_region.white());
    println!("Max Jitter      : {:.1} ms", reg.policy.max_acceptable_jitter_ms);
    println!();

    if nodes.is_empty() {
        println!("{}", "[INFO] No edge proxy nodes registered. Add nodes with 'craft edge add'.".yellow());
    } else {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(Row::from(vec![
                Cell::new("Node Name").fg(Color::Cyan),
                Cell::new("Region").fg(Color::Yellow),
                Cell::new("Endpoint").fg(Color::White),
                Cell::new("Weight").fg(Color::Blue),
                Cell::new("Enabled").fg(Color::Green),
                Cell::new("Tags").fg(Color::Magenta),
            ]));

        for node in &nodes {
            let enabled_str = if node.enabled { "[OK] Active" } else { "[DISABLED]" };
            let enabled_cell = if node.enabled {
                Cell::new(enabled_str).fg(Color::Green)
            } else {
                Cell::new(enabled_str).fg(Color::DarkGrey)
            };

            let tags_str = if node.tags.is_empty() {
                "-".to_string()
            } else {
                node.tags.join(", ")
            };

            table.add_row(Row::from(vec![
                Cell::new(&node.name),
                Cell::new(&node.region),
                Cell::new(&node.endpoint),
                Cell::new(node.weight.to_string()),
                enabled_cell,
                Cell::new(tags_str),
            ]));
        }

        println!("{}", table);
    }

    if !backbone.is_empty() {
        println!();
        println!("{}", "=== Inter-Region Backbone Telemetry ===".cyan().bold());
        let mut bb_table = Table::new();
        bb_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(Row::from(vec![
                Cell::new("Link").fg(Color::Cyan),
                Cell::new("RTT (ms)").fg(Color::Yellow),
                Cell::new("Jitter (ms)").fg(Color::Magenta),
                Cell::new("Packet Loss").fg(Color::Red),
                Cell::new("Backbone Status").fg(Color::Green),
            ]));

        for bc in &backbone {
            let link = format!("{} <-> {}", bc.source_region, bc.target_region);
            let status_cell = match bc.status {
                BackboneStatus::Optimal => Cell::new("[OPTIMAL]").fg(Color::Green),
                BackboneStatus::Elevated => Cell::new("[ELEVATED]").fg(Color::Yellow),
                BackboneStatus::Degraded => Cell::new("[DEGRADED]").fg(Color::Red),
                BackboneStatus::Critical => Cell::new("[CRITICAL]").fg(Color::DarkRed),
            };

            bb_table.add_row(Row::from(vec![
                Cell::new(link),
                Cell::new(format!("{:.1}", bc.rtt_ms)),
                Cell::new(format!("{:.1}", bc.jitter_ms)),
                Cell::new(format!("{:.1}%", bc.packet_loss_pct)),
                status_cell,
            ]));
        }

        println!("{}", bb_table);
    }

    Ok(())
}

async fn handle_add(
    name: &str,
    region: &str,
    endpoint: &str,
    weight: u32,
    tags: Vec<String>,
    paths: &CraftPaths,
) -> Result<()> {
    let mut node = EdgeNode::new(name, region, endpoint);
    node.weight = weight;
    node.tags = tags;

    let mut client = DaemonClient::connect(paths).await.ok();
    if let Some(ref mut c) = client {
        c.register_edge_node(node.clone()).await?;
    } else {
        EdgeRegistry::modify(paths, |reg| reg.add_node(node.clone()))?;
    }

    println!(
        "{} Registered edge node '{}' (region: {}, endpoint: {}, weight: {})",
        "[OK]".green().bold(),
        name.cyan().bold(),
        region.yellow(),
        endpoint.white(),
        weight
    );
    Ok(())
}

async fn handle_rm(name: &str, paths: &CraftPaths) -> Result<()> {
    let mut client = DaemonClient::connect(paths).await.ok();
    if let Some(ref mut c) = client {
        c.remove_edge_node(name.to_string()).await?;
    } else {
        let removed = EdgeRegistry::modify(paths, |reg| reg.remove_node(name))?;
        if !removed {
            return Err(CraftError::Other(format!("Edge node '{}' not found", name)));
        }
    }

    println!(
        "{} Unregistered edge node '{}'",
        "[OK]".green().bold(),
        name.cyan().bold()
    );
    Ok(())
}

async fn handle_probe(count: usize, timeout_ms: u64, paths: &CraftPaths) -> Result<()> {
    println!("{}", "=== Probing Global Edge Mesh Nodes ===".cyan().bold());

    let reg = EdgeRegistry::load(paths)?;
    let nodes = reg.list_nodes();
    if nodes.is_empty() {
        println!("{}", "[WARN] No edge nodes registered to probe. Use 'craft edge add'.".yellow());
        return Ok(());
    }

    println!(
        "Pinging {} edge nodes ({} samples each, timeout {} ms)...",
        nodes.len(),
        count,
        timeout_ms
    );

    let timeout_dur = Duration::from_millis(timeout_ms);
    let probes = EdgeLatencyProber::probe_all_nodes(nodes, count, timeout_dur).await;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Rank").fg(Color::Cyan),
            Cell::new("Node Name").fg(Color::Yellow),
            Cell::new("Region").fg(Color::White),
            Cell::new("Min RTT").fg(Color::Blue),
            Cell::new("Avg RTT").fg(Color::Green),
            Cell::new("Max RTT").fg(Color::Blue),
            Cell::new("Jitter (std dev)").fg(Color::Magenta),
            Cell::new("Loss %").fg(Color::Red),
            Cell::new("Status").fg(Color::Green),
        ]));

    let mut sorted_probes = probes.clone();
    sorted_probes.sort_by(|a, b| {
        if a.reachable && !b.reachable {
            std::cmp::Ordering::Less
        } else if !a.reachable && b.reachable {
            std::cmp::Ordering::Greater
        } else {
            a.avg_rtt_ms.partial_cmp(&b.avg_rtt_ms).unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    for (idx, p) in sorted_probes.iter().enumerate() {
        let status_cell = if p.reachable {
            Cell::new("[ONLINE]").fg(Color::Green)
        } else {
            Cell::new("[OFFLINE]").fg(Color::Red)
        };

        table.add_row(Row::from(vec![
            Cell::new(format!("#{}", idx + 1)),
            Cell::new(&p.node_name),
            Cell::new(&p.region),
            Cell::new(format!("{:.1} ms", p.min_rtt_ms)),
            Cell::new(format!("{:.1} ms", p.avg_rtt_ms)),
            Cell::new(format!("{:.1} ms", p.max_rtt_ms)),
            Cell::new(format!("{:.1} ms", p.jitter_ms)),
            Cell::new(format!("{:.1}%", p.packet_loss_pct)),
            status_cell,
        ]));
    }

    println!("{}", table);

    if let Some(optimal) = EdgeLatencyProber::select_optimal_edge_node(nodes, &probes, &reg.policy, None) {
        println!(
            "{} Optimal edge proxy for client traffic: '{}' (Region: {}, Endpoint: {})",
            "[RECOMMENDED]".green().bold(),
            optimal.name.cyan().bold(),
            optimal.region.yellow(),
            optimal.endpoint.white()
        );
    }

    Ok(())
}

async fn handle_sync_routing(
    cluster_name: Option<String>,
    dry_run: bool,
    out_dir: Option<String>,
    paths: &CraftPaths,
) -> Result<()> {
    println!("{}", "=== Synchronizing Global Edge Mesh Routing ===".cyan().bold());

    let reg = EdgeRegistry::load(paths)?;
    let nodes = reg.list_nodes();

    let target_cluster = cluster_name.unwrap_or_else(|| "default".to_string());
    let clusters_reg = ClustersRegistry::load(paths).unwrap_or_default();
    let servers_reg = ServersRegistry::load(paths).unwrap_or_default();

    let mut backend_servers: Vec<(String, String, Option<String>)> = Vec::new();

    if let Some(cluster) = clusters_reg.get_cluster(&target_cluster) {
        for node in &cluster.nodes {
            if let Some(srv) = servers_reg.find_by_name(&node.name) {
                let port = srv.port.unwrap_or(25565);
                let endpoint = format!("127.0.0.1:{}", port);
                backend_servers.push((node.name.clone(), endpoint, None));
            }
        }
    } else {
        // Fall back to all registered servers
        for srv in &servers_reg.servers {
            let port = srv.port.unwrap_or(25565);
            let endpoint = format!("127.0.0.1:{}", port);
            backend_servers.push((srv.name.clone(), endpoint, None));
        }
    }

    if backend_servers.is_empty() {
        return Err(CraftError::Other(
            "No backend servers available to route. Register servers first.".to_string(),
        ));
    }

    let velocity_cfg =
        EdgeRouteGenerator::generate_velocity_config(&target_cluster, nodes, &backend_servers);
    let bungeecord_cfg =
        EdgeRouteGenerator::generate_bungeecord_config(nodes, &backend_servers);

    let haproxy_backends: Vec<(String, String)> = backend_servers
        .iter()
        .map(|(name, ep, _)| (name.clone(), ep.clone()))
        .collect();
    let haproxy_cfg = EdgeRouteGenerator::generate_haproxy_config(25565, nodes, &haproxy_backends);

    if dry_run {
        println!("{}", "[DRY-RUN] Velocity velocity.toml:".yellow().bold());
        println!("{}", velocity_cfg);
        println!("{}", "[DRY-RUN] HAProxy haproxy.cfg:".yellow().bold());
        println!("{}", haproxy_cfg);
        return Ok(());
    }

    let destination_dir = if let Some(dir) = out_dir {
        PathBuf::from(dir)
    } else {
        paths.edge_dir.join("routing")
    };

    fs::create_dir_all(&destination_dir)?;
    fs::write(destination_dir.join("velocity.toml"), velocity_cfg)?;
    fs::write(destination_dir.join("bungeecord.yml"), bungeecord_cfg)?;
    fs::write(destination_dir.join("haproxy.cfg"), haproxy_cfg)?;

    println!(
        "{} Synchronized routing configs to '{}':",
        "[OK]".green().bold(),
        destination_dir.display()
    );
    println!("  - velocity.toml");
    println!("  - bungeecord.yml");
    println!("  - haproxy.cfg");

    Ok(())
}

async fn handle_handoff(
    player: &str,
    player_uuid: Option<String>,
    from: &str,
    to: &str,
    source_region: Option<String>,
    target_region: Option<String>,
    ttl: u64,
    paths: &CraftPaths,
) -> Result<()> {
    println!("{}", "=== Initiating Cross-Region Session Handoff ===".cyan().bold());

    let uuid_str = player_uuid.unwrap_or_else(|| {
        format!("player-{}", player.to_lowercase())
    });

    let s_region = source_region.unwrap_or_else(|| "us-east".to_string());
    let t_region = target_region.unwrap_or_else(|| "eu-west".to_string());

    let handoff = PlayerSessionHandoff::new(
        uuid_str,
        player,
        from,
        to,
        s_region,
        t_region,
        ttl,
        None,
    );

    let mut client = DaemonClient::connect(paths).await.ok();
    if let Some(ref mut c) = client {
        let msg = c.trigger_edge_handoff(handoff.clone()).await?;
        println!("{}", msg.green());
    } else {
        println!(
            "{} Daemon offline; generated handoff token: '{}'",
            "[WARN]".yellow(),
            handoff.transfer_token.cyan().bold()
        );
    }

    println!("Session ID    : {}", handoff.session_id);
    println!("Player        : {}", handoff.player_name.white().bold());
    println!("Route         : {} ({}) -> {} ({})", from.cyan(), handoff.source_region, to.green(), handoff.target_region);
    println!("Token         : {}", handoff.transfer_token.yellow().bold());
    println!("Expires In    : {} seconds", ttl);

    Ok(())
}

async fn handle_chat(
    channel: &str,
    message: &str,
    sender: Option<String>,
    region: Option<String>,
    paths: &CraftPaths,
) -> Result<()> {
    let s_name = sender.unwrap_or_else(|| "Operator".to_string());
    let r_name = region.unwrap_or_else(|| "local".to_string());

    let env = CrossRegionChatEnvelope::new(
        channel,
        s_name,
        None,
        message,
        r_name,
        DEFAULT_CHAT_SECRET,
    );

    let mut client = DaemonClient::connect(paths).await.ok();
    if let Some(ref mut c) = client {
        let delivered = c.broadcast_edge_chat(env.clone()).await?;
        println!(
            "{} Broadcast chat to channel '{}' across {} nodes",
            "[OK]".green().bold(),
            channel.cyan(),
            delivered
        );
    } else {
        println!(
            "{} Chat envelope signed and ready (daemon offline): id={}",
            "[WARN]".yellow(),
            env.id
        );
    }

    Ok(())
}

async fn handle_optimize(
    server: &str,
    preset: &str,
    dry_run: bool,
    paths: &CraftPaths,
) -> Result<()> {
    println!("{}", "=== Applying Regional Latency Optimization Playbook ===".cyan().bold());

    let p: PlaybookPreset = preset.parse()?;
    let playbook = LatencyPlaybook::from_preset(p);

    if dry_run {
        println!("Server: {}", server.cyan().bold());
        println!("Preset: {}", playbook.preset.to_string().yellow().bold());
        println!("  - View Distance        : {}", playbook.view_distance);
        println!("  - Simulation Distance  : {}", playbook.simulation_distance);
        println!("  - Entity Tracking Range: {}%", playbook.entity_tracking_range_percent);
        println!("  - Target TPS           : {:.1}", playbook.target_tps);
        println!("{}", "[DRY-RUN] No configurations were modified.".yellow());
        return Ok(());
    }

    let mut client = DaemonClient::connect(paths).await.ok();
    if let Some(ref mut c) = client {
        let (vd, sd, msg) = c.apply_latency_playbook(server.to_string(), preset.to_string()).await?;
        println!("{} {}", "[OK]".green().bold(), msg);
        println!("Applied view-distance={}, simulation-distance={}", vd, sd);
    } else {
        let pb = craft_daemon::EdgeStateBroker::apply_playbook_to_server(paths, server, preset)?;
        println!(
            "{} Applied playbook '{}' locally to server '{}': view-distance={}, simulation-distance={}",
            "[OK]".green().bold(),
            pb.preset.to_string().yellow(),
            server.cyan().bold(),
            pb.view_distance,
            pb.simulation_distance
        );
    }

    Ok(())
}
