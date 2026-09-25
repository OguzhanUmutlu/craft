// crates/cli/src/commands/ptp.rs
//
// CLI command handler for Autonomous Sub-Atomic Quantum Clock Synchronization,
// PTP Hardware Timestamping & Relativity-Aware Tick Sequencing.
// Strictly zero emojis.

use std::str::FromStr;

use crate::cli::PtpCommands;
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_core::ptp::{
    render_ptp_bench_table, render_ptp_status_table, render_truetime_table, ClockServoMode,
    PtpBenchmarkMetrics, PtpStatusSummary, TrueTimeInterval,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::PtpClockService;
use serde_json::json;

pub async fn handle_ptp(
    paths: &CraftPaths,
    action: Option<PtpCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(PtpCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(PtpCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(PtpCommands::TrueTime { server, json }) => {
            handle_truetime(paths, server, json).await
        }
        Some(PtpCommands::Step {
            server,
            offset,
            rtt,
            json,
        }) => handle_step(paths, server, offset, rtt, json).await,
        Some(PtpCommands::LeapSmear {
            server,
            leap_seconds,
            json,
        }) => handle_leap_smear(paths, server, leap_seconds, json).await,
        Some(PtpCommands::Bench {
            iterations,
            peers,
            json,
        }) => handle_bench(paths, iterations, peers, json).await,
        Some(PtpCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<PtpStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_ptp_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
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
        println!("{}", render_ptp_status_table(&summary));
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
        Ok(mut client) => match client.set_ptp_servo_mode(&mode, Some(&server)).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                let servo_mode = ClockServoMode::from_str(&mode)?;
                service.set_mode(servo_mode, Some(&server))?;
                format!("PTP clock servo mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            let servo_mode = ClockServoMode::from_str(&mode)?;
            service.set_mode(servo_mode, Some(&server))?;
            format!("PTP clock servo mode updated to {}", mode)
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

async fn handle_truetime(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let interval: TrueTimeInterval = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.query_ptp_truetime(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.query_truetime(server.as_deref())?
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            service.query_truetime(server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&interval)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_truetime_table(&interval));
    }

    Ok(())
}

async fn handle_step(
    paths: &CraftPaths,
    server: Option<String>,
    offset: f64,
    rtt: f64,
    json: bool,
) -> Result<()> {
    let corrected: f64 = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.step_ptp_servo(offset, rtt, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.step_servo(offset, rtt, server.as_deref())?
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            service.step_servo(offset, rtt, server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "offset_ns": offset,
            "rtt_ns": rtt,
            "corrected_phase_offset_ns": corrected,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] PTP servo stepped: input offset = {:.3} ns, RTT = {:.2} ns, corrected offset = {:.3} ns",
            offset, rtt, corrected
        );
    }

    Ok(())
}

async fn handle_leap_smear(
    paths: &CraftPaths,
    server: Option<String>,
    leap_seconds: i32,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.trigger_ptp_leap_smear(leap_seconds, server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.trigger_leap_smear(leap_seconds, server.as_deref())?
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            service.trigger_leap_smear(leap_seconds, server.as_deref())?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "leap_seconds": leap_seconds,
            "message": format!("Triggered 24-hour cosine leap second smear ({} seconds)", leap_seconds),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Triggered 24-hour cosine leap second smear ({} seconds)",
            leap_seconds
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: u32,
    peers: u16,
    json: bool,
) -> Result<()> {
    let metrics: PtpBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_ptp_bench(iterations, peers).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.run_bench(iterations, peers)?
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            service.run_bench(iterations, peers)?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_ptp_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_ptp_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = PtpClockService::global(paths);
                service.reset_metrics()?
            }
        },
        Err(_) => {
            let service = PtpClockService::global(paths);
            service.reset_metrics()?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "PTP clock synchronization telemetry counters reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] PTP clock synchronization telemetry counters reset successfully");
    }

    Ok(())
}
