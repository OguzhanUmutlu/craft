use crate::cli::PatchCommands;
use craft_core::error::Result;
use craft_core::patch::{PatchBenchmarkMetrics, PatchManifest, PatchStatusSummary};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::DynamicPatchService;
use serde_json::json;

pub async fn handle_patch(paths: &CraftPaths, action: Option<PatchCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(PatchCommands::Status { server, json }) => {
            handle_status(paths, server, json).await
        }
        Some(PatchCommands::Apply {
            server,
            patch,
            target,
            bytes,
            json,
        }) => handle_apply(paths, server, patch, target, bytes, json).await,
        Some(PatchCommands::Rollback {
            server,
            patch,
            json,
        }) => handle_rollback(paths, server, patch, json).await,
        Some(PatchCommands::Diff {
            server,
            patch,
            json,
        }) => handle_diff(paths, server, patch, json).await,
        Some(PatchCommands::Bench { iterations, json }) => {
            handle_bench(paths, iterations, json).await
        }
        Some(PatchCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(paths: &CraftPaths, server: Option<String>) -> Result<PatchStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_patch_status(server.clone()).await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = DynamicPatchService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, server).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("=== Autonomous Dynamic Binary Patching & Hot Code Replacement ===");
        println!("  Active Patches:          {}", summary.active_patches);
        println!("  Total Patches Applied:   {}", summary.total_applied);
        println!("  Total Rollbacks:         {}", summary.total_rollbacks);
        println!("  Safety Rejections:       {}", summary.safety_rejections);
        println!("  Avg Apply Latency:       {:.2} us", summary.avg_apply_micros);

        if !summary.patches.is_empty() {
            println!();
            println!("--- Registered Binary & Bytecode Patches ---");
            println!(
                "{:<16} {:<16} {:<16} {:<10} {:>10} {:>12}",
                "Server", "Patch", "Target Symbol", "State", "Type", "Apply (us)"
            );
            for p in &summary.patches {
                println!(
                    "{:<16} {:<16} {:<16} {:<10} {:>10} {:>12}",
                    p.server,
                    p.name,
                    p.target_type.to_string(),
                    format!("{:?}", p.state),
                    format!("{:?}", p.descriptor.patch_type),
                    p.apply_duration_micros
                );
            }
        }
    }

    Ok(())
}

async fn handle_apply(
    paths: &CraftPaths,
    server: String,
    patch: String,
    target: String,
    bytes: String,
    json: bool,
) -> Result<()> {
    let manifest: PatchManifest = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .apply_patch(server.clone(), patch.clone(), target.clone(), bytes.clone())
                .await
            {
                Ok(m) => m,
                Err(_) => {
                    let service = DynamicPatchService::global(paths);
                    service.apply_patch(&server, &patch, &target, &bytes)?
                }
            }
        }
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.apply_patch(&server, &patch, &target, &bytes)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&manifest).unwrap());
    } else {
        println!("[OK] Dynamic binary patch applied successfully");
        println!("  Server:            {}", manifest.server);
        println!("  Patch Name:        {}", manifest.name);
        println!("  Target Symbol:     {}", manifest.target_type.to_string());
        println!("  State:             {:?}", manifest.state);
        println!("  Target Arch:       {:?}", manifest.arch);
        println!("  Trampoline Type:   {:?}", manifest.descriptor.patch_type);
        println!("  Apply Duration:    {} us", manifest.apply_duration_micros);
    }

    Ok(())
}

async fn handle_rollback(
    paths: &CraftPaths,
    server: String,
    patch: String,
    json: bool,
) -> Result<()> {
    let success = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .rollback_patch(server.clone(), patch.clone())
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    let service = DynamicPatchService::global(paths);
                    service.rollback_patch(&server, &patch)?
                }
            }
        }
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.rollback_patch(&server, &patch)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "success": success,
                "server": server,
                "patch": patch
            })
        );
    } else if success {
        println!(
            "[OK] Dynamic binary patch '{}' for server '{}' rolled back and restored",
            patch, server
        );
    } else {
        println!(
            "[WARN] Dynamic binary patch '{}' for server '{}' was not active or not found",
            patch, server
        );
    }

    Ok(())
}

async fn handle_diff(
    paths: &CraftPaths,
    server: String,
    patch: String,
    json: bool,
) -> Result<()> {
    let diff = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client.get_patch_diff(server.clone(), patch.clone()).await {
                Ok(d) => d,
                Err(_) => {
                    let service = DynamicPatchService::global(paths);
                    service.get_diff(&server, &patch)?
                }
            }
        }
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.get_diff(&server, &patch)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "server": server,
                "patch": patch,
                "diff": diff
            })
        );
    } else {
        println!("=== Patch Disassembly & Bytecode Diff: {}/{} ===", server, patch);
        println!("{}", diff);
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    iterations: usize,
    json: bool,
) -> Result<()> {
    let metrics: PatchBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_patch_bench(iterations).await {
            Ok(m) => m,
            Err(_) => {
                let service = DynamicPatchService::global(paths);
                service.run_bench(iterations)?
            }
        },
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.run_bench(iterations)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        println!("=== Dynamic Binary Rewriting & Trampoline Benchmark ===");
        println!("  Iterations:              {}", metrics.iterations);
        println!("  Elapsed Time:            {:.2} ms", metrics.elapsed_ms);
        println!("  Throughput:              {:.1} patches/sec", metrics.patches_per_sec);
        println!("  Avg Generation Latency:  {:.2} us/patch", metrics.avg_apply_micros);
        println!("  P99 Latency:             {:.2} us/patch", metrics.p99_apply_micros);
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_patch_metrics(server.clone()).await {
            Ok(msg) => msg,
            Err(_) => {
                let service = DynamicPatchService::global(paths);
                service.reset_metrics(server.as_deref())?;
                "Dynamic binary patching metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = DynamicPatchService::global(paths);
            service.reset_metrics(server.as_deref())?;
            "Dynamic binary patching metrics reset successfully".to_string()
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
        println!("[OK] Dynamic binary patching metrics and latency counters reset successfully");
    }

    Ok(())
}
