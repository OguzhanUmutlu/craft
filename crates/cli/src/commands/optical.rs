// crates/cli/src/commands/optical.rs
//
// CLI command handler for Autonomous Optical Network Switching,
// Photonic Interconnects & Line-Rate Nanosecond Waveguide Routing.
// Strictly zero emojis.

use std::str::FromStr;

use crate::cli::OpticalCommands;
use craft_core::error::{CraftError, Result};
use craft_core::optical::{
    render_optical_bench_table, render_optical_circuits_table, render_optical_status_table,
    OpticalBenchmarkMetrics, OpticalCircuit, OpticalRoutingMode, OpticalStatusSummary,
};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::OpticalSwitchService;
use serde_json::json;

pub async fn handle_optical(
    paths: &CraftPaths,
    action: Option<OpticalCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(OpticalCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(OpticalCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(OpticalCommands::CircuitAdd {
            server,
            circuit_id,
            ingress,
            egress,
            channel,
            json,
        }) => handle_circuit_add(paths, server, circuit_id, ingress, egress, channel, json).await,
        Some(OpticalCommands::CircuitRm { circuit_id, json }) => {
            handle_circuit_rm(paths, circuit_id, json).await
        }
        Some(OpticalCommands::Circuits { json }) => {
            handle_circuits(paths, json).await
        }
        Some(OpticalCommands::Bench {
            frames,
            dimension,
            json,
        }) => handle_bench(paths, frames, dimension, json).await,
        Some(OpticalCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<OpticalStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_optical_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = OpticalSwitchService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
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
        println!("{}", render_optical_status_table(&summary));
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
        Ok(mut client) => match client.set_optical_mode(Some(&server), &mode).await {
            Ok(res) => res,
            Err(_) => {
                let service = OpticalSwitchService::global(paths);
                let routing_mode = OpticalRoutingMode::from_str(&mode)?;
                service.set_mode(routing_mode, Some(&server))?;
                format!("Optical routing mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            let routing_mode = OpticalRoutingMode::from_str(&mode)?;
            service.set_mode(routing_mode, Some(&server))?;
            format!("Optical routing mode updated to {}", mode)
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

async fn handle_circuit_add(
    paths: &CraftPaths,
    server: String,
    circuit_id: String,
    ingress: u32,
    egress: u32,
    channel: u16,
    json: bool,
) -> Result<()> {
    let circuit = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .create_optical_circuit(&circuit_id, ingress as u16, egress as u16, channel, Some(&server))
                .await
            {
                Ok(c) => c,
                Err(_) => {
                    let service = OpticalSwitchService::global(paths);
                    service.create_circuit(circuit_id.clone(), ingress as u16, egress as u16, channel, Some(server.clone()))?
                }
            }
        }
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            service.create_circuit(circuit_id.clone(), ingress as u16, egress as u16, channel, Some(server.clone()))?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "circuit": circuit,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Optical circuit '{}' provisioned successfully on port {} -> port {} (ch {}, bandwidth: {:.1} Gbps)",
            circuit.circuit_id,
            circuit.ingress_port,
            circuit.egress_port,
            circuit.wavelength_ch,
            circuit.bandwidth_gbps
        );
    }

    Ok(())
}

async fn handle_circuit_rm(
    paths: &CraftPaths,
    circuit_id: String,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.delete_optical_circuit(&circuit_id).await {
            Ok(res) => res,
            Err(_) => {
                let service = OpticalSwitchService::global(paths);
                service.delete_circuit(&circuit_id)?
            }
        },
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            service.delete_circuit(&circuit_id)?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "circuit_id": circuit_id,
            "torn_down": success,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else if success {
        println!("[OK] Optical circuit '{}' torn down successfully", circuit_id);
    } else {
        println!("[WARN] Optical circuit '{}' was not found or already inactive", circuit_id);
    }

    Ok(())
}

async fn handle_circuits(paths: &CraftPaths, json: bool) -> Result<()> {
    let circuits: Vec<OpticalCircuit> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_optical_circuits(None).await {
            Ok(c) => c,
            Err(_) => {
                let service = OpticalSwitchService::global(paths);
                service.list_circuits(None)?
            }
        },
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            service.list_circuits(None)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&circuits)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_optical_circuits_table(&circuits));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    frames: usize,
    dimension: usize,
    json: bool,
) -> Result<()> {
    let metrics: OpticalBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .run_optical_bench(frames as u32, dimension as u16)
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = OpticalSwitchService::global(paths);
                    service.run_bench(frames as u32, dimension as u16)?
                }
            }
        }
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            service.run_bench(frames as u32, dimension as u16)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_optical_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_optical_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = OpticalSwitchService::global(paths);
                service.reset_metrics()?
            }
        },
        Err(_) => {
            let service = OpticalSwitchService::global(paths);
            service.reset_metrics()?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "Optical switching telemetry counters and error stats reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] Optical switching telemetry counters and error stats reset successfully");
    }

    Ok(())
}
