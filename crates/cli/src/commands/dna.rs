// crates/cli/src/commands/dna.rs
//
// CLI command handler for Autonomous Bio-Molecular DNA State Archival,
// Cold-Storage Base-4 Encoding & Century-Scale World Preservation.
// Strictly zero emojis.

use std::str::FromStr;

use crate::cli::DnaCommands;
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_core::dna::{
    render_dna_archive_table, render_dna_bench_table, render_dna_status_table, DnaArchiveMode,
    DnaBenchmarkMetrics, DnaChunkArchiveDescriptor, DnaDecayModel, DnaStatusSummary,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::DnaArchiveService;
use serde_json::json;

pub async fn handle_dna(
    paths: &CraftPaths,
    action: Option<DnaCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(DnaCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(DnaCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(DnaCommands::Encode {
            chunk_x,
            chunk_z,
            dimension,
            density_target,
            server,
            json,
        }) => {
            handle_encode(
                paths,
                chunk_x,
                chunk_z,
                dimension,
                density_target,
                server,
                json,
            )
            .await
        }
        Some(DnaCommands::Decode {
            oligo_id,
            nanopore,
            server,
            json,
        }) => handle_decode(paths, oligo_id, nanopore, server, json).await,
        Some(DnaCommands::Decay {
            years,
            model,
            server,
            json,
        }) => handle_decay(paths, years, model, server, json).await,
        Some(DnaCommands::Bench {
            chunks,
            oligos,
            json,
        }) => handle_bench(paths, chunks, oligos, json).await,
        Some(DnaCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<DnaStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_dna_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = DnaArchiveService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, server).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_dna_status_table(&summary));
    }

    Ok(())
}

async fn handle_mode(
    paths: &CraftPaths,
    server: String,
    mode: String,
    json: bool,
) -> Result<()> {
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.set_dna_mode(&mode, Some(&server)).await {
            Ok(res) => res,
            Err(_) => {
                let service = DnaArchiveService::global(paths);
                let archive_mode = DnaArchiveMode::from_str(&mode)?;
                service.set_mode(archive_mode, Some(&server))?;
                format!("DNA archive mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            let archive_mode = DnaArchiveMode::from_str(&mode)?;
            service.set_mode(archive_mode, Some(&server))?;
            format!("DNA archive mode updated to {}", mode)
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "server": server,
            "mode": mode,
            "message": message,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] {}", message);
    }

    Ok(())
}

async fn handle_encode(
    paths: &CraftPaths,
    chunk_x: i32,
    chunk_z: i32,
    dimension: String,
    _density_target: u32,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    // Generate deterministic chunk payload block data
    let mut raw_data = vec![0u8; 512];
    for (i, b) in raw_data.iter_mut().enumerate() {
        *b = ((chunk_x as usize + i * 31) ^ (chunk_z as usize + i * 17)) as u8;
    }

    let descriptor: DnaChunkArchiveDescriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .encode_dna_chunk(
                    chunk_x,
                    chunk_z,
                    &dimension,
                    raw_data.clone(),
                    server.as_deref(),
                )
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = DnaArchiveService::global(paths);
                    service.encode_chunk(
                        chunk_x,
                        chunk_z,
                        &dimension,
                        &raw_data,
                        server.as_deref(),
                    )?
                }
            }
        }
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.encode_chunk(
                chunk_x,
                chunk_z,
                &dimension,
                &raw_data,
                server.as_deref(),
            )?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&descriptor)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_dna_archive_table(&descriptor));
    }

    Ok(())
}

async fn handle_decode(
    paths: &CraftPaths,
    oligo_id: u32,
    simulate_nanopore: bool,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let raw_bytes: Vec<u8> = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .decode_dna_oligo(oligo_id, server.as_deref())
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = DnaArchiveService::global(paths);
                    service.decode_oligo(oligo_id, server.as_deref())?
                }
            }
        }
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.decode_oligo(oligo_id, server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "oligo_id": oligo_id,
            "recovered_bytes_len": raw_bytes.len(),
            "simulate_nanopore": simulate_nanopore,
            "sample_hex": hex::encode(&raw_bytes[..raw_bytes.len().min(32)]),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Oligo #{} decoded: {} bytes recovered (nanopore={}), sample hex: {}",
            oligo_id,
            raw_bytes.len(),
            simulate_nanopore,
            hex::encode(&raw_bytes[..raw_bytes.len().min(16)])
        );
    }

    Ok(())
}

async fn handle_decay(
    paths: &CraftPaths,
    years: u64,
    model: String,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let decay_model = DnaDecayModel::from_str(&model)?;
    let summary: DnaStatusSummary = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .simulate_dna_decay(years, &model, server.as_deref())
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = DnaArchiveService::global(paths);
                    service.simulate_decay(years, decay_model, server.as_deref())?
                }
            }
        }
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.simulate_decay(years, decay_model, server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Simulated chemical decay over {} years ({:?}): {} oligos remaining, mean GC {:.2}%, half-life {:.1} years",
            years,
            summary.decay_model,
            summary.total_oligos,
            summary.mean_gc_ratio * 100.0,
            summary.decay_model.half_life_years()
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    chunks: usize,
    oligos: usize,
    json: bool,
) -> Result<()> {
    let metrics: DnaBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_dna_bench(Some(chunks), Some(oligos)).await {
            Ok(res) => res,
            Err(_) => {
                let service = DnaArchiveService::global(paths);
                service.run_bench(chunks, oligos)?
            }
        },
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.run_bench(chunks, oligos)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_dna_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_dna_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = DnaArchiveService::global(paths);
                service.reset_metrics()?
            }
        },
        Err(_) => {
            let service = DnaArchiveService::global(paths);
            service.reset_metrics()?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "Bio-molecular DNA archival telemetry counters reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] Bio-molecular DNA archival telemetry counters reset successfully");
    }

    Ok(())
}
