use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    AnvilChunkCache, AnvilIoEngine, AnvilRegistry, AnvilStatusSummary, CraftError,
    CraftPaths, RegionFileReader, Result,
};
use craft_daemon::DaemonClient;
use std::path::{Path, PathBuf};

use crate::cli::AnvilCommands;

pub async fn handle_anvil(action: AnvilCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        AnvilCommands::Status { json } => status(json, paths).await,
        AnvilCommands::Inspect { server, file, json } => inspect(server, file, json, paths).await,
        AnvilCommands::Prefetch {
            server,
            world,
            x,
            z,
            radius,
            json,
        } => prefetch(server, world, x, z, radius, json, paths).await,
        AnvilCommands::Bench { chunks, json } => bench(chunks, json, paths).await,
        AnvilCommands::Config {
            enabled,
            engine,
            cache_mb,
            prefetch_radius,
            batch_size,
            json,
        } => config(enabled, engine, cache_mb, prefetch_radius, batch_size, json, paths).await,
    }
}

async fn status(json: bool, paths: &CraftPaths) -> Result<()> {
    // Attempt connecting to daemon
    let status_res = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.get_anvil_status().await.ok()
    } else {
        None
    };

    let summary = match status_res {
        Some(s) => s,
        None => {
            // Offline fallback
            let registry = AnvilRegistry::load(paths).unwrap_or_default();
            let engine = AnvilIoEngine::auto_detect();
            AnvilStatusSummary {
                engine: engine.engine_type().as_str().to_string(),
                active_cached_chunks: 0,
                cache_memory_used_bytes: 0,
                cache_memory_limit_bytes: registry.config.cache_max_bytes,
                cache_hit_count: 0,
                cache_miss_count: 0,
                cache_hit_ratio: 0.0,
                cache_eviction_count: 0,
                cache_prefetch_count: 0,
                total_io_ops: 0,
                total_bytes_read: 0,
                total_bytes_written: 0,
                context_switch_savings: 0,
                avg_io_latency_micros: 0.0,
            }
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!();
    println!("  HARDWARE-ACCELERATED ANVIL STORAGE ENGINE (MCA)");
    println!("  ==============================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Green),
        ]);

    let engine_display = if summary.engine == "io_uring" {
        "Linux io_uring (Zero-Copy Ring Pipeline)".to_string()
    } else {
        "Threaded Positional Fallback (preadv2/pwritev2)".to_string()
    };

    let mem_used_mb = (summary.cache_memory_used_bytes as f64) / (1024.0 * 1024.0);
    let mem_limit_mb = (summary.cache_memory_limit_bytes as f64) / (1024.0 * 1024.0);
    let r_mb = (summary.total_bytes_read as f64) / (1024.0 * 1024.0);
    let w_mb = (summary.total_bytes_written as f64) / (1024.0 * 1024.0);

    table.add_row(Row::from(vec![
        Cell::new("I/O Engine Type"),
        Cell::new(&engine_display).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Active Cached Chunks"),
        Cell::new(&summary.active_cached_chunks.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Cache Memory Usage"),
        Cell::new(&format!("{:.2} MB / {:.2} MB", mem_used_mb, mem_limit_mb)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Cache Hit Ratio"),
        Cell::new(&format!(
            "{:.2}% ({} hits / {} misses)",
            summary.cache_hit_ratio * 100.0,
            summary.cache_hit_count,
            summary.cache_miss_count
        )),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Evictions / Prefetches"),
        Cell::new(&format!(
            "{} evicted / {} prefetched",
            summary.cache_eviction_count, summary.cache_prefetch_count
        )),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Total I/O Operations"),
        Cell::new(&summary.total_io_ops.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Throughput Processed"),
        Cell::new(&format!("{:.2} MB read / {:.2} MB written", r_mb, w_mb)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Context Switch Savings"),
        Cell::new(&format!(
            "{} syscall context switches saved",
            summary.context_switch_savings
        ))
        .fg(Color::Cyan),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Average I/O Latency"),
        Cell::new(&format!("{:.2} µs", summary.avg_io_latency_micros)),
    ]));

    println!("{table}");
    println!();
    Ok(())
}

async fn inspect(
    server: Option<String>,
    file: String,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = if let Some(s) = server.as_deref() {
        paths.resolve_server_path(None, Some(s), true)?
    } else {
        std::env::current_dir().map_err(CraftError::Io)?
    };

    // Try daemon first
    let details = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .inspect_region(server_path.clone(), file.clone())
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        // Offline inspection
        let mca_path = resolve_cli_region_path(&server_path, &file)?;
        let reader = RegionFileReader::open(&mca_path)?;
        let inspection = reader.inspect()?;
        let mut chunks = Vec::new();
        let header = reader.header();

        for cz in 0..32 {
            for cx in 0..32 {
                let loc = header.get_location(cx, cz);
                if !loc.is_empty() {
                    let ts = header.get_timestamp(cx, cz);
                    let size_bytes = (loc.sector_count as u32) * 4096;
                    chunks.push(craft_core::ChunkSectorInfo {
                        chunk_x: (inspection.region_x * 32) + cx,
                        chunk_z: (inspection.region_z * 32) + cz,
                        sector_offset: loc.offset,
                        sector_count: loc.sector_count,
                        timestamp: ts,
                        size_bytes,
                    });
                }
            }
        }

        craft_core::RegionDetails {
            file_name: mca_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&file)
                .to_string(),
            file_path: mca_path.display().to_string(),
            region_x: inspection.region_x,
            region_z: inspection.region_z,
            file_size_bytes: inspection.file_size_bytes,
            allocated_sectors: inspection.allocated_payload_sectors,
            total_sectors: inspection.total_sectors,
            active_chunks: inspection.active_chunks,
            empty_chunks: inspection.empty_chunks,
            fragmentation_ratio: inspection.fragmentation_ratio,
            largest_contiguous_free_sectors: inspection.largest_contiguous_free_sectors,
            chunks,
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&details)?);
        return Ok(());
    }

    println!();
    println!("  ANVIL REGION FILE INSPECTION: {}", details.file_name);
    println!("  ==================================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Property").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Green),
        ]);

    let size_kb = (details.file_size_bytes as f64) / 1024.0;
    table.add_row(Row::from(vec![
        Cell::new("Region Coordinates"),
        Cell::new(&format!("({}, {})", details.region_x, details.region_z)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Full File Path"),
        Cell::new(&details.file_path),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("File Size"),
        Cell::new(&format!("{:.2} KB ({} bytes)", size_kb, details.file_size_bytes)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Active / Empty Chunks"),
        Cell::new(&format!("{} active / {} empty (total 1024)", details.active_chunks, details.empty_chunks)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Total / Allocated Sectors"),
        Cell::new(&format!("{} total / {} payload sectors", details.total_sectors, details.allocated_sectors)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Fragmentation Ratio"),
        Cell::new(&format!("{:.2}%", details.fragmentation_ratio * 100.0)).fg(
            if details.fragmentation_ratio > 0.3 { Color::Red } else { Color::Green },
        ),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Largest Contiguous Free Run"),
        Cell::new(&format!("{} sectors", details.largest_contiguous_free_sectors)),
    ]));

    println!("{table}");

    // Show chunk sample table if chunks exist
    if !details.chunks.is_empty() {
        println!();
        println!("  SAMPLE ACTIVE CHUNK SECTORS (Showing first 10 chunks):");
        println!();

        let mut chunk_table = Table::new();
        chunk_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Chunk (X, Z)").fg(Color::Cyan),
                Cell::new("Sector Offset").fg(Color::Yellow),
                Cell::new("Sectors").fg(Color::Green),
                Cell::new("Size (Bytes)").fg(Color::White),
                Cell::new("Timestamp").fg(Color::DarkGrey),
            ]);

        for c in details.chunks.iter().take(10) {
            chunk_table.add_row(Row::from(vec![
                Cell::new(&format!("({}, {})", c.chunk_x, c.chunk_z)),
                Cell::new(&c.sector_offset.to_string()),
                Cell::new(&c.sector_count.to_string()),
                Cell::new(&c.size_bytes.to_string()),
                Cell::new(&c.timestamp.to_string()),
            ]));
        }

        println!("{chunk_table}");
    }

    println!();
    Ok(())
}

async fn prefetch(
    server: String,
    world: Option<String>,
    x: i32,
    z: i32,
    radius: u32,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(&server), true)?;
    let world_name = world.unwrap_or_else(|| "world".to_string());

    let summary = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .prefetch_chunks(server_path.clone(), world_name.clone(), x, z, radius)
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        // Offline fallback
        let region_dir = server_path.join(&world_name).join("region");
        let io_engine = AnvilIoEngine::auto_detect();
        let cache = AnvilChunkCache::new(64 * 1024 * 1024, 4096);
        cache.prefetch_radius(&world_name, &region_dir, x, z, radius, &io_engine)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!();
    println!("  CHUNK DIRECT-MEMORY PREFETCH COMPLETED");
    println!("  =====================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Parameter").fg(Color::Cyan),
            Cell::new("Result").fg(Color::Green),
        ]);

    let kb_loaded = (summary.bytes_loaded as f64) / 1024.0;
    table.add_row(Row::from(vec![
        Cell::new("Center Chunk Coordinates"),
        Cell::new(&format!("({}, {})", summary.center_x, summary.center_z)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Prefetch Radius"),
        Cell::new(&format!("{} chunks ({}x{} grid)", summary.radius, summary.radius * 2 + 1, summary.radius * 2 + 1)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Total Grid Candidates"),
        Cell::new(&summary.total_candidates.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Already Cached (Hits)"),
        Cell::new(&summary.already_cached.to_string()).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Chunks Loaded from Disk"),
        Cell::new(&summary.chunks_loaded.to_string()).fg(Color::Green),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Load Failures / Missing"),
        Cell::new(&summary.load_failures.to_string()).fg(if summary.load_failures > 0 { Color::Red } else { Color::Green }),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Data Loaded"),
        Cell::new(&format!("{:.2} KB", kb_loaded)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Pipeline Execution Time"),
        Cell::new(&format!("{} ms", summary.elapsed_millis)),
    ]));

    println!("{table}");
    println!();
    Ok(())
}

async fn bench(chunks: usize, json: bool, paths: &CraftPaths) -> Result<()> {
    let report = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .benchmark_anvil(chunks)
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        let service = craft_daemon::AnvilService::new(paths.clone());
        service.benchmark(chunks)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!();
    println!("  ANVIL STORAGE ENGINE BENCHMARK REPORT");
    println!("  ====================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Benchmark Metric").fg(Color::Cyan),
            Cell::new("Performance Score").fg(Color::Green),
        ]);

    table.add_row(Row::from(vec![
        Cell::new("Chunks Tested"),
        Cell::new(&report.chunks_tested.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Engine Used"),
        Cell::new(&report.engine_used).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Write Throughput"),
        Cell::new(&format!("{:.2} MB/s ({} ms total)", report.write_throughput_mb_sec, report.write_time_ms)).fg(Color::Green),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Read Throughput"),
        Cell::new(&format!("{:.2} MB/s ({} ms total)", report.read_throughput_mb_sec, report.read_time_ms)).fg(Color::Green),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Compression Ratio"),
        Cell::new(&format!("{:.2}x", report.compression_ratio)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Context Switch Savings"),
        Cell::new(&format!("{} switches saved", report.context_switch_savings)).fg(Color::Cyan),
    ]));

    println!("{table}");
    println!();
    Ok(())
}

async fn config(
    enabled: Option<bool>,
    engine: Option<String>,
    cache_mb: Option<usize>,
    prefetch_radius: Option<u32>,
    batch_size: Option<usize>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let mut reg = AnvilRegistry::load(paths).unwrap_or_default();
    let mut modified = false;

    if let Some(en) = enabled {
        reg.config.enabled = en;
        modified = true;
    }
    if let Some(eng) = engine {
        let valid_eng = match eng.to_lowercase().as_str() {
            "io_uring" | "uring" => "io_uring".to_string(),
            _ => "threaded_fallback".to_string(),
        };
        reg.config.engine = valid_eng;
        modified = true;
    }
    if let Some(mb) = cache_mb {
        reg.config.cache_max_bytes = mb * 1024 * 1024;
        modified = true;
    }
    if let Some(r) = prefetch_radius {
        reg.config.prefetch_radius = r.clamp(1, 16);
        modified = true;
    }
    if let Some(b) = batch_size {
        reg.config.batch_size = b.clamp(1, 128);
        modified = true;
    }

    if modified {
        reg.save(paths)?;
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            let _ = client.set_anvil_config(reg.config.clone()).await;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&reg.config)?);
        return Ok(());
    }

    println!();
    println!("  ANVIL STORAGE ENGINE CONFIGURATION");
    println!("  =================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Parameter").fg(Color::Cyan),
            Cell::new("Current Setting").fg(Color::Green),
        ]);

    let cache_mb_val = reg.config.cache_max_bytes / (1024 * 1024);
    table.add_row(Row::from(vec![
        Cell::new("Enabled"),
        Cell::new(&reg.config.enabled.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Preferred Engine"),
        Cell::new(&reg.config.engine).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Cache Memory Limit"),
        Cell::new(&format!("{} MB", cache_mb_val)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Cache Chunk Capacity"),
        Cell::new(&reg.config.cache_max_chunks.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Default Prefetch Radius"),
        Cell::new(&format!("{} chunks", reg.config.prefetch_radius)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("I/O Batch Size"),
        Cell::new(&reg.config.batch_size.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Direct NVMe DMA Simulation"),
        Cell::new(&reg.config.direct_dma.to_string()),
    ]));

    println!("{table}");
    println!();
    Ok(())
}

fn resolve_cli_region_path(server_path: &Path, file: &str) -> Result<PathBuf> {
    let p = PathBuf::from(file);
    if p.is_file() {
        return Ok(p);
    }
    let server_rel = server_path.join(file);
    if server_rel.is_file() {
        return Ok(server_rel);
    }
    let world_region = server_path.join("world").join("region").join(file);
    if world_region.is_file() {
        return Ok(world_region);
    }
    Err(CraftError::Other(format!(
        "Region file '{file}' not found in '{}'",
        server_path.display()
    )))
}
