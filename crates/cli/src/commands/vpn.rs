// crates/cli/src/commands/vpn.rs
//
// CLI command handler for Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh,
// WireGuard PQXDH & P4 Crypto Offloading.
// Strictly zero emojis.

use crate::cli::VpnCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::vpn::{
    render_vpn_bench_text, render_vpn_status_text, render_vpn_tunnels_text,
    VpnBenchmarkMetrics, VpnPeerConfig, VpnStatusSummary, VpnTunnelDescriptor,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::VpnMeshService;
use serde_json::json;

pub async fn handle_vpn(paths: &CraftPaths, action: Option<VpnCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(VpnCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(VpnCommands::Tunnels { server, json }) => handle_tunnels(paths, server, json).await,
        Some(VpnCommands::TunnelCreate {
            tunnel_id,
            address,
            port,
            crypto_mode,
            json,
        }) => handle_tunnel_create(paths, tunnel_id, address, port, crypto_mode, json).await,
        Some(VpnCommands::TunnelDelete { tunnel_id, json }) => {
            handle_tunnel_delete(paths, tunnel_id, json).await
        }
        Some(VpnCommands::PeerAdd {
            tunnel_id,
            peer_id,
            endpoint,
            allowed_ips,
            json,
        }) => handle_peer_add(paths, tunnel_id, peer_id, endpoint, allowed_ips, json).await,
        Some(VpnCommands::PeerRm {
            tunnel_id,
            peer_id,
            json,
        }) => handle_peer_rm(paths, tunnel_id, peer_id, json).await,
        Some(VpnCommands::RotateKey {
            tunnel_id,
            peer_id,
            json,
        }) => handle_rotate_key(paths, tunnel_id, peer_id, json).await,
        Some(VpnCommands::Bench {
            iterations,
            packet_size,
            json,
        }) => handle_bench(paths, iterations, packet_size, json).await,
        Some(VpnCommands::ResetMetrics { json }) => handle_reset_metrics(paths, json).await,
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<(VpnStatusSummary, Vec<VpnTunnelDescriptor>)> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_vpn_status(server.clone()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = VpnMeshService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (summary, tunnels) = fetch_status(paths, server).await?;

    if json {
        let output = json!({
            "status": summary,
            "summary": summary,
            "tunnels": tunnels,
            "active_tunnels": summary.active_tunnels,
            "active_peers": summary.active_peers,
            "total_rx_bytes": summary.total_rx_bytes,
            "total_tx_bytes": summary.total_tx_bytes,
            "throughput_gbps": summary.throughput_gbps,
            "avg_latency_micros": summary.avg_latency_micros,
            "key_rotations_total": summary.key_rotations_total,
            "quantum_defense_score": summary.quantum_defense_score,
            "hardware_offload_active": summary.hardware_offload_active,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_vpn_status_text(&summary));
        println!();
        println!("{}", render_vpn_tunnels_text(&tunnels));
    }

    Ok(())
}

async fn handle_tunnels(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (_summary, tunnels) = fetch_status(paths, server).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tunnels)?);
    } else {
        println!("{}", render_vpn_tunnels_text(&tunnels));
    }
    Ok(())
}

async fn handle_tunnel_create(
    paths: &CraftPaths,
    tunnel_id: String,
    address: String,
    port: u16,
    crypto_mode: Option<String>,
    json: bool,
) -> Result<()> {
    let descriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .create_vpn_tunnel(
                    tunnel_id.clone(),
                    address.clone(),
                    port,
                    crypto_mode.clone(),
                )
                .await?
        }
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.create_tunnel(
                &tunnel_id,
                &address,
                port,
                crypto_mode.as_deref(),
            )?
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "success": true,
            "message": "VPN tunnel created successfully",
            "tunnel": descriptor,
            "tunnel_id": descriptor.tunnel_id,
            "address": descriptor.local_address,
            "port": descriptor.listen_port,
            "crypto_mode": descriptor.crypto_mode,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "[OK] Created WireGuard PQXDH tunnel '{}' (address: {}, port: {}, mode: {})",
            descriptor.tunnel_id,
            descriptor.local_address,
            descriptor.listen_port,
            descriptor.crypto_mode
        );
    }

    Ok(())
}

async fn handle_tunnel_delete(paths: &CraftPaths, tunnel_id: String, json: bool) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.delete_vpn_tunnel(tunnel_id.clone()).await?,
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.delete_tunnel(&tunnel_id)?
        }
    };

    if json {
        let output = json!({
            "status": if success { "deleted" } else { "error" },
            "success": success,
            "tunnel_id": tunnel_id,
            "deleted": success,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if success {
        println!("[OK] Deleted WireGuard PQXDH tunnel '{}'", tunnel_id);
    } else {
        println!("[ERROR] VPN tunnel '{}' was not found or could not be deleted", tunnel_id);
    }

    Ok(())
}

async fn handle_peer_add(
    paths: &CraftPaths,
    tunnel_id: String,
    peer_id: String,
    endpoint: String,
    allowed_ips: Vec<String>,
    json: bool,
) -> Result<()> {
    let peer: VpnPeerConfig = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .add_vpn_peer(
                    tunnel_id.clone(),
                    peer_id.clone(),
                    endpoint.clone(),
                    allowed_ips.clone(),
                )
                .await?
        }
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.add_peer(&tunnel_id, &peer_id, &endpoint, allowed_ips)?
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "success": true,
            "tunnel_id": tunnel_id,
            "peer_id": peer.peer_id,
            "peer": peer,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "[OK] Registered peer '{}' to tunnel '{}' (endpoint: {}, allowed IPs: {})",
            peer.peer_id,
            tunnel_id,
            peer.endpoint,
            peer.allowed_ips.join(", ")
        );
    }

    Ok(())
}

async fn handle_peer_rm(
    paths: &CraftPaths,
    tunnel_id: String,
    peer_id: String,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .remove_vpn_peer(tunnel_id.clone(), peer_id.clone())
                .await?
        }
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.remove_peer(&tunnel_id, &peer_id)?
        }
    };

    if json {
        let output = json!({
            "status": if success { "deleted" } else { "error" },
            "success": success,
            "tunnel_id": tunnel_id,
            "peer_id": peer_id,
            "removed": success,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if success {
        println!(
            "[OK] Removed peer '{}' from tunnel '{}'",
            peer_id, tunnel_id
        );
    } else {
        println!(
            "[ERROR] Peer '{}' was not found in tunnel '{}'",
            peer_id, tunnel_id
        );
    }

    Ok(())
}

async fn handle_rotate_key(
    paths: &CraftPaths,
    tunnel_id: String,
    peer_id: Option<String>,
    json: bool,
) -> Result<()> {
    let latency_micros = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            client
                .rotate_vpn_key(tunnel_id.clone(), peer_id.clone())
                .await?
        }
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.rotate_key(&tunnel_id, peer_id.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "rotated",
            "success": true,
            "tunnel_id": tunnel_id,
            "peer_id": peer_id,
            "renegotiation_micros": latency_micros,
            "renegotiation_latency_micros": latency_micros,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let target = match &peer_id {
            Some(p) => format!("peer '{}' on tunnel '{}'", p, tunnel_id),
            None => format!("all peers on tunnel '{}'", tunnel_id),
        };
        println!(
            "[OK] Zero-loss PQXDH key rotation executed for {} in {} us",
            target, latency_micros
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    packet_size: usize,
    json: bool,
) -> Result<()> {
    let metrics: VpnBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.run_vpn_bench(iterations, packet_size).await?,
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.run_bench(iterations, packet_size)?
        }
    };

    if json {
        let output = json!({
            "status": "ok",
            "metrics": metrics,
            "packets_processed": metrics.packets_processed,
            "packet_size": metrics.packet_size,
            "throughput_gbps": metrics.throughput_gbps,
            "encryption_latency_nanos": metrics.encryption_latency_nanos,
            "renegotiation_latency_micros": metrics.renegotiation_latency_micros,
            "packet_loss_percent": metrics.packet_loss_percent,
            "key_rotations": metrics.key_rotations,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_vpn_bench_text(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, json: bool) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => client.reset_vpn_metrics().await?,
        Err(_) => {
            let service = VpnMeshService::global(paths);
            service.reset_metrics()?;
            "VPN mesh telemetry metrics reset successfully".to_string()
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
