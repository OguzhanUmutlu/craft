// crates/cli/src/commands/crash.rs
//
// CLI command handler for Autonomous AI-Guided Crash Triaging,
// Memory Leak Detection & Automated Core Dump Analysis.
// Strictly zero emojis.

use crate::cli::CrashCommands;
use craft_core::crash::{
    render_crash_report_text, render_crash_status_text, CrashTriageBenchmarkMetrics,
    CrashTriageReport, CrashTriageStatusSummary,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::CrashTriageService;
use serde_json::json;

pub async fn handle_crash(paths: &CraftPaths, action: Option<CrashCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(CrashCommands::Status { server, json }) => handle_status(paths, server, json).await,
        Some(CrashCommands::Triage {
            server,
            file,
            leak_history,
            json,
        }) => handle_triage(paths, server, file, leak_history, json).await,
        Some(CrashCommands::List {
            server,
            limit,
            json,
        }) => handle_list(paths, server, limit, json).await,
        Some(CrashCommands::Inspect { id, json }) => handle_inspect(paths, id, json).await,
        Some(CrashCommands::Bench { iterations, json }) => {
            handle_bench(paths, iterations, json).await
        }
        Some(CrashCommands::ResetMetrics { server, json }) => {
            handle_reset_metrics(paths, server, json).await
        }
    }
}

async fn fetch_status(
    paths: &CraftPaths,
    server: Option<String>,
) -> Result<CrashTriageStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_crash_status(server.clone()).await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.get_status(server.as_deref())
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service.get_status(server.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, server: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, server).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        print!("{}", render_crash_status_text(&summary));
    }

    Ok(())
}

async fn handle_triage(
    paths: &CraftPaths,
    server: Option<String>,
    file: String,
    leak_history: Option<String>,
    json: bool,
) -> Result<()> {
    let mut report: CrashTriageReport = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.triage_crash_file(server.clone(), file.clone()).await {
            Ok(r) => r,
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.triage_file(server.as_deref(), &file)?
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service.triage_file(server.as_deref(), &file)?
        }
    };

    if let Some(lh_path) = leak_history {
        if let Ok(content) = std::fs::read_to_string(&lh_path) {
            if let Ok(candidates) =
                serde_json::from_str::<Vec<craft_core::crash::LeakCandidate>>(&content)
            {
                report.leak_candidates.extend(candidates);
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print!("{}", render_crash_report_text(&report));
    }

    Ok(())
}

async fn handle_list(
    paths: &CraftPaths,
    server: Option<String>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let reports: Vec<CrashTriageReport> = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.list_crash_reports(server.clone(), Some(limit)).await {
            Ok(r) => r,
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.list_reports(server.as_deref(), Some(limit))?
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service.list_reports(server.as_deref(), Some(limit))?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    } else if reports.is_empty() {
        println!("No crash triage reports found.");
    } else {
        println!("=== Historical Crash Triage Reports ===");
        println!(
            "{:<16} {:<12} {:<12} {:<10} {:<30} {:<10}",
            "Report ID", "Server", "Type", "Severity", "Root Cause", "Playbook"
        );
        for r in &reports {
            let srv = r.server.as_deref().unwrap_or("-");
            let cause_preview = if r.root_cause_analysis.len() > 28 {
                format!("{}...", &r.root_cause_analysis[..25])
            } else {
                r.root_cause_analysis.clone()
            };
            println!(
                "{:<16} {:<12} {:<12} {:<10} {:<30} {:<10}",
                r.id,
                srv,
                r.crash_type.to_string(),
                r.severity.to_string(),
                cause_preview,
                if !r.remediation_playbook.is_empty() { "[READY]" } else { "[NONE]" }
            );
        }
    }

    Ok(())
}

async fn handle_inspect(paths: &CraftPaths, id: String, json: bool) -> Result<()> {
    let report: CrashTriageReport = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_crash_report(id.clone()).await {
            Ok(r) => r,
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.get_report(&id)?.ok_or_else(|| {
                    CraftError::Other(format!("Crash report '{}' not found", id))
                })?
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service
                .get_report(&id)?
                .ok_or_else(|| CraftError::Other(format!("Crash report '{}' not found", id)))?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print!("{}", render_crash_report_text(&report));
    }

    Ok(())
}

async fn handle_bench(paths: &CraftPaths, iterations: usize, json: bool) -> Result<()> {
    let metrics: CrashTriageBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_crash_bench(iterations).await {
            Ok(m) => m,
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.run_bench(iterations)?
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service.run_bench(iterations)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        println!("=== Crash Triage & Leak Detection Benchmark Results ===");
        println!("  Processed Dumps:           {}", metrics.processed_dumps);
        println!(
            "  Avg Parse Duration:        {:.2} us",
            metrics.avg_parse_micros
        );
        println!(
            "  P95 Parse Duration:        {:.2} us",
            metrics.p95_parse_micros
        );
        println!(
            "  Dumps / sec:               {:.2}",
            metrics.dumps_per_sec
        );
        println!(
            "  Alloc Diff Rate (ops/sec): {:.2}",
            metrics.alloc_diff_rate_ops_per_sec
        );
    }

    Ok(())
}

async fn handle_reset_metrics(
    paths: &CraftPaths,
    server: Option<String>,
    json: bool,
) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_crash_metrics(server.clone()).await {
            Ok(m) => m,
            Err(_) => {
                let service = CrashTriageService::global(paths);
                service.reset_metrics(server.as_deref())?;
                "Crash triage metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = CrashTriageService::global(paths);
            service.reset_metrics(server.as_deref())?;
            "Crash triage metrics reset successfully".to_string()
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "message": msg,
            })
        );
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
