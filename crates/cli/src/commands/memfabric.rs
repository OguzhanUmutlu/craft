// crates/cli/src/commands/memfabric.rs
//
// CLI command handler for Autonomous Distributed Inter-Server Memory Fabric,
// Remote Paged Compaction & Cluster NVRAM Pool.
// Strictly zero emojis.

use crate::cli::MemFabricCommands;
use craft_core::error::Result;
use craft_core::memfabric::{
    render_memfabric_bench_text, render_memfabric_pages_text, render_memfabric_status_text,
    MemFabricBenchmarkMetrics, MemFabricNodeInfo, MemFabricStatusSummary, MemoryTier,
    RemotePageDescriptor,
};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::MemFabricService;
use serde_json::json;

pub async fn handle_memfabric(paths: &CraftPaths, action: Option<MemFabricCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(MemFabricCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(MemFabricCommands::PageAlloc {
            page_id,
            size,
            tier,
            dimension,
            server,
            json,
        }) => handle_page_alloc(paths, page_id, size, tier, dimension, server, json).await,
        Some(MemFabricCommands::EvictDim {
            dimension,
            target_node,
            server,
            json,
        }) => handle_evict_dim(paths, dimension, target_node, server, json).await,
        Some(MemFabricCommands::Pages { server, json }) => handle_pages(paths, server, json).await,
        Some(MemFabricCommands::Bench {
            iterations,
            page_size,
            server,
            json,
        }) => handle_bench(paths, iterations, page_size, server, json).await,
        Some(MemFabricCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<(MemFabricStatusSummary, Vec<MemFabricNodeInfo>)> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_memfabric_status(server.clone()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = MemFabricService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = MemFabricService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let (summary, nodes) = fetch_status(paths, server).await?;

    if json {
        println!(
            "{}",
            json!({
                "status": summary,
                "nodes": nodes,
                "active_nodes": summary.active_nodes,
                "total_dram_bytes": summary.total_dram_bytes,
                "allocated_dram_bytes": summary.allocated_dram_bytes,
                "total_nvram_bytes": summary.total_nvram_bytes,
                "allocated_nvram_bytes": summary.allocated_nvram_bytes,
                "dram_utilization_percent": summary.dram_utilization_percent,
                "nvram_utilization_percent": summary.nvram_utilization_percent,
                "total_pages_managed": summary.total_pages_managed,
                "remote_pages_count": summary.remote_pages_count,
                "page_faults_total": summary.page_faults_total,
                "avg_page_fault_latency_nanos": summary.avg_page_fault_latency_nanos,
                "remote_evictions_total": summary.remote_evictions_total,
            })
        );
    } else {
        print!("{}", render_memfabric_status_text(&summary, &nodes));
    }

    Ok(())
}

async fn handle_page_alloc(
    paths: &CraftPaths,
    page_id: String,
    size: usize,
    tier_str: String,
    dimension: Option<String>,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let tier = tier_str.parse::<MemoryTier>().unwrap_or(MemoryTier::LocalDram);

    let page: RemotePageDescriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .allocate_memfabric_page(server.clone(), page_id.clone(), size, tier, dimension.clone())
                .await
            {
                Ok(p) => p,
                Err(_) => {
                    let service = MemFabricService::global(paths);
                    service.allocate_page(page_id, size, tier, dimension)?
                }
            }
        }
        Err(_) => {
            let service = MemFabricService::global(paths);
            service.allocate_page(page_id, size, tier, dimension)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "page_id": page.page_id,
                "virtual_addr": format!("0x{:x}", page.virtual_addr),
                "page_size": page.page_size,
                "tier": page.tier.as_str(),
                "node_id": page.node_id,
                "remote_mr_id": page.remote_mr_id,
                "remote_offset": page.remote_offset,
                "protection": page.protection.as_str(),
                "dimension": page.dimension,
                "created_at_secs": page.created_at_secs,
            })
        );
    } else {
        println!("=== Memory Fabric Page Allocated ===");
        println!("  Page ID:       {}", page.page_id);
        println!("  Virtual Addr:  0x{:x}", page.virtual_addr);
        println!("  Size:          {} bytes ({:.1} KB)", page.page_size, page.page_size as f64 / 1024.0);
        println!("  Memory Tier:   {}", page.tier.as_str());
        println!("  Node ID:       {}", page.node_id);
        if let Some(ref mr) = page.remote_mr_id {
            println!("  Remote MR ID:  {}", mr);
        }
        if let Some(ref dim) = page.dimension {
            println!("  Dimension:     {}", dim);
        }
    }

    Ok(())
}

async fn handle_evict_dim(
    paths: &CraftPaths,
    dimension: String,
    target_node: Option<String>,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let (pages_count, bytes_freed, dim_name) = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .evict_memfabric_dimension(server.clone(), dimension.clone(), target_node.clone())
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = MemFabricService::global(paths);
                    let (cnt, bytes) = service.evict_dimension(&dimension, target_node.as_deref())?;
                    (cnt, bytes, dimension)
                }
            }
        }
        Err(_) => {
            let service = MemFabricService::global(paths);
            let (cnt, bytes) = service.evict_dimension(&dimension, target_node.as_deref())?;
            (cnt, bytes, dimension)
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "dimension": dim_name,
                "pages_evicted": pages_count,
                "bytes_freed": bytes_freed,
                "target_node": target_node.unwrap_or_else(|| "remote-fabric-node-1".to_string()),
            })
        );
    } else {
        println!(
            "[OK] Evicted dormant dimension '{}' to remote NVRAM ({} pages, {:.2} MB freed)",
            dim_name,
            pages_count,
            bytes_freed as f64 / (1024.0 * 1024.0)
        );
    }

    Ok(())
}

async fn handle_pages(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let pages: Vec<RemotePageDescriptor> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_memfabric_pages(server.clone()).await {
            Ok(p) => p,
            Err(_) => {
                let service = MemFabricService::global(paths);
                service.list_pages(server.as_deref())?
            }
        },
        Err(_) => {
            let service = MemFabricService::global(paths);
            service.list_pages(server.as_deref())?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&pages).unwrap());
    } else {
        print!("{}", render_memfabric_pages_text(&pages));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    page_size: usize,
    _server: Option<String>,
    json: bool,
) -> Result<()> {
    let metrics: MemFabricBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client.run_memfabric_bench(iterations, page_size).await {
                Ok(m) => m,
                Err(_) => {
                    let service = MemFabricService::global(paths);
                    service.run_bench(iterations, page_size)?
                }
            }
        }
        Err(_) => {
            let service = MemFabricService::global(paths);
            service.run_bench(iterations, page_size)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        print!("{}", render_memfabric_bench_text(&metrics));
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
            match client.reset_memfabric_metrics(server.clone()).await {
                Ok(m) => m,
                Err(_) => {
                    let service = MemFabricService::global(paths);
                    service.reset_metrics(server.as_deref())?;
                    "MemFabric metrics reset successfully".to_string()
                }
            }
        }
        Err(_) => {
            let service = MemFabricService::global(paths);
            service.reset_metrics(server.as_deref())?;
            "MemFabric metrics reset successfully".to_string()
        }
    };

    if json {
        println!("{}", json!({ "status": "ok", "message": msg }));
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
