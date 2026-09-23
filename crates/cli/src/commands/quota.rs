use crate::cli::{QuotaCommands, TenantQuotaCommands};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{
    format_size, CraftError, CraftPaths, QuotaRegistry, QuotaUsageSummary, Result, ServerPriority,
    ServerResourceLimit, TenantQuota,
};
use craft_daemon::{DaemonClient, QuotaService};
use serde_json::json;

/// Entry point for `craft quota` / `craft cgroup` subcommands
pub async fn handle_quota(cmd: QuotaCommands, paths: &CraftPaths) -> Result<()> {
    match cmd {
        QuotaCommands::List { tenant, json } => handle_list(paths, tenant.as_deref(), json).await,
        QuotaCommands::Get { server, json } => handle_get(paths, &server, json).await,
        QuotaCommands::Set {
            server,
            cpu,
            memory,
            memory_high,
            cpu_weight,
            io_weight,
            pids_max,
            priority,
            tenant,
            json,
        } => {
            handle_set(
                paths,
                &server,
                cpu,
                memory,
                memory_high,
                cpu_weight,
                io_weight,
                pids_max,
                priority.as_deref(),
                tenant.as_deref(),
                json,
            )
            .await
        }
        QuotaCommands::Tenant { action } => handle_tenant(paths, action).await,
        QuotaCommands::Balance { json } => handle_balance(paths, json).await,
    }
}

async fn handle_list(paths: &CraftPaths, tenant: Option<&str>, as_json: bool) -> Result<()> {
    let items = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .list_quota_usage(tenant.map(str::to_string))
            .await
            .unwrap_or_else(|_| QuotaService::list_quota_usage(paths, tenant).unwrap_or_default())
    } else {
        QuotaService::list_quota_usage(paths, tenant)?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if items.is_empty() {
        println!("[OK] No resource quota limits configured. Servers run unconstrained.");
        return Ok(());
    }

    println!("[OK] Resource Quotas & Cgroups v2 Utilization");

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Server").fg(Color::Cyan),
            Cell::new("Tenant").fg(Color::Cyan),
            Cell::new("Priority").fg(Color::Cyan),
            Cell::new("CPU Max").fg(Color::Cyan),
            Cell::new("CPU Wgt").fg(Color::Cyan),
            Cell::new("Mem High").fg(Color::Cyan),
            Cell::new("Mem Max").fg(Color::Cyan),
            Cell::new("Current RSS").fg(Color::Cyan),
            Cell::new("Throttle %").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
        ]);

    for item in &items {
        let cpu_max_str = match item.limits.cpu_max_quota {
            Some(q) => format!("{}%", q),
            None => "unlimited".to_string(),
        };

        let mem_high_str = match item.limits.memory_high_bytes {
            Some(b) => format_size(b),
            None => "-".to_string(),
        };

        let mem_max_str = match item.limits.memory_max_bytes {
            Some(b) => format_size(b),
            None => "unlimited".to_string(),
        };

        let cur_rss_str = if item.stats.memory_current_bytes > 0 {
            format_size(item.stats.memory_current_bytes)
        } else {
            "0 B".to_string()
        };

        let throttle_pct_str = format!("{:.1}%", item.stats.cpu_throttle_ratio * 100.0);

        let status_color = match item.health_indicator.as_str() {
            "[PRISTINE]" => Color::Green,
            "[NORMAL]" => Color::White,
            "[THROTTLED]" => Color::Yellow,
            "[OOM_RISK]" => Color::Red,
            _ => Color::White,
        };

        table.add_row(vec![
            Cell::new(&item.server_name).fg(Color::White),
            Cell::new(item.tenant_id.as_deref().unwrap_or("default")),
            Cell::new(item.limits.priority.name()),
            Cell::new(cpu_max_str),
            Cell::new(item.limits.cpu_weight.to_string()),
            Cell::new(mem_high_str),
            Cell::new(mem_max_str),
            Cell::new(cur_rss_str),
            Cell::new(throttle_pct_str),
            Cell::new(&item.health_indicator).fg(status_color),
        ]);
    }

    println!("{table}");
    Ok(())
}

async fn handle_get(paths: &CraftPaths, server: &str, as_json: bool) -> Result<()> {
    let summary = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .get_server_quota(server.to_string())
            .await
            .unwrap_or_else(|_| QuotaService::get_server_quota(paths, server).unwrap_or_else(|_| {
                QuotaUsageSummary {
                    server_name: server.to_string(),
                    tenant_id: None,
                    active: false,
                    pid: None,
                    limits: ServerResourceLimit::new(server),
                    stats: Default::default(),
                    throttled: false,
                    health_indicator: "[PRISTINE]".to_string(),
                }
            }))
    } else {
        QuotaService::get_server_quota(paths, server)?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("[OK] Cgroups v2 Quota & Resource Status: {}", summary.server_name);
    println!("  Tenant:              {}", summary.tenant_id.as_deref().unwrap_or("default"));
    println!("  Priority Tier:       {}", summary.limits.priority.name());
    println!(
        "  Active Process:      {}",
        if summary.active {
            format!("Yes (PID: {})", summary.pid.unwrap_or(0))
        } else {
            "No (Stopped)".to_string()
        }
    );
    println!(
        "  CPU Max Quota:       {}",
        summary
            .limits
            .cpu_max_quota
            .map(|q| format!("{}% ({} cores)", q, q as f64 / 100.0))
            .unwrap_or_else(|| "unlimited".to_string())
    );
    println!("  CFS CPU Weight:      {} / 10000", summary.limits.cpu_weight);
    println!("  BFQ I/O Weight:      {} / 10000", summary.limits.io_weight);
    println!(
        "  Memory High:         {}",
        summary
            .limits
            .memory_high_bytes
            .map(format_size)
            .unwrap_or_else(|| "unlimited".to_string())
    );
    println!(
        "  Memory Max:          {}",
        summary
            .limits
            .memory_max_bytes
            .map(format_size)
            .unwrap_or_else(|| "unlimited".to_string())
    );
    println!(
        "  Tasks Limit (pids):  {}",
        summary
            .limits
            .pids_max
            .map(|p| p.to_string())
            .unwrap_or_else(|| "unlimited".to_string())
    );
    println!("  Current Memory RSS:  {}", format_size(summary.stats.memory_current_bytes));
    println!("  Anon Memory:         {}", format_size(summary.stats.memory_anon_bytes));
    println!("  File Memory:         {}", format_size(summary.stats.memory_file_bytes));
    println!("  CPU Usage Time:      {:.2}s", summary.stats.cpu_usage_usec as f64 / 1_000_000.0);
    println!(
        "  CPU Throttled Time:  {:.2}s ({:.1}%)",
        summary.stats.cpu_throttled_usec as f64 / 1_000_000.0,
        summary.stats.cpu_throttle_ratio * 100.0
    );
    println!("  Throttled Periods:   {}", summary.stats.cpu_nr_throttled);
    println!("  OOM Events:          {}", summary.stats.memory_oom_events);
    println!("  OOM Kills:           {}", summary.stats.memory_oom_kill_events);
    println!("  High Breach Events:  {}", summary.stats.memory_high_events);
    println!("  Health Grade:        {}", summary.health_indicator);

    Ok(())
}

async fn handle_set(
    paths: &CraftPaths,
    server: &str,
    cpu: Option<u32>,
    memory_mb: Option<u64>,
    memory_high_mb: Option<u64>,
    cpu_weight: Option<u32>,
    io_weight: Option<u32>,
    pids_max: Option<u32>,
    priority_str: Option<&str>,
    tenant: Option<&str>,
    as_json: bool,
) -> Result<()> {
    let registry = QuotaRegistry::load(paths).unwrap_or_default();
    let mut limit = registry
        .get_server_limit(server)
        .cloned()
        .unwrap_or_else(|| ServerResourceLimit::new(server));

    if let Some(c) = cpu {
        limit.cpu_max_quota = Some(c);
    }
    if let Some(m) = memory_mb {
        limit.memory_max_bytes = Some(m.saturating_mul(1024 * 1024));
    }
    if let Some(mh) = memory_high_mb {
        limit.memory_high_bytes = Some(mh.saturating_mul(1024 * 1024));
    }
    if let Some(cw) = cpu_weight {
        limit.cpu_weight = cw.clamp(1, 10000);
    }
    if let Some(iw) = io_weight {
        limit.io_weight = iw.clamp(1, 10000);
    }
    if let Some(p) = pids_max {
        limit.pids_max = Some(p);
    }
    if let Some(pr) = priority_str {
        if let Some(priority) = ServerPriority::from_str_opt(pr) {
            limit.priority = priority;
            if cpu_weight.is_none() {
                limit.cpu_weight = priority.default_cpu_weight();
            }
            if io_weight.is_none() {
                limit.io_weight = priority.default_io_weight();
            }
        } else {
            return Err(CraftError::Config(format!(
                "Invalid priority '{}'. Supported: gateway, standard, worker, batch",
                pr
            )));
        }
    }
    if let Some(t) = tenant {
        limit.tenant_id = Some(t.to_string());
    }

    let summary = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .set_server_quota(limit.clone())
            .await
            .unwrap_or_else(|_| QuotaService::set_server_quota(paths, limit).unwrap())
    } else {
        QuotaService::set_server_quota(paths, limit)?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!(
        "[OK] Resource quota limits applied for server '{}' (Priority: {})",
        server,
        summary.limits.priority.name()
    );
    if let Some(q) = summary.limits.cpu_max_quota {
        println!("  CPU Quota:   {}%", q);
    }
    if let Some(m) = summary.limits.memory_max_bytes {
        println!("  Memory Max:  {}", format_size(m));
    }
    if let Some(h) = summary.limits.memory_high_bytes {
        println!("  Memory High: {}", format_size(h));
    }
    println!("  CPU Weight:  {}", summary.limits.cpu_weight);
    println!("  I/O Weight:  {}", summary.limits.io_weight);

    Ok(())
}

async fn handle_tenant(paths: &CraftPaths, action: TenantQuotaCommands) -> Result<()> {
    match action {
        TenantQuotaCommands::List { json } => {
            let registry = QuotaRegistry::load(paths)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&registry.tenants)?);
                return Ok(());
            }

            println!("[OK] Tenant Quotas & Allocation Ledger");

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Tenant ID").fg(Color::Cyan),
                    Cell::new("Max Servers").fg(Color::Cyan),
                    Cell::new("Used Servers").fg(Color::Cyan),
                    Cell::new("Max Mem (MB)").fg(Color::Cyan),
                    Cell::new("Used Mem (MB)").fg(Color::Cyan),
                    Cell::new("Max CPU (%)").fg(Color::Cyan),
                    Cell::new("Used CPU (%)").fg(Color::Cyan),
                    Cell::new("Burst").fg(Color::Cyan),
                ]);

            for (id, t) in &registry.tenants {
                let (count, used_mem, used_cpu) = registry.calculate_tenant_usage(id);
                table.add_row(vec![
                    Cell::new(id).fg(Color::White),
                    Cell::new(t.max_servers.to_string()),
                    Cell::new(count.to_string()),
                    Cell::new((t.max_memory_bytes / (1024 * 1024)).to_string()),
                    Cell::new((used_mem / (1024 * 1024)).to_string()),
                    Cell::new(t.max_cpu_percent.to_string()),
                    Cell::new(used_cpu.to_string()),
                    Cell::new(if t.allow_burst { "Allowed" } else { "No" }),
                ]);
            }

            println!("{table}");
            Ok(())
        }
        TenantQuotaCommands::Get { tenant, json } => {
            let (quota, used_mem_mb, used_cpu_percent, count) =
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    client
                        .get_tenant_quota(tenant.clone())
                        .await
                        .unwrap_or_else(|_| QuotaService::get_tenant_quota(paths, &tenant).unwrap())
                } else {
                    QuotaService::get_tenant_quota(paths, &tenant)?
                };

            if json {
                println!(
                    "{}",
                    json!({
                        "quota": quota,
                        "used_memory_mb": used_mem_mb,
                        "used_cpu_percent": used_cpu_percent,
                        "server_count": count
                    })
                );
                return Ok(());
            }

            println!("[OK] Tenant Quota Profile: {}", quota.tenant_id);
            println!("  Servers:       {} / {}", count, quota.max_servers);
            println!(
                "  Memory:        {} MB / {} MB",
                used_mem_mb,
                quota.max_memory_bytes / (1024 * 1024)
            );
            println!(
                "  CPU Quota:     {}% / {}%",
                used_cpu_percent, quota.max_cpu_percent
            );
            println!(
                "  Storage Limit: {}",
                format_size(quota.max_storage_bytes)
            );
            println!(
                "  Allow Burst:   {}",
                if quota.allow_burst { "Yes" } else { "No" }
            );

            Ok(())
        }
        TenantQuotaCommands::Set {
            tenant,
            max_servers,
            max_memory,
            max_cpu,
            max_storage_gb,
            allow_burst,
            json,
        } => {
            let registry = QuotaRegistry::load(paths).unwrap_or_default();
            let mut q = registry
                .get_tenant_quota(&tenant)
                .cloned()
                .unwrap_or_else(|| TenantQuota::new(&tenant, 10, 16384, 400));

            if let Some(s) = max_servers {
                q.max_servers = s;
            }
            if let Some(m) = max_memory {
                q.max_memory_bytes = m.saturating_mul(1024 * 1024);
            }
            if let Some(c) = max_cpu {
                q.max_cpu_percent = c;
            }
            if let Some(st) = max_storage_gb {
                q.max_storage_bytes = st.saturating_mul(1024 * 1024 * 1024);
            }
            if let Some(b) = allow_burst {
                q.allow_burst = b;
            }

            if let Ok(mut client) = DaemonClient::connect(paths).await {
                client
                    .set_tenant_quota(q.clone())
                    .await
                    .unwrap_or_else(|_| QuotaService::set_tenant_quota(paths, q.clone()).unwrap());
            } else {
                QuotaService::set_tenant_quota(paths, q.clone())?;
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&q)?);
                return Ok(());
            }

            println!("[OK] Tenant quota updated for '{}'", tenant);
            println!("  Max Servers: {}", q.max_servers);
            println!("  Max Memory:  {} MB", q.max_memory_bytes / (1024 * 1024));
            println!("  Max CPU:     {}%", q.max_cpu_percent);
            println!("  Allow Burst: {}", if q.allow_burst { "Yes" } else { "No" });

            Ok(())
        }
    }
}

async fn handle_balance(paths: &CraftPaths, as_json: bool) -> Result<()> {
    let (rebalanced_count, message) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client
            .enforce_fair_share_now()
            .await
            .unwrap_or_else(|_| QuotaService::enforce_fair_share(paths).unwrap())
    } else {
        QuotaService::enforce_fair_share(paths)?
    };

    if as_json {
        println!(
            "{}",
            json!({
                "rebalanced_count": rebalanced_count,
                "message": message
            })
        );
        return Ok(());
    }

    println!("{}", message);
    Ok(())
}
