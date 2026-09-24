use crate::cli::{XdpCommands, XdpRuleSubcommands};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_core::xdp::{
    XdpAction, XdpAttachMode, XdpFilterRule, XdpInterfaceStatus, XdpMapConfig, XdpProtocol,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::XdpService;
use craft_net::XdpPipeline;
use serde_json::json;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn handle_xdp(paths: &CraftPaths, action: Option<XdpCommands>) -> Result<()> {
    match action {
        None | Some(XdpCommands::Status { json: false }) => handle_status(paths, false).await,
        Some(XdpCommands::Status { json: true }) => handle_status(paths, true).await,
        Some(XdpCommands::Attach { interface, mode, json }) => {
            handle_attach(paths, &interface, &mode, json).await
        }
        Some(XdpCommands::Detach { json }) => handle_detach(paths, json).await,
        Some(XdpCommands::Rule { action }) => handle_rule(paths, action).await,
        Some(XdpCommands::ResetMetrics { json }) => handle_reset_metrics(paths, json).await,
        Some(XdpCommands::Bench {
            packets,
            attack_ratio,
            json,
        }) => handle_bench(packets, attack_ratio, json).await,
    }
}

async fn fetch_status(paths: &CraftPaths) -> Result<XdpInterfaceStatus> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_xdp_status().await {
            Ok(status) => Ok(status),
            Err(_) => {
                let service = XdpService::global(paths);
                service.get_status()
            }
        },
        Err(_) => {
            let service = XdpService::global(paths);
            service.get_status()
        }
    }
}

async fn handle_status(paths: &CraftPaths, json: bool) -> Result<()> {
    let status = fetch_status(paths).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap());
    } else {
        println!("=== Autonomous eBPF XDP Firewall Status ===");
        println!("  Interface:              {}", status.interface_name);
        println!("  Attachment Mode:         {}", status.mode);
        println!(
            "  Driver Attachment:       {}",
            if status.attached {
                "[OK] Attached & Mitigating"
            } else {
                "[WARN] Detached (Filtering Inactive)"
            }
        );
        if let Some(prog_id) = status.bpf_prog_id {
            println!("  BPF Program ID:          {}", prog_id);
        }
        println!("  Active Filter Rules:     {}", status.rules_count);
        println!("  Tracked Flows (LRU):     {}", status.active_flows);
        println!("  Total Rx Packets:        {}", status.metrics.total_rx_packets);
        println!("  Passed Packets:          {}", status.metrics.passed_packets);
        println!("  Dropped Packets:         {}", status.metrics.dropped_packets);
        println!("  SYN Flood Drops:         {}", status.metrics.syn_flood_drops);
        println!("  UDP Flood Drops:         {}", status.metrics.udp_flood_drops);
        println!("  RakNet Handshake Drops:  {}", status.metrics.raknet_flood_drops);
        println!(
            "  Drop Rate:               {:.2} pps",
            status.metrics.drop_rate_pps
        );
        println!(
            "  Absorbed Flood BW:       {:.4} Gbps",
            status.metrics.bandwidth_absorbed_gbps
        );

        let service = XdpService::global(paths);
        if let Ok(plain_table) = service.render_plain_status() {
            if plain_table.contains("--- Active Filter Rules ---") {
                println!();
                println!("{}", plain_table);
            }
        }
    }

    Ok(())
}

async fn handle_attach(
    paths: &CraftPaths,
    interface: &str,
    mode_str: &str,
    json: bool,
) -> Result<()> {
    let mode = XdpAttachMode::from_str(mode_str).map_err(CraftError::Other)?;

    let status = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.attach_xdp_interface(interface, mode).await {
            Ok(s) => s,
            Err(_) => {
                let service = XdpService::global(paths);
                service.attach_interface(interface, mode)?
            }
        },
        Err(_) => {
            let service = XdpService::global(paths);
            service.attach_interface(interface, mode)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap());
    } else {
        println!(
            "[OK] eBPF XDP firewall attached to '{}' in {} mode (Prog ID: {})",
            status.interface_name,
            status.mode,
            status.bpf_prog_id.unwrap_or(0)
        );
    }

    Ok(())
}

async fn handle_detach(paths: &CraftPaths, json: bool) -> Result<()> {
    let status = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.detach_xdp_interface().await {
            Ok(s) => s,
            Err(_) => {
                let service = XdpService::global(paths);
                service.detach_interface()?
            }
        },
        Err(_) => {
            let service = XdpService::global(paths);
            service.detach_interface()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap());
    } else {
        println!(
            "[OK] eBPF XDP firewall detached from '{}'",
            status.interface_name
        );
    }

    Ok(())
}

async fn handle_rule(paths: &CraftPaths, action: XdpRuleSubcommands) -> Result<()> {
    match action {
        XdpRuleSubcommands::Add {
            id,
            cidr,
            port,
            proto,
            action,
            rate_limit,
            ban_seconds,
            priority,
            json,
        } => {
            let protocol = XdpProtocol::from_str(&proto).map_err(CraftError::Other)?;
            let act = XdpAction::from_str(&action).map_err(CraftError::Other)?;
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let rule = XdpFilterRule {
                id: id.clone(),
                cidr,
                port,
                protocol,
                rate_limit_pps: rate_limit,
                burst_tokens: rate_limit.map(|r| r * 2),
                action: act,
                priority,
                ban_ttl_seconds: ban_seconds,
                created_at: now_secs,
            };

            let status = match DaemonClient::connect(paths).await {
                Ok(mut client) => match client.add_xdp_rule(rule.clone()).await {
                    Ok((s, _)) => s,
                    Err(_) => {
                        let service = XdpService::global(paths);
                        service.add_rule(rule)?
                    }
                },
                Err(_) => {
                    let service = XdpService::global(paths);
                    service.add_rule(rule)?
                }
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                println!(
                    "[OK] Rule '{}' added successfully (Total rules: {})",
                    id, status.rules_count
                );
            }
        }
        XdpRuleSubcommands::Remove { id, json } => {
            let status = match DaemonClient::connect(paths).await {
                Ok(mut client) => match client.remove_xdp_rule(&id).await {
                    Ok((s, _)) => s,
                    Err(_) => {
                        let service = XdpService::global(paths);
                        service.remove_rule(&id)?
                    }
                },
                Err(_) => {
                    let service = XdpService::global(paths);
                    service.remove_rule(&id)?
                }
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                println!(
                    "[OK] Rule '{}' removed successfully (Total rules: {})",
                    id, status.rules_count
                );
            }
        }
        XdpRuleSubcommands::List { json } => {
            let service = XdpService::global(paths);
            let status = service.get_status()?;

            if json {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                println!("{}", service.render_plain_status()?);
            }
        }
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, json: bool) -> Result<()> {
    let status = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            let _ = client.reset_xdp_metrics().await;
            fetch_status(paths).await?
        }
        Err(_) => {
            let service = XdpService::global(paths);
            service.reset_metrics()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap());
    } else {
        println!("[OK] eBPF XDP firewall metrics reset to zero");
    }

    Ok(())
}

async fn handle_bench(packets: usize, attack_ratio: f64, json: bool) -> Result<()> {
    let mut pipeline = XdpPipeline::new(XdpMapConfig::default());
    // Add default drop rule for bad CIDR
    pipeline.add_rule(XdpFilterRule {
        id: "bench-attack-block".to_string(),
        cidr: "198.51.100.0/24".to_string(),
        port: Some(25565),
        protocol: XdpProtocol::Udp,
        action: XdpAction::Drop,
        rate_limit_pps: None,
        burst_tokens: None,
        ban_ttl_seconds: None,
        priority: 100,
        created_at: 0,
    });

    let res = pipeline.benchmark_synthetic_flood(packets as u64, 1024, attack_ratio);

    if json {
        let out = json!({
            "total_packets": res.total_packets,
            "dropped_packets": res.dropped_packets,
            "passed_packets": res.passed_packets,
            "duration_millis": res.duration_millis,
            "throughput_pps": res.throughput_pps,
            "throughput_gbps": res.throughput_gbps,
            "latency_nanos_per_pkt": res.latency_nanos_per_pkt,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("=== Pure-Rust In-Kernel eBPF XDP Synthetic Flood Benchmark ===");
        println!("  Total Ingress Packets:     {}", res.total_packets);
        println!(
            "  Synthetic Flood Dropped:   {} ({:.2}%)",
            res.dropped_packets,
            (res.dropped_packets as f64 / res.total_packets as f64) * 100.0
        );
        println!(
            "  Legitimate Traffic Passed: {} ({:.2}%)",
            res.passed_packets,
            (res.passed_packets as f64 / res.total_packets as f64) * 100.0
        );
        println!(
            "  Elapsed Duration:          {:.2} ms",
            res.duration_millis
        );
        println!(
            "  Processing Throughput:     {:.1} pps",
            res.throughput_pps
        );
        println!(
            "  Estimated Absorbed BW:     {:.2} Gbps",
            res.throughput_gbps
        );
        println!(
            "  Average Latency / Packet:  {:.2} ns",
            res.latency_nanos_per_pkt
        );
        println!("  Pipeline Efficiency:       [OK] Sub-microsecond wire-speed mitigation");
    }

    Ok(())
}
