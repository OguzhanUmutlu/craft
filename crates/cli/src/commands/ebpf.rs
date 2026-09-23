use craft_core::ebpf::EbpfProbeType;
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_daemon::ipc::DaemonClient;
use craft_daemon::EbpfObservabilityService;
use serde_json::json;
use std::path::Path;
use std::str::FromStr;

/// Handles `craft bpf trace`
pub async fn handle_bpf_trace(
    paths: &CraftPaths,
    server: &str,
    duration: u64,
    event: &str,
    rate: u32,
    json: bool,
) -> Result<()> {
    let probe_type = EbpfProbeType::from_str(event)?;

    // Try communicating via supervisor daemon, fallback to in-process service
    let descriptor = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .start_ebpf_profiling(server.to_string(), probe_type, duration, rate)
                .await
            {
                Ok(desc) => desc,
                Err(_) => {
                    let service = EbpfObservabilityService::global(paths);
                    service.start_profiling(server, probe_type, duration, rate)?
                }
            }
        }
        Err(_) => {
            let service = EbpfObservabilityService::global(paths);
            service.start_profiling(server, probe_type, duration, rate)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&descriptor).unwrap());
    } else {
        println!(
            "[PROBE] Attached eBPF tracepoint probe '{}' to server '{}' (PID: {}, Rate: {} Hz, Duration: {}s)",
            descriptor.id, descriptor.server_name, descriptor.pid, descriptor.sample_rate_hz, descriptor.duration_secs
        );
        println!("        Probe Type: {}", descriptor.probe_type);
        println!("        Status:     {}", descriptor.status);
    }

    Ok(())
}

/// Handles `craft bpf status`
pub async fn handle_bpf_status(paths: &CraftPaths, server: &str, json: bool) -> Result<()> {
    let (descriptor, socket_telemetry, aggs) = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_ebpf_status(server.to_string()).await {
            Ok(res) => res,
            Err(_) => {
                let service = EbpfObservabilityService::global(paths);
                service.get_status(server)?
            }
        },
        Err(_) => {
            let service = EbpfObservabilityService::global(paths);
            service.get_status(server)?
        }
    };

    if json {
        let output = json!({
            "server": server,
            "descriptor": descriptor,
            "socket_telemetry": socket_telemetry,
            "syscall_aggregations": aggs,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(());
    }

    println!("[EBPF] Kernel Observability & Probe Status for '{}'", server);
    println!("--------------------------------------------------------------------------------");

    if let Some(desc) = descriptor {
        println!("Active Probe ID:    {}", desc.id);
        println!("Target Process PID: {}", desc.pid);
        println!("Probe Type:         {}", desc.probe_type);
        println!("Status:             {}", desc.status);
        println!("Sample Rate:        {} Hz", desc.sample_rate_hz);
        println!("Events Intercepted: {}", desc.event_count);
    } else {
        println!("Status:             [IDLE] No active eBPF probe attached");
    }

    println!();
    if let Some(sock) = socket_telemetry {
        println!("Socket Buffer Telemetry:");
        println!("  Pressure Level:     {}", sock.pressure_level);
        println!(
            "  RX Queue Depth:     {} / {} bytes ({:.1}%)",
            sock.rx_queue_bytes,
            sock.so_rcvbuf_bytes,
            sock.rx_fill_percentage()
        );
        println!(
            "  TX Queue Depth:     {} / {} bytes ({:.1}%)",
            sock.tx_queue_bytes,
            sock.so_sndbuf_bytes,
            sock.tx_fill_percentage()
        );
        println!("  Active Sockets:     {}", sock.active_connections);
        println!("  TCP Window Stalls:  {}", sock.window_stalls);
    }

    println!();
    println!("Intercepted Syscalls & Latency Telemetry:");
    if aggs.is_empty() {
        println!("  (No kernel syscalls recorded yet)");
    } else {
        println!("  {:<18} {:<10} {:<15}", "Syscall", "Count", "Avg Latency");
        println!("  ---------------------------------------------");
        for (name, (count, avg_ms)) in aggs {
            println!("  {:<18} {:<10} {:>10.3} ms", name, count, avg_ms);
        }
    }

    Ok(())
}

/// Handles `craft bpf flamegraph`
pub async fn handle_bpf_flamegraph(
    paths: &CraftPaths,
    server: &str,
    out: Option<&Path>,
    format: &str,
    json: bool,
) -> Result<()> {
    let (content, root_node) = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .get_ebpf_flamegraph(server.to_string(), format.to_string())
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = EbpfObservabilityService::global(paths);
                    service.get_flamegraph(server, format)?
                }
            }
        }
        Err(_) => {
            let service = EbpfObservabilityService::global(paths);
            service.get_flamegraph(server, format)?
        }
    };

    if json {
        let output = json!({
            "server": server,
            "format": format,
            "root_node": root_node,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(());
    }

    if format.eq_ignore_ascii_case("svg") || out.is_some() {
        let dest = if let Some(p) = out {
            p.to_path_buf()
        } else {
            paths.ebpf_flamegraph_path(server)
        };

        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::write(&dest, &content).map_err(CraftError::Io)?;
        println!(
            "[FLAME] SVG flame graph generated and saved to '{}' (Total Samples: {})",
            dest.display(),
            root_node.value
        );
    } else {
        println!("{}", content);
    }

    Ok(())
}

/// Handles `craft bpf gc`
pub async fn handle_bpf_gc(
    paths: &CraftPaths,
    server: &str,
    limit: usize,
    watch: bool,
    json: bool,
) -> Result<()> {
    let events = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_ebpf_gc_telemetry(server.to_string(), limit).await {
            Ok(evs) => evs,
            Err(_) => {
                let service = EbpfObservabilityService::global(paths);
                service.get_gc_telemetry(server, limit)?
            }
        },
        Err(_) => {
            let service = EbpfObservabilityService::global(paths);
            service.get_gc_telemetry(server, limit)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&events).unwrap());
        return Ok(());
    }

    println!("[JVM] Deep GC Telemetry & Safepoint Analysis for '{}'", server);
    println!("--------------------------------------------------------------------------------");
    println!(
        "{:<19} {:<10} {:<16} {:>10} {:>15} {:>12}",
        "Timestamp", "Collector", "Phase", "Pause (ms)", "Reclaimed", "Safepoint"
    );
    println!("--------------------------------------------------------------------------------");

    if events.is_empty() {
        println!("(No recent GC events observed)");
    } else {
        for ev in &events {
            let dt = chrono::DateTime::from_timestamp_millis(ev.timestamp_ms as i64)
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            let reclaimed = craft_core::format_size(ev.reclaimed_bytes().max(0) as u64);
            let safepoint_str = if ev.safepoint_sync_ms() > 50.0 {
                format!("{:.1}ms [WARN]", ev.safepoint_sync_ms())
            } else {
                format!("{:.1}ms", ev.safepoint_sync_ms())
            };

            println!(
                "{:<19} {:<10} {:<16} {:>9.2}ms {:>15} {:>12}",
                dt, ev.collector, ev.phase, ev.pause_ms(), reclaimed, safepoint_str
            );
        }
    }

    if watch {
        println!();
        println!("[WATCH] Watching for real-time safepoint spikes... Press Ctrl+C to exit.");
    }

    Ok(())
}

/// Handles `craft bpf stop`
pub async fn handle_bpf_stop(
    paths: &CraftPaths,
    server: &str,
    probe_id: Option<&str>,
    json: bool,
) -> Result<()> {
    let (descriptor, message) = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .stop_ebpf_profiling(server.to_string(), probe_id.map(|s| s.to_string()))
                .await
            {
                Ok(res) => res,
                Err(_) => {
                    let service = EbpfObservabilityService::global(paths);
                    let desc = service.stop_profiling(server, probe_id)?;
                    (desc, "Kernel probe detached successfully".to_string())
                }
            }
        }
        Err(_) => {
            let service = EbpfObservabilityService::global(paths);
            let desc = service.stop_profiling(server, probe_id)?;
            (desc, "Kernel probe detached successfully".to_string())
        }
    };

    if json {
        let output = json!({
            "probe": descriptor,
            "message": message,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!(
            "[OK] Detached eBPF probe '{}' for server '{}': {}",
            descriptor.id, descriptor.server_name, message
        );
    }

    Ok(())
}

/// Dispatches all `craft bpf` subcommands
pub async fn handle_bpf(action: crate::cli::BpfCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        crate::cli::BpfCommands::Trace {
            server,
            duration,
            event,
            rate,
            json,
        } => handle_bpf_trace(paths, &server, duration, &event, rate, json).await,
        crate::cli::BpfCommands::Status { server, json } => {
            handle_bpf_status(paths, &server, json).await
        }
        crate::cli::BpfCommands::Flamegraph {
            server,
            out,
            format,
            json,
        } => handle_bpf_flamegraph(paths, &server, out.as_deref(), &format, json).await,
        crate::cli::BpfCommands::Gc {
            server,
            limit,
            watch,
            json,
        } => handle_bpf_gc(paths, &server, limit, watch, json).await,
        crate::cli::BpfCommands::Stop {
            server,
            probe_id,
            json,
        } => handle_bpf_stop(paths, &server, probe_id.as_deref(), json).await,
    }
}
