// crates/cli/src/commands/neuromorphic.rs
//
// CLI command handler for Autonomous Neuromorphic AI Tick Scheduling,
// Spike-Driven Game Loop Inference & Microsecond Latency Forecasting.
// Strictly zero emojis.

use crate::cli::NeuromorphicCommands;
use craft_core::error::{CraftError, Result};
use craft_core::neuromorphic::{
    render_neuromorphic_bench_table, render_neuromorphic_status_table,
    render_raster_plot_table, NeuromorphicBenchmarkMetrics,
    NeuromorphicStatusSummary,
};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::NeuromorphicService;
use serde_json::json;

pub async fn handle_neuromorphic(
    paths: &CraftPaths,
    action: Option<NeuromorphicCommands>,
) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(NeuromorphicCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(NeuromorphicCommands::Mode { server, mode, json }) => {
            handle_mode(paths, server, mode, json).await
        }
        Some(NeuromorphicCommands::Inject {
            server,
            neuron,
            current,
            source,
            json,
        }) => handle_inject(paths, server, neuron, current, source, json).await,
        Some(NeuromorphicCommands::Raster { server, limit, json }) => {
            handle_raster(paths, server, limit, json).await
        }
        Some(NeuromorphicCommands::Predict { server, json }) => {
            handle_predict(paths, server, json).await
        }
        Some(NeuromorphicCommands::Bench {
            iterations,
            burst_ratio,
            json,
        }) => handle_bench(paths, iterations, burst_ratio, json).await,
        Some(NeuromorphicCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<NeuromorphicStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_neuromorphic_status(server.as_deref()).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let service = NeuromorphicService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = NeuromorphicService::global(paths);
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
        println!("{}", render_neuromorphic_status_table(&summary));
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
        Ok(mut client) => match client.set_neuromorphic_mode(Some(&server), &mode).await {
            Ok(res) => res,
            Err(_) => {
                let service = NeuromorphicService::global(paths);
                service.set_mode(Some(&server), &mode)?;
                format!("Neuromorphic schedule mode updated to {}", mode)
            }
        },
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.set_mode(Some(&server), &mode)?;
            format!("Neuromorphic schedule mode updated to {}", mode)
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

async fn handle_inject(
    paths: &CraftPaths,
    server: String,
    neuron: u32,
    current: f32,
    source: String,
    json: bool,
) -> Result<()> {
    let spike_id = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .inject_neuromorphic_spike(Some(&server), neuron, current, &source)
                .await
            {
                Ok(id) => id,
                Err(_) => {
                    let service = NeuromorphicService::global(paths);
                    service.inject_spike(Some(&server), neuron, current, &source)?
                }
            }
        }
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.inject_spike(Some(&server), neuron, current, &source)?
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "server": server,
            "neuron": neuron,
            "current": current,
            "source": source,
            "spike_id": spike_id,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "[OK] Injected spike {} to neuron {} with current {:.2} (source: {})",
            spike_id, neuron, current, source
        );
    }

    Ok(())
}

async fn handle_raster(
    paths: &CraftPaths,
    server: Option<String>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let points = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_neuromorphic_raster(server.as_deref(), Some(limit)).await {
            Ok(res) => res,
            Err(_) => {
                let service = NeuromorphicService::global(paths);
                service.get_raster(server.as_deref(), Some(limit))?
            }
        },
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.get_raster(server.as_deref(), Some(limit))?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&points)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_raster_plot_table(&points));
    }

    Ok(())
}

async fn handle_predict(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let prediction = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_neuromorphic_prediction(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = NeuromorphicService::global(paths);
                service.get_prediction(server.as_deref())?
            }
        },
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.get_prediction(server.as_deref())?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&prediction)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!(
            "+--------------------------------------------------------------+\n\
             | Neuromorphic AI Tick Prediction                              |\n\
             +--------------------+-----------------------------------------+\n\
             | Predicted Duration | {:<36.1} us |\n\
             | Recommended Sleep  | {:<36} us |\n\
             | Burst Intensity    | {:<39.2} |\n\
             | Contention Score   | {:<39.2} |\n\
             | Confidence         | {:<38.1}% |\n\
             +--------------------+-----------------------------------------+",
            prediction.predicted_duration_micros,
            prediction.recommended_sleep_micros,
            prediction.burst_intensity,
            prediction.entity_contention_score,
            prediction.confidence * 100.0,
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    burst_ratio: f64,
    json: bool,
) -> Result<()> {
    let metrics: NeuromorphicBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .run_neuromorphic_bench(Some(iterations), Some(burst_ratio))
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = NeuromorphicService::global(paths);
                    service.run_bench(Some(iterations), Some(burst_ratio))?
                }
            }
        }
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.run_bench(Some(iterations), Some(burst_ratio))?
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&metrics)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("{}", render_neuromorphic_bench_table(&metrics));
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_neuromorphic_metrics(server.as_deref()).await {
            Ok(res) => res,
            Err(_) => {
                let service = NeuromorphicService::global(paths);
                service.reset_metrics(server.as_deref())?;
                true
            }
        },
        Err(_) => {
            let service = NeuromorphicService::global(paths);
            service.reset_metrics(server.as_deref())?;
            true
        }
    };

    if json {
        let output = json!({
            "status": "OK",
            "success": success,
            "message": "Neuromorphic telemetry counters and raster buffer reset successfully",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| CraftError::Other(e.to_string()))?
        );
    } else {
        println!("[OK] Neuromorphic telemetry counters and raster buffer reset successfully");
    }

    Ok(())
}
