// crates/cli/src/commands/cpo.rs
//
// CLI command handler for Autonomous Silicon Photonic Co-Packaged Optics (CPO),
// Optical Neural Matrix Multiply & Sub-Nanosecond Direct Die Interconnects.
// Strictly zero emojis.

use std::str::FromStr;

use crate::cli::CpoCommands;
use craft_core::cpo::{
    render_cpo_bench_table, render_cpo_status_table, render_cpo_tiles_table, CpoBenchmarkMetrics,
    CpoMode, CpoStatusSummary, CpoThermalServoState, CpoTileDescriptor,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_daemon::cpo_service::CpoService;
use craft_daemon::ipc::DaemonClient;
use serde_json::json;

pub async fn handle_cpo(
    paths: &CraftPaths,
    action: Option<CpoCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(CpoCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(CpoCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(CpoCommands::Mvm {
            vector,
            server,
            json,
        }) => handle_mvm(paths, vector, server, json).await,
        Some(CpoCommands::Thermal { temp, server, json }) => {
            handle_thermal(paths, temp, server, json).await
        }
        Some(CpoCommands::Tiles { server, json }) => {
            handle_tiles(paths, server, json).await
        }
        Some(CpoCommands::Bench {
            iterations,
            dim,
            json,
        }) => handle_bench(paths, iterations, dim, json).await,
        Some(CpoCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<CpoStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_cpo_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = CpoService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
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
        println!("{}", render_cpo_status_table(&summary));
    }

    Ok(())
}

async fn handle_mode(
    paths: &CraftPaths,
    server: Option<String>,
    mode: String,
    json: bool,
) -> Result<()> {
    let parsed_mode = CpoMode::from_str(&mode)?;
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.set_cpo_mode(&mode, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.set_mode(parsed_mode, server.as_deref())?;
                format!("CPO operational mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.set_mode(parsed_mode, server.as_deref())?;
            format!("CPO operational mode updated to {}", mode)
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

async fn handle_mvm(
    paths: &CraftPaths,
    vector: Vec<f32>,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let (output_vec, mac_ops, latency_ps, energy_pj) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.execute_cpo_mvm(vector.clone(), server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.execute_mvm(&vector, server.as_deref())?
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.execute_mvm(&vector, server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "input_dimension": vector.len(),
            "output_vector": output_vec,
            "mac_operations": mac_ops,
            "latency_picoseconds": latency_ps,
            "energy_picojoules": energy_pj,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Photonic MVM executed: [{}] -> {} MAC ops in {:.1} ps ({:.3} pJ)",
            output_vec
                .iter()
                .map(|v| format!("{:.3}", v))
                .collect::<Vec<_>>()
                .join(", "),
            mac_ops,
            latency_ps,
            energy_pj
        );
    }

    Ok(())
}

async fn handle_thermal(
    paths: &CraftPaths,
    temp: f64,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let servo_state: CpoThermalServoState = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.adjust_cpo_thermal(temp, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.adjust_thermal(temp, server.as_deref())?
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.adjust_thermal(temp, server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&servo_state)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Micro-ring thermal servo stabilized: {:.2} deg C (status: {}, drift: {:.4} nm)",
            servo_state.substrate_temp_c, servo_state.thermal_status, servo_state.drift_nm
        );
    }

    Ok(())
}

async fn handle_tiles(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let tiles: Vec<CpoTileDescriptor> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_cpo_tiles(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.list_tiles(server.as_deref())?
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.list_tiles(server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&tiles)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_cpo_tiles_table(&tiles));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    dim: usize,
    json: bool,
) -> Result<()> {
    let metrics: CpoBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_cpo_bench(Some(iterations), Some(dim)).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.run_bench(iterations, dim, None)?
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.run_bench(iterations, dim, None)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_cpo_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_cpo_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CpoService::global(paths);
                service.reset_metrics(server.as_deref())?
            }
        },
        Err(_) => {
            let service = CpoService::global(paths);
            service.reset_metrics(server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "Silicon photonic CPO telemetry counters reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] Silicon photonic CPO telemetry counters reset successfully");
    }

    Ok(())
}
