// crates/cli/src/commands/rdma.rs
//
// CLI command handler for Autonomous RDMA Network Acceleration,
// InfiniBand/RoCE Direct Memory Offloading & Sub-Microsecond Inter-Server Fabric.
// Strictly zero emojis.

use crate::cli::RdmaCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::rdma::{
    render_rdma_bench_text, render_rdma_peers_text, render_rdma_status_text,
    MemoryRegionDescriptor, RdmaBenchmarkMetrics, RdmaPeerEndpoint,
    RdmaStatusSummary,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::RdmaService;
use serde_json::json;

pub async fn handle_rdma(paths: &CraftPaths, action: Option<RdmaCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(RdmaCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(RdmaCommands::MrRegister {
            server,
            size,
            read_only,
            atomic,
            json,
        }) => handle_mr_register(paths, server, size, read_only, atomic, json).await,
        Some(RdmaCommands::Connect {
            peer,
            addr,
            transport,
            mtu,
            json,
        }) => handle_connect(paths, peer, addr, transport, mtu, json).await,
        Some(RdmaCommands::Peers { json }) => handle_peers(paths, json).await,
        Some(RdmaCommands::Bench {
            peer,
            iterations,
            size,
            json,
        }) => handle_bench(paths, peer, iterations, size, json).await,
        Some(RdmaCommands::ResetMetrics { json }) => handle_reset_metrics(paths, json).await,
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<RdmaStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_rdma_status(server.clone()).await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = RdmaService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = RdmaService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, server).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        print!("{}", render_rdma_status_text(&summary));
    }

    Ok(())
}

async fn handle_mr_register(
    paths: &CraftPaths,
    server: Option<String>,
    size: usize,
    read_only: bool,
    _atomic: bool,
    json: bool,
) -> Result<()> {
    let mr: MemoryRegionDescriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.register_rdma_mr(server.clone(), size, read_only).await {
            Ok(r) => r,
            Err(_) => {
                let service = RdmaService::global(paths);
                service.register_mr(server.as_deref(), size, read_only)?
            }
        },
        Err(_) => {
            let service = RdmaService::global(paths);
            service.register_mr(server.as_deref(), size, read_only)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&mr).unwrap());
    } else {
        println!("=== RDMA Memory Region Registered ===");
        println!("  MR ID:             {}", mr.mr_id);
        println!("  Length:            {} bytes ({:.2} MB)", mr.length, mr.length as f64 / (1024.0 * 1024.0));
        println!("  Base Addr:         0x{:016x}", mr.addr);
        println!("  Local Key:         0x{:08x}", mr.lkey);
        println!("  Remote Key:        0x{:08x}", mr.rkey);
        println!("  Protection Domain: {}", mr.protection_domain_id);
        println!("  Access Flags:      {}", mr.access_flags.as_str());
    }

    Ok(())
}

async fn handle_connect(
    paths: &CraftPaths,
    peer: String,
    addr: String,
    _transport: String,
    _mtu: u32,
    json: bool,
) -> Result<()> {
    let qp_num: u32 = 1001;
    let endpoint: RdmaPeerEndpoint = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .connect_rdma_peer(Some(peer.clone()), addr.clone(), qp_num)
                .await
            {
                Ok(ep) => ep,
                Err(_) => {
                    let service = RdmaService::global(paths);
                    service.connect_peer(Some(&peer), &addr, qp_num)?
                }
            }
        }
        Err(_) => {
            let service = RdmaService::global(paths);
            service.connect_peer(Some(&peer), &addr, qp_num)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&endpoint).unwrap());
    } else {
        println!("=== RDMA Peer Queue Pair Connected ===");
        println!("  Node ID:      {}", endpoint.node_id);
        println!("  Server:       {}", endpoint.server_name);
        println!("  Remote GID:   {}", endpoint.gid_or_ip);
        println!("  Transport:    {}", endpoint.transport.as_str());
        println!("  QP Number:    {}", endpoint.qp_num);
        println!("  Remote Key:   0x{:08x}", endpoint.rkey);
        println!("  Link Status:  {}", endpoint.link_status.as_str());
        println!("  RTT Latency:  {} ns ({:.3} us)", endpoint.rtt_nanos, endpoint.rtt_nanos as f64 / 1000.0);
        println!("  Bandwidth:    {:.2} Gbps", endpoint.bandwidth_gbps);
    }

    Ok(())
}

async fn handle_peers(paths: &CraftPaths, json: bool) -> Result<()> {
    let peers: Vec<RdmaPeerEndpoint> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_rdma_peers(None).await {
            Ok(p) => p,
            Err(_) => {
                let service = RdmaService::global(paths);
                service.list_peers(None)?
            }
        },
        Err(_) => {
            let service = RdmaService::global(paths);
            service.list_peers(None)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&peers).unwrap());
    } else {
        print!("{}", render_rdma_peers_text(&peers));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    _peer: Option<String>,
    iterations: usize,
    size: usize,
    json: bool,
) -> Result<()> {
    let metrics: RdmaBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_rdma_bench(iterations, size).await {
            Ok(m) => m,
            Err(_) => {
                let service = RdmaService::global(paths);
                service.run_bench(iterations, size)?
            }
        },
        Err(_) => {
            let service = RdmaService::global(paths);
            service.run_bench(iterations, size)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        print!("{}", render_rdma_bench_text(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, json: bool) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_rdma_metrics(None).await {
            Ok(m) => m,
            Err(_) => {
                let service = RdmaService::global(paths);
                service.reset_metrics(None)?;
                "RDMA metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = RdmaService::global(paths);
            service.reset_metrics(None)?;
            "RDMA metrics reset successfully".to_string()
        }
    };

    if json {
        println!("{}", json!({ "status": "ok", "message": msg }));
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
