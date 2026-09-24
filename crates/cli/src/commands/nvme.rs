// crates/cli/src/commands/nvme.rs
//
// CLI command handler for Autonomous Zero-Copy Storage Fabrics,
// NVMe-oF Target & Distributed Flash Block Pool.
// Strictly zero emojis.

use crate::cli::NvmeCommands;
use craft_core::error::Result;
use craft_core::nvme::{
    format_bytes, render_nvme_bench_text, render_nvme_namespaces_text,
    render_nvme_status_text, render_nvme_subsystems_text, NvmeBenchmarkMetrics,
    NvmeNamespaceDescriptor, NvmeStatusSummary, NvmeSubsystemDescriptor,
};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::NvmeTargetService;
use serde_json::json;

pub async fn handle_nvme(paths: &CraftPaths, action: Option<NvmeCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(NvmeCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(NvmeCommands::NsCreate {
            nsid,
            size_mb,
            block_size,
            server,
            dimension,
            json,
        }) => handle_ns_create(paths, nsid, size_mb, block_size, server, dimension, json).await,
        Some(NvmeCommands::NsDelete { nsid, json }) => handle_ns_delete(paths, nsid, json).await,
        Some(NvmeCommands::Namespaces { server, json }) => handle_namespaces(paths, server, json).await,
        Some(NvmeCommands::Subsystems { json }) => handle_subsystems(paths, json).await,
        Some(NvmeCommands::Bench {
            iterations,
            block_size,
            json,
        }) => handle_bench(paths, iterations, block_size, json).await,
        Some(NvmeCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<(NvmeStatusSummary, Vec<NvmeSubsystemDescriptor>)> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_nvme_status(server.clone()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (summary, subsystems) = fetch_status(paths, server).await?;

    if json {
        println!(
            "{}",
            json!({
                "status": summary,
                "subsystems": subsystems,
                "active_subsystems": summary.active_subsystems,
                "active_namespaces": summary.active_namespaces,
                "active_controllers": summary.active_controllers,
                "total_pool_bytes": summary.total_pool_bytes,
                "allocated_pool_bytes": summary.allocated_pool_bytes,
                "pool_utilization_percent": summary.pool_utilization_percent,
                "iops_current": summary.iops_current,
                "avg_latency_micros": summary.avg_latency_micros,
                "multipath_failovers_total": summary.multipath_failovers_total,
            })
        );
    } else {
        print!("{}", render_nvme_status_text(&summary));
    }

    Ok(())
}

async fn handle_ns_create(
    paths: &CraftPaths,
    nsid: u32,
    size_mb: u64,
    block_size: u32,
    server: Option<String>,
    dimension: Option<String>,
    json: bool,
) -> Result<()> {
    let ns: NvmeNamespaceDescriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .create_nvme_namespace(
                    nsid,
                    size_mb,
                    block_size,
                    server.clone(),
                    dimension.clone(),
                )
                .await
            {
                Ok(n) => n,
                Err(_) => {
                    let service = NvmeTargetService::global(paths);
                    service.create_namespace(nsid, size_mb, block_size, server, dimension)?
                }
            }
        }
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.create_namespace(nsid, size_mb, block_size, server, dimension)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "created",
                "nsid": ns.nsid,
                "size_blocks": ns.size_blocks,
                "block_size": ns.block_size,
                "capacity_bytes": ns.capacity_bytes,
                "allocated_bytes": ns.allocated_bytes,
                "server_id": ns.server_id,
                "dimension": ns.dimension,
                "thin_provisioned": ns.thin_provisioned,
                "read_only": ns.read_only,
                "created_at_secs": ns.created_at_secs,
            })
        );
    } else {
        println!("=== NVMe-oF Storage Namespace Provisioned ===");
        println!("  Namespace ID:    {}", ns.nsid);
        println!("  Block Size:      {} bytes", ns.block_size);
        println!("  Block Count:     {} blocks", ns.size_blocks);
        println!("  Capacity:        {}", format_bytes(ns.capacity_bytes));
        if let Some(ref srv) = ns.server_id {
            println!("  Server:          {}", srv);
        }
        if let Some(ref dim) = ns.dimension {
            println!("  Dimension:       {}", dim);
        }
        println!("  Provisioning:    {}", if ns.thin_provisioned { "Thin" } else { "Thick" });
    }

    Ok(())
}

async fn handle_ns_delete(paths: &CraftPaths, nsid: u32, json: bool) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.delete_nvme_namespace(nsid).await {
            Ok(s) => s,
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.delete_namespace(nsid)?
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.delete_namespace(nsid)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": if success { "deleted" } else { "error" },
                "nsid": nsid,
                "success": success,
            })
        );
    } else if success {
        println!("[OK] NVMe storage namespace NSID {} deleted successfully", nsid);
    } else {
        println!("[ERROR] Failed to delete NVMe storage namespace NSID {}", nsid);
    }

    Ok(())
}

async fn handle_namespaces(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let namespaces: Vec<NvmeNamespaceDescriptor> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_nvme_namespaces(server.clone()).await {
            Ok(n) => n,
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.list_namespaces(server.as_deref())?
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.list_namespaces(server.as_deref())?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&namespaces).unwrap_or_default());
    } else {
        print!("{}", render_nvme_namespaces_text(&namespaces));
    }

    Ok(())
}

async fn handle_subsystems(paths: &CraftPaths, json: bool) -> Result<()> {
    let subsystems: Vec<NvmeSubsystemDescriptor> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_nvme_subsystems().await {
            Ok(s) => s,
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.list_subsystems()?
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.list_subsystems()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&subsystems).unwrap_or_default());
    } else {
        print!("{}", render_nvme_subsystems_text(&subsystems));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    block_size: usize,
    json: bool,
) -> Result<()> {
    let metrics: NvmeBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_nvme_bench(block_size, iterations).await {
            Ok(m) => m,
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.run_bench(block_size, iterations)?
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.run_bench(block_size, iterations)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "completed",
                "ops_processed": metrics.ops_processed,
                "block_size": metrics.block_size,
                "iops": metrics.iops,
                "bandwidth_gbps": metrics.bandwidth_gbps,
                "avg_latency_micros": metrics.avg_latency_micros,
                "p99_latency_micros": metrics.p99_latency_micros,
                "multipath_failovers": metrics.multipath_failovers,
            })
        );
    } else {
        print!("{}", render_nvme_bench_text(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, _server: Option<String>, json: bool) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_nvme_metrics().await {
            Ok(m) => m,
            Err(_) => {
                let service = NvmeTargetService::global(paths);
                service.reset_metrics(None)?
            }
        },
        Err(_) => {
            let service = NvmeTargetService::global(paths);
            service.reset_metrics(None)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "message": msg,
            })
        );
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
