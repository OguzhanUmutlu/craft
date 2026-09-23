use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    format_cpu_range_string, parse_cpu_range_string, CpuAffinityManager, CraftError, CraftPaths,
    NumaPolicy, NumaRegistry, NumaStatusSummary, Result, ServerPinningConfig,
};
use craft_daemon::DaemonClient;

use crate::cli::NumaCommands;

pub async fn handle_numa(action: NumaCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        NumaCommands::Status { json } => status(json, paths).await,
        NumaCommands::Pin {
            server,
            cpus,
            node,
            policy,
            json,
        } => pin(server, cpus, node, policy, json, paths).await,
        NumaCommands::Policy {
            server,
            policy,
            node,
            json,
        } => policy_cmd(server, policy, node, json, paths).await,
        NumaCommands::Bench {
            node,
            size_mb,
            json,
        } => bench(node, size_mb, json, paths).await,
        NumaCommands::BootArgs {
            cores,
            hugepages_1g,
            hugepages_2m,
            json,
        } => boot_args(cores, hugepages_1g, hugepages_2m, json).await,
    }
}

async fn status(json: bool, paths: &CraftPaths) -> Result<()> {
    let summary: NumaStatusSummary = if let Ok(mut client) = DaemonClient::connect(paths).await {
        if let Ok(s) = client.get_numa_status().await {
            s
        } else {
            let reg = NumaRegistry::load(paths).unwrap_or_default();
            reg.get_status_summary()
        }
    } else {
        let reg = NumaRegistry::load(paths).unwrap_or_default();
        reg.get_status_summary()
    };

    let dpdk_stats = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.get_dpdk_status(None).await.ok()
    } else {
        craft_daemon::DpdkNumaService::global(paths).get_dpdk_status(None).into()
    };

    if json {
        let combined = serde_json::json!({
            "numa": summary,
            "dpdk": dpdk_stats,
        });
        println!("{}", serde_json::to_string_pretty(&combined)?);
        return Ok(());
    }

    println!();
    println!("  AUTONOMOUS KERNEL-BYPASSED DPDK & NUMA TOPOLOGY");
    println!("  ==============================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Topology Metric").fg(Color::Cyan),
            Cell::new("System Value").fg(Color::Green),
        ]);

    let numa_support_str = if summary.topology.is_numa_available {
        "[OK] Hardware NUMA Architecture Available"
    } else {
        "[INFO] Single Unified Memory Architecture (UMA) Fallback"
    };

    let total_mem_bytes: u64 = summary.topology.nodes.iter().map(|n| n.total_memory_bytes).sum();
    let free_mem_bytes: u64 = summary.topology.nodes.iter().map(|n| n.free_memory_bytes).sum();
    let total_mem_mb = (total_mem_bytes as f64) / (1024.0 * 1024.0);
    let free_mem_mb = (free_mem_bytes as f64) / (1024.0 * 1024.0);

    let isolated_str = if summary.isolated_cpus.is_empty() {
        "None (run `craft numa boot-args` to configure isolcpus)".to_string()
    } else {
        format_cpu_range_string(&summary.isolated_cpus)
    };

    let dpdk_str = if let Some(ref d) = dpdk_stats {
        if d.is_hardware_driver_active {
            "[OK] DPDK Hardware Driver Active (vfio-pci)".to_string()
        } else {
            format!(
                "[INFO] High-Performance Ring Buffer Fallback ({:.0} pps, {:.2} µs jitter)",
                d.throughput_pps, d.avg_jitter_micros
            )
        }
    } else {
        "[INFO] Cache-Line Ring Buffer Active".to_string()
    };

    table.add_row(Row::from(vec![
        Cell::new("NUMA Subsystem"),
        Cell::new(numa_support_str).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Online NUMA Nodes"),
        Cell::new(&summary.topology.nodes.len().to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Total Logic CPUs"),
        Cell::new(&summary.topology.total_cpus.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("NUMA Memory Capacity"),
        Cell::new(&format!("{:.2} MB total / {:.2} MB free", total_mem_mb, free_mem_mb)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Kernel Isolated Cores"),
        Cell::new(&isolated_str).fg(Color::Cyan),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Kernel-Bypass DPDK"),
        Cell::new(&dpdk_str),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Pinned Game Servers"),
        Cell::new(&summary.pinned_servers_count.to_string()),
    ]));

    println!("{table}");
    println!();

    if !summary.topology.nodes.is_empty() {
        println!("  NUMA NODE DETAILS");
        println!("  -----------------");
        let mut node_table = Table::new();
        node_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Node").fg(Color::Cyan),
                Cell::new("CPUs").fg(Color::Green),
                Cell::new("Total RAM (MB)").fg(Color::Green),
                Cell::new("Free RAM (MB)").fg(Color::Green),
                Cell::new("2MB Hugepages").fg(Color::Green),
                Cell::new("1GB Hugepages").fg(Color::Green),
            ]);

        for node in &summary.topology.nodes {
            let n_total_mb = (node.total_memory_bytes as f64) / (1024.0 * 1024.0);
            let n_free_mb = (node.free_memory_bytes as f64) / (1024.0 * 1024.0);
            let cpus_str = format_cpu_range_string(&node.cpus);
            node_table.add_row(Row::from(vec![
                Cell::new(&node.node_id.to_string()),
                Cell::new(&cpus_str),
                Cell::new(&format!("{:.2}", n_total_mb)),
                Cell::new(&format!("{:.2}", n_free_mb)),
                Cell::new(&node.hugepages_2mb.to_string()),
                Cell::new(&node.hugepages_1gb.to_string()),
            ]));
        }
        println!("{node_table}");
        println!();
    }

    if !summary.servers.is_empty() {
        println!("  PINNED SERVER ALLOCATIONS");
        println!("  -------------------------");
        let mut server_table = Table::new();
        server_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Server").fg(Color::Cyan),
                Cell::new("Pinned Cores").fg(Color::Green),
                Cell::new("NUMA Node").fg(Color::Green),
                Cell::new("Memory Policy").fg(Color::Green),
                Cell::new("Hugepages").fg(Color::Green),
            ]);

        for s in &summary.servers {
            let cpus_str = format_cpu_range_string(&s.pinned_cpus);
            let node_str = s.numa_node.map(|n| n.to_string()).unwrap_or_else(|| "Any".to_string());
            let hp_str = if s.hugepages_enabled { "[OK] Enabled" } else { "Disabled" };
            server_table.add_row(Row::from(vec![
                Cell::new(&s.server_name),
                Cell::new(&cpus_str),
                Cell::new(&node_str),
                Cell::new(s.policy.as_str()),
                Cell::new(hp_str),
            ]));
        }
        println!("{server_table}");
        println!();
    }

    Ok(())
}

async fn pin(
    server: String,
    cpus: String,
    node: Option<u32>,
    policy: String,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let parsed_policy = NumaPolicy::from_str_policy(&policy, node);
    let parsed_cpus = parse_cpu_range_string(&cpus)?;

    let config = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .pin_server_cores(server.clone(), parsed_cpus.clone(), node, parsed_policy)
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        // Offline configuration
        let mut reg = NumaRegistry::load(paths).unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cfg = ServerPinningConfig {
            server_name: server.clone(),
            numa_node: node,
            pinned_cpus: parsed_cpus.clone(),
            policy: parsed_policy,
            hugepages_enabled: false,
            updated_at: now,
        };
        reg.set_pinning(cfg.clone(), paths)?;
        cfg
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&config)?);
        return Ok(());
    }

    let cpus_formatted = format_cpu_range_string(&config.pinned_cpus);
    let node_formatted = config.numa_node.map(|n| n.to_string()).unwrap_or_else(|| "System Default".to_string());

    println!();
    println!("  [OK] SERVER PINNING CONFIGURED: {}", config.server_name);
    println!("  ---------------------------------------------");
    println!("  Pinned CPU Cores : {}", cpus_formatted);
    println!("  NUMA Node Target : {}", node_formatted);
    println!("  Memory Policy    : {}", config.policy.as_str());
    println!("  Hugepages Active : {}", if config.hugepages_enabled { "Yes" } else { "No" });
    println!();

    Ok(())
}

async fn policy_cmd(
    server: String,
    policy: String,
    node: Option<u32>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let parsed_policy = NumaPolicy::from_str_policy(&policy, node);

    let config = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .set_numa_policy(server.clone(), parsed_policy)
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        let mut reg = NumaRegistry::load(paths).unwrap_or_default();
        let mut cfg = if let Some(existing) = reg.get_pinning(&server) {
            existing.clone()
        } else {
            ServerPinningConfig {
                server_name: server.clone(),
                numa_node: node,
                pinned_cpus: vec![0, 1],
                policy: parsed_policy,
                hugepages_enabled: false,
                updated_at: 0,
            }
        };
        cfg.policy = parsed_policy;
        if node.is_some() {
            cfg.numa_node = node;
        }
        cfg.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        reg.set_pinning(cfg.clone(), paths)?;
        cfg
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&config)?);
        return Ok(());
    }

    println!();
    println!("  [OK] NUMA POLICY UPDATED: {}", config.server_name);
    println!("  -----------------------------------------");
    println!("  Policy           : {}", config.policy.as_str());
    println!("  Target Node      : {}", config.numa_node.map(|n| n.to_string()).unwrap_or_else(|| "Default".to_string()));
    println!();

    Ok(())
}

async fn bench(
    node: u32,
    size_mb: usize,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let report = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .benchmark_numa_memory(node, size_mb)
            .await
            .map_err(|e| CraftError::Other(e.to_string()))?
    } else {
        NumaRegistry::benchmark_memory(node, size_mb)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!();
    println!("  NUMA MEMORY & DPDK PACKET RING BENCHMARK");
    println!("  ========================================");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Benchmark Metric").fg(Color::Cyan),
            Cell::new("Result").fg(Color::Green),
        ]);

    table.add_row(Row::from(vec![
        Cell::new("Tested Node ID"),
        Cell::new(&report.node_tested.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Buffer Size Tested"),
        Cell::new(&format!("{} MB", report.buffer_size_mb)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Local Memory Bandwidth"),
        Cell::new(&format!("{:.2} MB/s", report.local_bandwidth_mb_s)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Remote (Cross-Socket) Bandwidth"),
        Cell::new(&format!("{:.2} MB/s", report.remote_bandwidth_mb_s)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Remote Interconnect Penalty"),
        Cell::new(&format!("{:.2}%", report.bandwidth_penalty_pct)).fg(Color::Yellow),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Direct Memory Allocation Latency"),
        Cell::new(&format!("{:.2} ns/MB", report.alloc_latency_ns)).fg(Color::Cyan),
    ]));

    println!("{table}");
    println!();
    Ok(())
}

async fn boot_args(
    cores: String,
    hugepages_1g: Option<usize>,
    _hugepages_2m: Option<usize>,
    json: bool,
) -> Result<()> {
    let parsed_cores = parse_cpu_range_string(&cores)?;
    let boot_params = CpuAffinityManager::generate_boot_params(&parsed_cores, hugepages_1g.unwrap_or(0));

    if json {
        let res = serde_json::json!({
            "cores": cores,
            "parsed_cores": parsed_cores,
            "hugepages_1g": hugepages_1g,
            "isolcpus": boot_params.isolcpus,
            "nohz_full": boot_params.nohz_full,
            "rcu_nocbs": boot_params.rcu_nocbs,
            "hugepages": boot_params.hugepages,
            "boot_arguments": boot_params.full_cmdline,
        });
        println!("{}", serde_json::to_string_pretty(&res)?);
        return Ok(());
    }

    println!();
    println!("  LINUX KERNEL BOOT ARGUMENTS FOR ZERO-JITTER SCHEDULING");
    println!("  =====================================================");
    println!();
    println!("  To eliminate kernel tick jitter, RCU callbacks, and scheduler interrupts");
    println!("  from isolated game server cores, append the following to GRUB_CMDLINE_LINUX:");
    println!();
    println!("  {}", boot_params.full_cmdline);
    println!();
    println!("  Components:");
    println!("    Core Isolation  : {}", boot_params.isolcpus);
    println!("    Adaptive-Ticks  : {}", boot_params.nohz_full);
    println!("    RCU Callbacks   : {}", boot_params.rcu_nocbs);
    println!("    Hugepage Reserves: {}", boot_params.hugepages);
    println!();
    println!("  Instructions:");
    println!("  1. Edit /etc/default/grub (as root).");
    println!("  2. Append the parameter string above to GRUB_CMDLINE_LINUX_DEFAULT.");
    println!("  3. Update bootloader: sudo update-grub (Debian/Ubuntu) or sudo grub2-mkconfig -o /boot/grub2/grub.cfg (RHEL).");
    println!("  4. Reboot system to enable isolated CPU zero-jitter real-time cores.");
    println!();

    Ok(())
}
