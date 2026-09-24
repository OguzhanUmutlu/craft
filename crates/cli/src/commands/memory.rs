use crate::cli::MemoryCommands;
use craft_core::compaction::{PagePoolConfig, ThpDefragMode, ThpMode};
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::CompactionService;
use craft_net::SocketPagePool;
use serde_json::json;
use std::str::FromStr;

pub async fn handle_memory(paths: &CraftPaths, action: Option<MemoryCommands>) -> Result<()> {
    match action {
        None | Some(MemoryCommands::Status { json: false }) => handle_status(paths, false).await,
        Some(MemoryCommands::Status { json: true }) => handle_status(paths, true).await,
        Some(MemoryCommands::Compact { target_order, json }) => {
            handle_compact(paths, target_order, json).await
        }
        Some(MemoryCommands::Thp { mode, defrag, json }) => {
            handle_thp(paths, &mode, defrag.as_deref(), json).await
        }
        Some(MemoryCommands::Pool { packets, slice_size, json }) => {
            handle_pool(packets, slice_size, json).await
        }
    }
}

async fn handle_status(paths: &CraftPaths, json: bool) -> Result<()> {
    let (summary, pool_stats) = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            let s = match client.get_compaction_status().await {
                Ok(status) => status,
                Err(_) => {
                    let service = CompactionService::global(paths);
                    service.get_status()?
                }
            };
            let p = match client.get_compaction_pool_stats().await {
                Ok(stats) => stats,
                Err(_) => {
                    let service = CompactionService::global(paths);
                    service.get_pool_stats()?
                }
            };
            (s, p)
        }
        Err(_) => {
            let service = CompactionService::global(paths);
            (service.get_status()?, service.get_pool_stats()?)
        }
    };

    if json {
        let out = json!({
            "summary": summary,
            "pool_stats": pool_stats,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("[MEMORY] Autonomous Memory Compaction & Transparent Hugepage Status");
        println!("  THP Mode:                 {}", summary.thp_status.enabled);
        println!("  THP Defrag Mode:          {}", summary.thp_status.defrag);
        println!(
            "  Fragmentation Index:      {:.2}% (Threshold: {:.1}%)",
            summary.fragmentation_index * 100.0,
            summary.fragmentation_threshold * 100.0
        );
        println!(
            "  Compaction Needed:        {}",
            if summary.compaction_needed {
                "[WARN] Active Fragmentation - Compaction Recommended"
            } else {
                "[OK] Memory Coalesced"
            }
        );
        println!("  Total Compaction Cycles:  {}", summary.total_cycles_completed);
        println!("  Allocated Network Pages:  {}", summary.total_pages_allocated);
        println!(
            "  Zero-Alloc Recycled:      {} ({:.1}% efficiency)",
            summary.pages_recycled,
            summary.recycle_efficiency_percent
        );
        println!("  Page Pool Active Slots:   {}", pool_stats.active_pages);
        println!("  Page Pool Exhaustions:    {}", pool_stats.pool_exhaustions);
        println!("  THP Hugepages Allocated:  {}", summary.thp_status.hugepages_allocated);
        println!("  THP Hugepages Split:      {}", summary.thp_status.hugepages_split);

        if let Some(ref last) = summary.last_cycle {
            println!("\n[LAST COMPACTION CYCLE]");
            println!("  Cycle ID:                 {}", last.cycle_id);
            println!("  Status:                   [{}]", last.status);
            println!(
                "  Fragmentation:            {:.2}% -> {:.2}%",
                last.initial_fragmentation * 100.0,
                last.final_fragmentation * 100.0
            );
            println!("  Pages Migrated:           {}", last.pages_migrated);
            println!("  Hugepages Formed:         {} (2MB each)", last.hugepages_formed);
            println!("  Duration:                 {} ms", last.duration_ms);
        }
    }
    Ok(())
}

async fn handle_compact(paths: &CraftPaths, _target_order: usize, json: bool) -> Result<()> {
    let cycle = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.trigger_compaction().await {
            Ok(c) => c,
            Err(_) => {
                let service = CompactionService::global(paths);
                service.trigger_compaction()?
            }
        },
        Err(_) => {
            let service = CompactionService::global(paths);
            service.trigger_compaction()?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&cycle).unwrap());
    } else {
        println!("[COMPACTION] Proactive Memory Compaction Cycle Executed");
        println!("  Cycle ID:                 {}", cycle.cycle_id);
        println!("  Status:                   [{}]", cycle.status);
        println!(
            "  Fragmentation Before:     {:.2}%",
            cycle.initial_fragmentation * 100.0
        );
        println!(
            "  Fragmentation After:      {:.2}%",
            cycle.final_fragmentation * 100.0
        );
        println!("  Pages Migrated:           {}", cycle.pages_migrated);
        println!("  Pages Freed:              {}", cycle.pages_freed);
        println!("  Hugepages Formed:         {} (2MB each)", cycle.hugepages_formed);
        println!("  Duration:                 {} ms", cycle.duration_ms);
    }
    Ok(())
}

async fn handle_thp(
    paths: &CraftPaths,
    mode_str: &str,
    defrag_opt: Option<&str>,
    json: bool,
) -> Result<()> {
    let mode = ThpMode::from_str(mode_str)?;
    let defrag = match defrag_opt {
        Some(d) => ThpDefragMode::from_str(d)?,
        None => ThpDefragMode::Madvise,
    };

    let (status, msg) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.configure_thp(mode, defrag).await {
            Ok((s, m)) => (s, m),
            Err(_) => {
                let service = CompactionService::global(paths);
                let s = service.configure_thp(mode, defrag)?;
                (s, format!("Transparent hugepages configured to mode='{}', defrag='{}'", mode, defrag))
            }
        },
        Err(_) => {
            let service = CompactionService::global(paths);
            let s = service.configure_thp(mode, defrag)?;
            (s, format!("Transparent hugepages configured to mode='{}', defrag='{}'", mode, defrag))
        }
    };

    if json {
        let out = json!({
            "message": msg,
            "status": status,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("[THP] Transparent Hugepage Configuration Updated");
        println!("  Status:                   [OK]");
        println!("  Active Mode:              {}", status.enabled);
        println!("  Active Defrag Mode:       {}", status.defrag);
        println!("  Message:                  {}", msg);
    }
    Ok(())
}

async fn handle_pool(
    packets: usize,
    slice_size: usize,
    json: bool,
) -> Result<()> {
    let pool = SocketPagePool::new(PagePoolConfig::default(), 1024, slice_size);
    let result = pool.simulate_packet_pipeline(packets, slice_size);

    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("[PAGE_POOL] Zero-Allocation Network Page Pool Benchmark");
        println!("  Packets Processed:        {}", result.packets_processed);
        println!(
            "  Total Data Processed:     {} bytes ({:.2} MB)",
            result.total_bytes,
            result.total_bytes as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  Throughput:               {:.2} MB/s ({:.0} packets/sec)",
            result.throughput_mb_sec, result.packets_per_sec
        );
        println!(
            "  Zero-Alloc Reuse:         {:.1}%",
            result.fast_path_reuse_ratio * 100.0
        );
        println!(
            "  Duration:                 {:.2} ms",
            result.duration_micros as f64 / 1000.0
        );
        println!("  Buffer Exhaustions:       {}", result.exhaustion_events);
    }
    Ok(())
}
