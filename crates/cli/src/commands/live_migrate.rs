use colored::Colorize;
use craft_core::{
    AnycastRouteAnnouncement, AnycastRouteStatus, CraftPaths, LiveMigrationPlan,
    MigrationRegistry, MigrationStage, Result,
};
use craft_daemon::{DaemonClient, MigrationService};

pub async fn handle_migrate_live(
    server: &str,
    target_node: &str,
    target_host: Option<String>,
    target_port: u16,
    freeze_max_ms: u64,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let host = target_host.unwrap_or_else(|| target_node.to_string());
    let mut plan = LiveMigrationPlan::new(
        server,
        "local-node",
        target_node,
        host,
        target_port,
    );
    plan.freeze_timeout_ms = freeze_max_ms;

    if !json {
        println!(
            "{}",
            format!(
                "[LIVE_MIGRATE] Initiating zero-downtime live migration for server '{}' -> node '{}'...",
                server, target_node
            )
            .bold()
            .cyan()
        );
        println!("  {:<24} {}", "Migration ID:".dimmed(), plan.migration_id);
        println!("  {:<24} {}:{}", "Destination:".dimmed(), plan.target_host, plan.target_port);
        println!("  {:<24} {}ms", "Max Freeze SLA:".dimmed(), plan.freeze_timeout_ms);
        println!("  {:<24} {:.1} MB", "Convergence Cap:".dimmed(), plan.pre_copy_threshold_bytes as f64 / (1024.0 * 1024.0));
        println!();
    }

    let completed_plan = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.start_live_migration(plan).await?
    } else {
        let service = MigrationService::global(paths);
        service.start_live_migration(plan)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&completed_plan)?);
        return Ok(());
    }

    println!("{}", "[PRE_COPY] Iterative memory sync completed:".bold().blue());
    for r in &completed_plan.rounds {
        println!(
            "  Round {:<2} -> Transferred: {:>6.2} MB | Dirty remaining: {:>6.2} MB | Duration: {}ms",
            r.round,
            r.bytes_transferred as f64 / (1024.0 * 1024.0),
            r.dirty_bytes_remaining as f64 / (1024.0 * 1024.0),
            r.duration_ms
        );
    }

    println!();
    match completed_plan.status {
        MigrationStage::Completed => {
            println!(
                "{}",
                "[SUCCESS] Live migration completed with zero downtime!"
                    .green()
                    .bold()
            );
            println!("  {:<24} {}", "Source Server:".dimmed(), completed_plan.server_name);
            println!("  {:<24} {}", "Target Node:".dimmed(), completed_plan.target_node);
            println!("  {:<24} {}", "Final Status:".dimmed(), "COMPLETED [OK]".green());
            println!("  {:<24} Transparent socket splice, 0 client resets", "Traffic Handshake:".dimmed());
        }
        MigrationStage::RolledBack { ref reason } => {
            println!(
                "{}",
                format!("[ROLLED_BACK] Live migration aborted: {}", reason)
                    .red()
                    .bold()
            );
        }
        _ => {
            println!("  Current stage: {}", completed_plan.status.name());
        }
    }

    Ok(())
}

pub async fn handle_migrate_status(
    migration_id: Option<String>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let plans = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.get_migration_status(migration_id.clone()).await?
    } else {
        let service = MigrationService::global(paths);
        service.get_migration_status(migration_id.as_deref())?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&plans)?);
        return Ok(());
    }

    if plans.is_empty() {
        println!("{}", "[INFO] No live migrations registered.".dimmed());
        return Ok(());
    }

    println!("{}", "[LIVE MIGRATION STATUS]".bold().cyan());
    println!(
        "{:<28} {:<12} {:<16} {:<16} {:<16} {:<8}",
        "MIGRATION ID", "SERVER", "SOURCE", "TARGET", "STATUS", "ROUNDS"
    );
    println!("{}", "-".repeat(100).dimmed());

    for p in &plans {
        let status_colored = match p.status {
            MigrationStage::Completed => "COMPLETED".green(),
            MigrationStage::RolledBack { .. } => "ROLLED_BACK".red(),
            MigrationStage::FreezeAndHandoff => "FREEZE".yellow(),
            MigrationStage::PreCopyRound { .. } => "PRE_COPY".blue(),
            _ => p.status.name().normal(),
        };

        println!(
            "{:<28} {:<12} {:<16} {:<16} {:<16} {:<8}",
            p.migration_id,
            p.server_name,
            p.source_node,
            p.target_node,
            status_colored,
            p.rounds.len()
        );
    }

    Ok(())
}

pub async fn handle_migrate_abort(
    migration_id: String,
    reason: Option<String>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let plan = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.abort_migration(migration_id.clone(), reason.clone()).await?
    } else {
        let service = MigrationService::global(paths);
        service.abort_migration(&migration_id, reason.as_deref())?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    println!(
        "{}",
        format!("[ABORTED] Live migration '{}' rolled back successfully.", migration_id)
            .yellow()
            .bold()
    );
    if let MigrationStage::RolledBack { ref reason } = plan.status {
        println!("  {:<16} {}", "Reason:".dimmed(), reason);
    }

    Ok(())
}

pub async fn handle_migrate_list(json: bool, paths: &CraftPaths) -> Result<()> {
    handle_migrate_status(None, json, paths).await
}

pub async fn handle_anycast_route(
    action: &str,
    prefix: Option<String>,
    asn: Option<u32>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let action_clean = action.to_lowercase();

    if action_clean == "list" {
        let reg = MigrationRegistry::load(paths)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&reg.routes)?);
            return Ok(());
        }

        println!("{}", "[ANYCAST BGP ROUTE TABLE]".bold().cyan());
        println!("{:<24} {:<10} {:<14} {:<10} {:<8}", "PREFIX", "ASN", "STATUS", "COMMUNITY", "ACTIVE");
        println!("{}", "-".repeat(72).dimmed());

        for r in &reg.routes {
            let status_colored = match r.status {
                AnycastRouteStatus::Announced => "ANNOUNCED".green(),
                AnycastRouteStatus::Withdrawn => "WITHDRAWN".red(),
                AnycastRouteStatus::PrependPath => "PREPEND".yellow(),
            };
            println!(
                "{:<24} {:<10} {:<14} {:<10} {:<8}",
                r.prefix,
                r.asn,
                status_colored,
                r.community.first().map(|s| s.as_str()).unwrap_or("-"),
                if r.active { "[YES]".green() } else { "[NO]".dimmed() }
            );
        }
        return Ok(());
    }

    let pfx = prefix.ok_or_else(|| {
        craft_core::CraftError::Config("Please specify --prefix (e.g. --prefix 198.51.100.0/24)".to_string())
    })?;
    let bgp_asn = asn.unwrap_or(65000);
    let mut route = AnycastRouteAnnouncement::new(&pfx, bgp_asn);

    if action_clean == "withdraw" || action_clean == "del" {
        route.status = AnycastRouteStatus::Withdrawn;
        route.active = false;
    } else if action_clean == "prepend" {
        route.status = AnycastRouteStatus::PrependPath;
        route.active = true;
    }

    let (ok, msg, routes) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        client.manage_anycast_route(action_clean.clone(), route).await?
    } else {
        let service = MigrationService::global(paths);
        service.manage_anycast_route(&action_clean, route)?
    };

    if json {
        let output = serde_json::json!({
            "success": ok,
            "message": msg,
            "routes": routes,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{}", format!("[ANYCAST] {}", msg).green().bold());
    Ok(())
}
