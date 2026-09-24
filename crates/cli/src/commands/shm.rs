use crate::cli::ShmCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::shm::{ShmBenchmarkMetrics, ShmSegmentMeta, ShmStatusSummary};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::ShmService;
use serde_json::json;

pub async fn handle_shm(paths: &CraftPaths, action: Option<ShmCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(ShmCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(ShmCommands::Create {
            server,
            channel,
            slot_size,
            slots,
            json,
        }) => handle_create(paths, server, channel, slot_size, slots, json).await,
        Some(ShmCommands::Close {
            server,
            channel,
            json,
        }) => handle_close(paths, server, channel, json).await,
        Some(ShmCommands::Bench {
            messages,
            size,
            json,
        }) => handle_bench(paths, messages, size, json).await,
        Some(ShmCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(paths: &CraftPaths, server: Option<String>) -> Result<ShmStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_shm_status(server.clone()).await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = ShmService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = ShmService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, server).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("=== Autonomous POSIX Shared Memory & Ring Bus Status ===");
        println!("  Active Segments:         {}", summary.active_segments);
        println!(
            "  Total Allocated Memory:  {} bytes ({:.2} MB)",
            summary.total_allocated_bytes,
            summary.total_allocated_bytes as f64 / 1_048_576.0
        );
        println!("  Messages Written Total:  {}", summary.messages_written_total);
        println!("  Messages Read Total:     {}", summary.messages_read_total);
        println!("  Avg IPC Latency:         {:.2} ns", summary.avg_latency_ns);
        println!("  Watchdog Reclaimed:      {}", summary.watchdog_reclaimed_segments);

        if !summary.segments.is_empty() {
            println!();
            println!("--- Registered Ring Buffer Segments ---");
            println!(
                "{:<16} {:<16} {:>6} {:>10} {:>12} {:>9} {:>12}",
                "Server", "Channel", "Slots", "Slot Size", "Capacity", "Prod/Cons", "Msgs Written"
            );
            for seg in &summary.segments {
                println!(
                    "{:<16} {:<16} {:>6} {:>9}B {:>11}B {:>4}/{:<4} {:>12}",
                    seg.server,
                    seg.channel_type.as_str(),
                    seg.slot_count,
                    seg.slot_size,
                    seg.capacity_bytes,
                    seg.active_producers,
                    seg.active_consumers,
                    seg.messages_written
                );
            }
        }
    }

    Ok(())
}

async fn handle_create(
    paths: &CraftPaths,
    server: String,
    channel: String,
    slot_size: usize,
    slots: usize,
    json: bool,
) -> Result<()> {
    let meta: ShmSegmentMeta = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .create_shm_channel(server.clone(), channel.clone(), slot_size, slots)
                .await
            {
                Ok(m) => m,
                Err(_) => {
                    let service = ShmService::global(paths);
                    service.create_channel(&server, &channel, slot_size, slots)?
                }
            }
        }
        Err(_) => {
            let service = ShmService::global(paths);
            service.create_channel(&server, &channel, slot_size, slots)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&meta).unwrap());
    } else {
        println!("[OK] Shared memory ring buffer channel created successfully");
        println!("  Server:            {}", meta.server);
        println!("  Channel Type:      {}", meta.channel_type.as_str());
        println!("  Segment Name:      {}", meta.name);
        println!("  Slot Count:        {}", meta.slot_count);
        println!("  Slot Size:         {} bytes", meta.slot_size);
        println!("  Total Mmap Size:   {} bytes", meta.capacity_bytes);
    }

    Ok(())
}

async fn handle_close(
    paths: &CraftPaths,
    server: String,
    channel: String,
    json: bool,
) -> Result<()> {
    let closed = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .close_shm_channel(server.clone(), channel.clone())
                .await
            {
                Ok(c) => c,
                Err(_) => {
                    let service = ShmService::global(paths);
                    service.close_channel(&server, &channel)?
                }
            }
        }
        Err(_) => {
            let service = ShmService::global(paths);
            service.close_channel(&server, &channel)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "closed": closed,
                "server": server,
                "channel": channel
            })
        );
    } else if closed {
        println!(
            "[OK] Shared memory segment for server '{}', channel '{}' closed and unlinked",
            server, channel
        );
    } else {
        println!(
            "[WARN] Shared memory segment for server '{}', channel '{}' was not found or already closed",
            server, channel
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    messages: usize,
    size: usize,
    json: bool,
) -> Result<()> {
    let report: ShmBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_shm_bench(messages, size).await {
            Ok(r) => r,
            Err(_) => {
                let service = ShmService::global(paths);
                service.run_bench(messages, size)?
            }
        },
        Err(_) => {
            let service = ShmService::global(paths);
            service.run_bench(messages, size)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("=== High-Speed POSIX Shared Memory Zero-Copy Benchmark ===");
        println!("  Messages Streamed:       {}", report.message_count);
        println!("  Payload Size:            {} bytes", report.payload_size);
        println!("  Duration:                {:.2} ms", report.elapsed_ms);
        println!("  Throughput:              {:.1} msgs/sec", report.throughput_msgs_per_sec);
        println!("  Bandwidth:               {:.2} MB/s", report.bandwidth_mb_per_sec);
        println!("  Average Latency:         {:.2} ns/msg", report.avg_latency_ns);
        println!("  P99 Latency:             {:.2} ns/msg", report.p99_latency_ns);
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_shm_metrics(server.clone()).await {
            Ok(msg) => msg,
            Err(_) => {
                let service = ShmService::global(paths);
                service.reset_metrics(server.as_deref())?;
                "Shared memory metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = ShmService::global(paths);
            service.reset_metrics(server.as_deref())?;
            "Shared memory metrics reset successfully".to_string()
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "message": message,
                "server": server
            })
        );
    } else {
        println!("[OK] Shared memory cumulative throughput metrics reset and stale leases cleared");
    }

    Ok(())
}
