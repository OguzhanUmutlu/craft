use crate::cli::PmuCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::pmu::PmuMetricsSummary;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::PmuService;
use serde_json::json;

pub async fn handle_pmu(paths: &CraftPaths, action: Option<PmuCommands>) -> Result<()> {
    match action {
        None | Some(PmuCommands::Status { json: false }) => handle_status(paths, false).await,
        Some(PmuCommands::Status { json: true }) => handle_status(paths, true).await,
        Some(PmuCommands::Sample { pid, rate, stop, json }) => {
            handle_sample(paths, pid, rate, stop, json).await
        }
        Some(PmuCommands::Hotspots { limit, json }) => handle_hotspots(paths, limit, json).await,
        Some(PmuCommands::Bench { iterations, json }) => handle_bench(paths, iterations, json).await,
        Some(PmuCommands::ResetMetrics { json }) => handle_reset_metrics(paths, json).await,
    }
}

async fn fetch_status(paths: &CraftPaths) -> Result<PmuMetricsSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_pmu_status().await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = PmuService::global(paths);
                service.get_status()
            }
        },
        Err(_) => {
            let service = PmuService::global(paths);
            service.get_status()
        }
    }
}

async fn handle_status(paths: &CraftPaths, json: bool) -> Result<()> {
    let summary = fetch_status(paths).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("=== Autonomous Hardware PMU & Cache Miss Profile ===");
        println!(
            "  Execution Mode:          {}",
            if summary.simulation_mode {
                "Simulation (Fallback)"
            } else {
                "Hardware PMU (Native)"
            }
        );
        println!("  Active Probes:           {}", summary.active_probes);
        println!("  Total Samples:           {}", summary.total_samples);
        println!("  Retired Instructions:    {}", summary.instructions_retired);
        println!("  CPU Cycles:              {}", summary.cpu_cycles);
        println!("  IPC (Instr/Cycle):       {:.3}", summary.ipc);
        println!("  L1D Cache Misses:        {}", summary.l1d_misses);
        println!("  L1D CMPI:                {:.6}", summary.cmpi_l1d);
        println!("  LLC Cache Misses:        {}", summary.llc_misses);
        println!("  LLC CMPI:                {:.6}", summary.cmpi_llc);
        println!("  Branch Mispredictions:   {}", summary.branch_mispredictions);
        println!("  BMPI:                    {:.6}", summary.bmpi);

        if !summary.top_hotspots.is_empty() {
            println!();
            println!("--- Top Execution Hotspots ---");
            println!("{:<6} {:<10} {:<32} {}", "Rank", "Share", "Module / Class", "Demangled Symbol");
            for (idx, hot) in summary.top_hotspots.iter().enumerate().take(10) {
                println!(
                    "#{:<5} {:>6.2}%   {:<32} {}",
                    idx + 1,
                    hot.percentage * 100.0,
                    hot.module_or_class,
                    hot.demangled_symbol
                );
            }
        }
    }

    Ok(())
}

async fn handle_sample(
    paths: &CraftPaths,
    pid: Option<u32>,
    rate: u32,
    stop: bool,
    json: bool,
) -> Result<()> {
    if stop {
        let summary = match DaemonClient::connect(paths).await {
            Ok(mut client) => match client.stop_pmu_sampling().await {
                Ok(s) => s,
                Err(_) => {
                    let service = PmuService::global(paths);
                    service.stop_sampling()?
                }
            },
            Err(_) => {
                let service = PmuService::global(paths);
                service.stop_sampling()?
            }
        };

        if json {
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        } else {
            println!("[OK] Hardware PMU sampling stopped successfully");
        }
        return Ok(());
    }

    let summary = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.start_pmu_sampling(pid, rate).await {
            Ok(s) => s,
            Err(_) => {
                let service = PmuService::global(paths);
                service.start_sampling(pid, rate)?
            }
        },
        Err(_) => {
            let service = PmuService::global(paths);
            service.start_sampling(pid, rate)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!(
            "[OK] Hardware PMU sampling active at {} Hz (Target: {})",
            rate,
            pid.map(|p| format!("PID {}", p))
                .unwrap_or_else(|| "global".to_string())
        );
        println!(
            "  Retired Instructions: {}, CPU Cycles: {}, IPC: {:.3}",
            summary.instructions_retired, summary.cpu_cycles, summary.ipc
        );
        println!(
            "  L1D CMPI: {:.6}, LLC CMPI: {:.6}, BMPI: {:.6}",
            summary.cmpi_l1d, summary.cmpi_llc, summary.bmpi
        );
    }

    Ok(())
}

async fn handle_hotspots(paths: &CraftPaths, limit: usize, json: bool) -> Result<()> {
    let hotspots = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_pmu_hotspots(limit).await {
            Ok(h) => h,
            Err(_) => {
                let service = PmuService::global(paths);
                service.get_hotspots(limit)?
            }
        },
        Err(_) => {
            let service = PmuService::global(paths);
            service.get_hotspots(limit)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&hotspots).unwrap());
    } else {
        println!("=== Top Execution Hotspots ===");
        if hotspots.is_empty() {
            println!("  [WARN] No hotspot samples recorded yet. Run 'craft pmu sample' first.");
        } else {
            println!("{:<6} {:<10} {:<32} {}", "Rank", "Share", "Module / Class", "Demangled Symbol");
            for (idx, hot) in hotspots.iter().enumerate() {
                println!(
                    "#{:<5} {:>6.2}%   {:<32} {}",
                    idx + 1,
                    hot.percentage * 100.0,
                    hot.module_or_class,
                    hot.demangled_symbol
                );
            }
        }
    }

    Ok(())
}

async fn handle_bench(paths: &CraftPaths, iterations: usize, json: bool) -> Result<()> {
    let report = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_pmu_bench(iterations).await {
            Ok(r) => r,
            Err(_) => {
                let service = PmuService::global(paths);
                service.run_bench(iterations)?
            }
        },
        Err(_) => {
            let service = PmuService::global(paths);
            service.run_bench(iterations)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("=== Synthetic Memory Churn & Cache Profiling Benchmark ===");
        println!("  Working Sets:            L1 (64 KB) / LLC (8 MB)");
        println!("  Iterations:              {}", iterations);
        println!("  Total Memory Accesses:   {}", report.total_accesses);
        println!("  Duration:                {} ms", report.duration_ms);
        println!("  L1 Sequential CMPI:      {:.6}", report.l1_sequential_cmpi);
        println!("  L1 Strided CMPI:         {:.6} ({:.1}x miss surge)", report.l1_strided_cmpi, report.l1_strided_cmpi / report.l1_sequential_cmpi.max(1e-6));
        println!("  LLC Sequential CMPI:     {:.6}", report.llc_sequential_cmpi);
        println!("  LLC Strided CMPI:        {:.6} ({:.1}x miss surge)", report.llc_strided_cmpi, report.llc_strided_cmpi / report.llc_sequential_cmpi.max(1e-6));
        println!("  Benchmark Result:        [OK] {}", report.status_message);
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, json: bool) -> Result<()> {
    let message = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_pmu_metrics().await {
            Ok(msg) => msg,
            Err(_) => {
                let service = PmuService::global(paths);
                service.reset_metrics()?;
                "PMU metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = PmuService::global(paths);
            service.reset_metrics()?;
            "PMU metrics reset successfully".to_string()
        }
    };

    if json {
        println!("{}", json!({ "status": "ok", "message": message }));
    } else {
        println!("[OK] Hardware PMU counter metrics and sample buffers reset successfully");
    }

    Ok(())
}
