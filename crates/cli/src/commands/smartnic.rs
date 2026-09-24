// crates/cli/src/commands/smartnic.rs
//
// CLI command handler for Autonomous eBPF XDP Hardware Offloading,
// SmartNIC Acceleration & P4 Programmable Data Plane Line-Rate Switching.
// Strictly zero emojis.

use crate::cli::SmartNicCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::smartnic::{
    render_smartnic_bench_text, render_smartnic_rules_text, render_smartnic_status_text,
    OffloadProtocol, P4ActionType, SmartNicBenchmarkMetrics, SmartNicDeviceInfo,
    SmartNicOffloadRule, SmartNicStatusSummary,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::SmartNicService;
use serde_json::json;

pub async fn handle_smartnic(paths: &CraftPaths, action: Option<SmartNicCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(SmartNicCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(SmartNicCommands::RuleAdd {
            rule_id,
            protocol,
            action,
            port,
            cidr,
            priority,
            server,
            json,
        }) => {
            handle_rule_add(
                paths, rule_id, protocol, action, port, cidr, priority, server, json,
            )
            .await
        }
        Some(SmartNicCommands::RuleRm {
            rule_id,
            server,
            json,
        }) => handle_rule_rm(paths, rule_id, server, json).await,
        Some(SmartNicCommands::Rules { server, json }) => handle_rules(paths, server, json).await,
        Some(SmartNicCommands::Bench {
            iterations,
            packet_size,
            server,
            json,
        }) => handle_bench(paths, iterations, packet_size, server, json).await,
        Some(SmartNicCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<(SmartNicStatusSummary, Vec<SmartNicDeviceInfo>)> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_smartnic_status(server.clone()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = SmartNicService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (summary, devices) = fetch_status(paths, server).await?;

    if json {
        println!(
            "{}",
            json!({
                "status": summary,
                "devices": devices,
                "offload_mode": summary.offload_mode.as_str(),
                "vendor": devices.first().map(|d| d.vendor.as_str()).unwrap_or("generic_p4_emulated"),
                "tcam_rules_used": devices.first().map(|d| d.used_tcam_rules).unwrap_or(0),
                "tcam_rules_capacity": devices.first().map(|d| d.total_tcam_rules).unwrap_or(16384),
                "cpu_overhead_percent": (100.0 - summary.host_cpu_saved_percent).max(0.0),
                "fallback_driver_active": summary.fallback_count > 0,
                "hardware_offloaded_packets": summary.offloaded_packets,
                "hardware_offloaded_bytes": summary.offloaded_bytes,
            })
        );
    } else {
        print!("{}", render_smartnic_status_text(&summary, &devices));
    }

    Ok(())
}

async fn handle_rule_add(
    paths: &CraftPaths,
    rule_id: String,
    protocol: String,
    action: String,
    port: Option<u16>,
    cidr: Option<String>,
    priority: u32,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let proto = protocol
        .parse::<OffloadProtocol>()
        .unwrap_or(OffloadProtocol::MinecraftJavaSlp);

    let action_type = match action.to_lowercase().as_str() {
        "drop" => P4ActionType::Drop,
        "forward" => P4ActionType::ForwardPort(port.unwrap_or(25565)),
        "pong" | "slp" | "raknet" => P4ActionType::SendPongDirect,
        "syn_cookie" | "syncookie" => P4ActionType::SendSynCookie,
        "rate_limit" | "ratelimit" => P4ActionType::RateLimitToken(1000),
        "pass" | "pass_to_host" => P4ActionType::PassToHost,
        _ => P4ActionType::SendPongDirect,
    };

    let rule = SmartNicOffloadRule {
        rule_id,
        priority,
        protocol: proto,
        match_port: port,
        match_cidr: cidr,
        action: action_type,
        hardware_installed: true,
        hits: 0,
        bytes: 0,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let installed: SmartNicOffloadRule = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.install_smartnic_rule(server.clone(), rule.clone()).await {
            Ok(r) => r,
            Err(_) => {
                let service = SmartNicService::global(paths);
                service.install_rule(rule)?
            }
        },
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.install_rule(rule)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "rule_id": installed.rule_id,
                "priority": installed.priority,
                "protocol": installed.protocol.as_str(),
                "action": installed.action,
                "port": installed.match_port,
                "cidr": installed.match_cidr,
                "offload_hardware": installed.hardware_installed,
                "hits": installed.hits,
                "bytes": installed.bytes,
                "created_at": installed.created_at,
            })
        );
    } else {
        println!("=== SmartNIC Offload Rule Installed ===");
        println!("  Rule ID:     {}", installed.rule_id);
        println!("  Protocol:    {}", installed.protocol.as_str());
        println!("  Action:      {}", installed.action);
        if let Some(port) = installed.match_port {
            println!("  Port:        {}", port);
        }
        if let Some(ref cidr) = installed.match_cidr {
            println!("  CIDR:        {}", cidr);
        }
        println!("  Priority:    {}", installed.priority);
        println!(
            "  Hardware:    {}",
            if installed.hardware_installed {
                "Yes (ASIC TCAM)"
            } else {
                "No (Fallback Driver)"
            }
        );
    }

    Ok(())
}

async fn handle_rule_rm(
    paths: &CraftPaths,
    rule_id: String,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let removed = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .remove_smartnic_rule(server.clone(), rule_id.clone())
                .await
            {
                Ok(r) => r,
                Err(_) => {
                    let service = SmartNicService::global(paths);
                    service.remove_rule(&rule_id)?
                }
            }
        }
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.remove_rule(&rule_id)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": if removed { "ok" } else { "not_found" },
                "rule_id": rule_id,
                "removed": removed
            })
        );
    } else if removed {
        println!(
            "[OK] SmartNIC offload rule '{}' removed from hardware",
            rule_id
        );
    } else {
        println!("[WARN] SmartNIC offload rule '{}' was not found", rule_id);
    }

    Ok(())
}

async fn handle_rules(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let rules: Vec<SmartNicOffloadRule> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_smartnic_rules(server.clone()).await {
            Ok(r) => r,
            Err(_) => {
                let service = SmartNicService::global(paths);
                service.list_rules(server.as_deref())?
            }
        },
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.list_rules(server.as_deref())?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&rules).unwrap());
    } else {
        print!("{}", render_smartnic_rules_text(&rules));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    packet_size: usize,
    _server: Option<String>,
    json: bool,
) -> Result<()> {
    let metrics: SmartNicBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client.run_smartnic_bench(iterations, packet_size).await {
                Ok(m) => m,
                Err(_) => {
                    let service = SmartNicService::global(paths);
                    service.run_bench(iterations, packet_size)?
                }
            }
        }
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.run_bench(iterations, packet_size)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        print!("{}", render_smartnic_bench_text(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client.reset_smartnic_metrics(server.clone()).await {
                Ok(m) => m,
                Err(_) => {
                    let service = SmartNicService::global(paths);
                    service.reset_metrics(server.as_deref())?;
                    "SmartNIC metrics reset successfully".to_string()
                }
            }
        }
        Err(_) => {
            let service = SmartNicService::global(paths);
            service.reset_metrics(server.as_deref())?;
            "SmartNIC metrics reset successfully".to_string()
        }
    };

    if json {
        println!("{}", json!({ "status": "ok", "message": msg }));
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
