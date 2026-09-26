// crates/cli/src/commands/cryo.rs
//
// CLI command handler for Autonomous Zero-Point Vacuum Energy Harvesting,
// Thermoelectric Cluster Power Balancing & Sub-Kelvin Cryogenic Cooling.
// Strictly zero emojis.

use std::str::FromStr;

use crate::cli::CryoCommands;
use craft_core::cryo::{
    render_cryo_bench_table, render_cryo_power_table, render_cryo_status_table,
    render_cryo_zones_table, CryoBenchmarkMetrics, CryoMode, CryoStatusSummary,
    CryoZoneDescriptor,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_daemon::cryo_service::CryoService;
use craft_daemon::ipc::DaemonClient;
use serde_json::json;

pub async fn handle_cryo(
    paths: &CraftPaths,
    action: Option<CryoCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(CryoCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(CryoCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(CryoCommands::Balance { zone, server, json }) => {
            handle_balance(paths, zone, server, json).await
        }
        Some(CryoCommands::Harvest {
            zone,
            duration,
            server,
            json,
        }) => handle_harvest(paths, zone, duration, server, json).await,
        Some(CryoCommands::Zones { server, json }) => {
            handle_zones(paths, server, json).await
        }
        Some(CryoCommands::Bench { iterations, json }) => {
            handle_bench(paths, iterations, json).await
        }
        Some(CryoCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<CryoStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_cryo_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = CryoService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
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
        println!("{}", render_cryo_status_table(&summary));
    }

    Ok(())
}

async fn handle_mode(
    paths: &CraftPaths,
    server: Option<String>,
    mode: String,
    json: bool,
) -> Result<()> {
    let parsed_mode = CryoMode::from_str(&mode)?;
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.set_cryo_mode(&mode, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.set_mode(parsed_mode, server.as_deref())?;
                format!("Cryogenic operational mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.set_mode(parsed_mode, server.as_deref())?;
            format!("Cryogenic operational mode updated to {}", mode)
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

async fn handle_balance(
    paths: &CraftPaths,
    _zone: Option<String>,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let (zone_count, mean_temp, quenches_averted) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.balance_cryo_zones(None, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.balance_zones(None, server.as_deref())?
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.balance_zones(None, server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "zones_balanced": zone_count,
            "mean_mixing_chamber_mk": mean_temp,
            "quenches_averted": quenches_averted,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Cryogenic zones balanced: {} zones active, mean temperature {:.2} mK, {} quenches averted",
            zone_count, mean_temp, quenches_averted
        );
    }

    Ok(())
}

async fn handle_harvest(
    paths: &CraftPaths,
    zone: String,
    duration: u64,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let (zpe_uw, teg_w, total_j) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.harvest_cryo_zero_point(Some(zone.clone()), server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.harvest_zero_point(Some(zone.clone()), server.as_deref())?
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.harvest_zero_point(Some(zone.clone()), server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "zone": zone,
            "duration_ms": duration,
            "zero_point_power_uw": zpe_uw,
            "thermoelectric_power_w": teg_w,
            "total_energy_recovered_j": total_j,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Harvested {:.2} uW zero-point energy & {:.2} W thermoelectric power (total {:.4} J recovered)",
            zpe_uw, teg_w, total_j
        );
        let service = CryoService::global(paths);
        let cavities = service.list_cavities().unwrap_or_default();
        let tegs = service.list_thermoelectrics().unwrap_or_default();
        if !cavities.is_empty() || !tegs.is_empty() {
            println!("{}", render_cryo_power_table(&cavities, &tegs));
        }
    }

    Ok(())
}

async fn handle_zones(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let zones: Vec<CryoZoneDescriptor> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_cryo_zones(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.list_zones(server.as_deref())?
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.list_zones(server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&zones)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_cryo_zones_table(&zones));
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    json: bool,
) -> Result<()> {
    let metrics: CryoBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_cryo_bench(Some(4), Some(iterations as u64), None).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.run_bench(Some(4), Some(iterations as u64), None)?
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.run_bench(Some(4), Some(iterations as u64), None)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_cryo_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_cryo_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = CryoService::global(paths);
                service.reset_metrics(server.as_deref())?
            }
        },
        Err(_) => {
            let service = CryoService::global(paths);
            service.reset_metrics(server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "Cryogenic cooling telemetry and energy counters reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] Cryogenic cooling telemetry and energy counters reset successfully");
    }

    Ok(())
}
