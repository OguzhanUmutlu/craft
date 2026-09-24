use crate::cli::VmCommands;
use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::vm::{
    MicroVmBenchmarkMetrics, MicroVmStatusSummary,
};
use craft_daemon::ipc::DaemonClient;
use craft_daemon::MicroVmService;
use serde_json::json;

pub async fn handle_vm(paths: &CraftPaths, action: Option<VmCommands>) -> Result<()> {
    match action {
        None => handle_status(paths, None, false).await,
        Some(VmCommands::Status { id, json }) => handle_status(paths, id, json).await,
        Some(VmCommands::Spawn {
            name,
            vcpus,
            memory,
            cid,
            devices,
            json,
        }) => handle_spawn(paths, name, vcpus, memory, cid, devices, json).await,
        Some(VmCommands::Stop { id, force, json }) => handle_stop(paths, id, force, json).await,
        Some(VmCommands::Inspect { id, json }) => handle_inspect(paths, id, json).await,
        Some(VmCommands::Bench {
            concurrency,
            iterations,
            json,
        }) => handle_bench(paths, concurrency, iterations, json).await,
        Some(VmCommands::ResetMetrics { id, json }) => {
            handle_reset_metrics(paths, id, json).await
        }
    }
}

async fn fetch_status(paths: &CraftPaths, vm_id: Option<String>) -> Result<MicroVmStatusSummary> {
    match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.get_vm_status(vm_id.clone()).await {
            Ok(summary) => Ok(summary),
            Err(_) => {
                let service = MicroVmService::global(paths);
                service.get_status(vm_id.as_deref())
            }
        },
        Err(_) => {
            let service = MicroVmService::global(paths);
            service.get_status(vm_id.as_deref())
        }
    }
}

async fn handle_status(paths: &CraftPaths, id: Option<String>, json: bool) -> Result<()> {
    let summary = fetch_status(paths, id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("=== Autonomous MicroVM Sandboxing & KVM Isolation ===");
        println!("  Active MicroVMs:       {}", summary.active_vms);
        println!("  Total vCPUs Allocated: {}", summary.total_vcpus);
        println!("  Total Memory (MB):     {}", summary.total_memory_mb);
        println!("  Avg Cold Start:        {:.2} ms", summary.avg_boot_time_ms);
        println!(
            "  KVM Hardware Driver:   {}",
            if summary.kvm_available {
                "[AVAILABLE] (/dev/kvm)"
            } else {
                "[FALLBACK] (Synthetic KVM Virtualization)"
            }
        );
        println!("  KVM API Version:       {}", summary.kvm_api_version);

        if !summary.vms.is_empty() {
            println!();
            println!("--- Registered MicroVM Instances ---");
            println!(
                "{:<16} {:<16} {:<12} {:>6} {:>10} {:>6} {:>14}",
                "VM ID", "Name", "State", "vCPUs", "Memory (MB)", "CID", "Cold Start (ms)"
            );
            for v in &summary.vms {
                println!(
                    "{:<16} {:<16} {:<12} {:>6} {:>10} {:>6} {:>14.2}",
                    v.config.vm_id,
                    v.config.name,
                    v.state.to_string(),
                    v.config.vcpus,
                    v.config.memory_mb,
                    v.config.vsock_cid,
                    v.boot_time_ms
                );
            }
        }
    }

    Ok(())
}

async fn handle_spawn(
    paths: &CraftPaths,
    name: String,
    vcpus: u32,
    memory: u64,
    cid: Option<u32>,
    devices_opt: Option<String>,
    json: bool,
) -> Result<()> {
    let devices: Vec<String> = devices_opt
        .unwrap_or_else(|| "net,vsock,block".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let desc = match DaemonClient::connect(paths).await {
        Ok(mut client) => {
            match client
                .spawn_vm(name.clone(), vcpus, memory, cid, devices.clone())
                .await
            {
                Ok(d) => d,
                Err(_) => {
                    let service = MicroVmService::global(paths);
                    service.spawn_vm(&name, vcpus, memory, cid, devices)?
                }
            }
        }
        Err(_) => {
            let service = MicroVmService::global(paths);
            service.spawn_vm(&name, vcpus, memory, cid, devices)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&desc).unwrap());
    } else {
        println!("[OK] MicroVM '{}' spawned successfully.", desc.config.name);
        println!("  VM ID:                 {}", desc.config.vm_id);
        println!("  Context ID (CID):      {}", desc.config.vsock_cid);
        println!("  vCPUs:                 {}", desc.config.vcpus);
        println!("  Allocated Memory:      {} MB", desc.config.memory_mb);
        println!("  Cold Start Duration:   {:.2} ms", desc.boot_time_ms);
        println!(
            "  Virtio Devices:        {}",
            desc.config
                .virtio_devices
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some(pid) = desc.pid {
            println!("  Virtual Supervisor PID:{}", pid);
        }
    }

    Ok(())
}

async fn handle_stop(paths: &CraftPaths, id: String, force: bool, json: bool) -> Result<()> {
    let stopped = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.stop_vm(id.clone(), force).await {
            Ok(s) => s,
            Err(_) => {
                let service = MicroVmService::global(paths);
                service.stop_vm(&id, force)?
            }
        },
        Err(_) => {
            let service = MicroVmService::global(paths);
            service.stop_vm(&id, force)?
        }
    };

    if json {
        println!(
            "{}",
            json!({
                "stopped": stopped,
                "id": id,
            })
        );
    } else if stopped {
        println!("[OK] MicroVM '{}' terminated successfully.", id);
    } else {
        println!("[WARN] MicroVM '{}' was not running.", id);
    }

    Ok(())
}

async fn handle_inspect(paths: &CraftPaths, id: String, json: bool) -> Result<()> {
    let desc = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.inspect_vm(id.clone()).await {
            Ok(d) => d,
            Err(_) => {
                let service = MicroVmService::global(paths);
                service.inspect_vm(&id)?
            }
        },
        Err(_) => {
            let service = MicroVmService::global(paths);
            service.inspect_vm(&id)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&desc).unwrap());
    } else {
        println!("=== MicroVM Inspection: {} ===", desc.config.name);
        println!("  VM ID:                 {}", desc.config.vm_id);
        println!("  State:                 {}", desc.state);
        println!("  vCPUs:                 {}", desc.config.vcpus);
        println!("  Memory:                {} MB", desc.config.memory_mb);
        println!("  VSOCK CID:             {}", desc.config.vsock_cid);
        println!("  Cold Start Duration:   {:.2} ms", desc.boot_time_ms);
        println!("  PID:                   {:?}", desc.pid);
        println!("  Uptime:                {} seconds", desc.uptime_seconds);
        println!("  Seccomp Level:         {}", desc.config.seccomp_level);
        println!(
            "  Virtio Devices:        {}",
            desc.config
                .virtio_devices
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(())
}

async fn handle_bench(
    paths: &CraftPaths,
    concurrency: usize,
    iterations: usize,
    json: bool,
) -> Result<()> {
    let metrics: MicroVmBenchmarkMetrics = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.run_vm_bench(concurrency, iterations).await {
            Ok(m) => m,
            Err(_) => {
                let service = MicroVmService::global(paths);
                service.run_bench(concurrency, iterations)?
            }
        },
        Err(_) => {
            let service = MicroVmService::global(paths);
            service.run_bench(concurrency, iterations)?
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    } else {
        println!("=== Autonomous MicroVM Cold Start & AF_VSOCK Benchmark ===");
        println!("  Concurrency:           {} workers", metrics.concurrency);
        println!("  Total Boots:           {}", metrics.total_boots);
        println!("  Avg Cold Start:        {:.2} ms", metrics.avg_cold_start_ms);
        println!("  P50 Cold Start:        {:.2} ms", metrics.p50_cold_start_ms);
        println!("  P95 Cold Start:        {:.2} ms", metrics.p95_cold_start_ms);
        println!("  P99 Cold Start:        {:.2} ms", metrics.p99_cold_start_ms);
        println!(
            "  AF_VSOCK Throughput:   {:.1} msgs/sec",
            metrics.vsock_throughput_msgs_sec
        );
        println!(
            "  AF_VSOCK Bandwidth:    {:.2} MB/s",
            metrics.vsock_bandwidth_mb_sec
        );
        println!(
            "  AF_VSOCK Latency:      {:.2} us",
            metrics.vsock_latency_micros
        );
    }

    Ok(())
}

async fn handle_reset_metrics(paths: &CraftPaths, id: Option<String>, json: bool) -> Result<()> {
    let msg = match DaemonClient::connect(paths).await {
        Ok(mut client) => match client.reset_vm_metrics(id.clone()).await {
            Ok(m) => m,
            Err(_) => {
                let service = MicroVmService::global(paths);
                let _ = service.reset_metrics(id.as_deref())?;
                "MicroVM metrics reset successfully".to_string()
            }
        },
        Err(_) => {
            let service = MicroVmService::global(paths);
            let _ = service.reset_metrics(id.as_deref())?;
            "MicroVM metrics reset successfully".to_string()
        }
    };

    if json {
        println!("{}", json!({ "success": true, "message": msg }));
    } else {
        println!("[OK] {}", msg);
    }

    Ok(())
}
