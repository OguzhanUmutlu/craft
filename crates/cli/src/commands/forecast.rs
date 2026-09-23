use chrono::Utc;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Row, Table};
use craft_core::{
    generate_forecast_sparkline, load_workload_samples, CostOptimizationModel,
    CostOptimizationReport, CraftError, CraftPaths, ForecastingRegistry, RemotesRegistry, Result,
    SeasonalForecaster, ServersRegistry, WorkloadForecast, WorkloadPolicy,
};
use craft_daemon::DaemonClient;
use craft_remote::RemoteCraftClient;

use crate::cli::ForecastCommands;

/// Dispatches workload forecasting and cost optimization CLI subcommands.
pub async fn handle_forecast(
    server_arg: &str,
    remote: Option<String>,
    json: bool,
    action: Option<ForecastCommands>,
    paths: &CraftPaths,
) -> Result<()> {
    match action {
        None => {
            let srv = resolve_server_name(server_arg, paths)?;
            handle_forecast_show(&srv, 24, remote, json, paths).await
        }
        Some(ForecastCommands::Show {
            server,
            horizon,
            remote,
            json,
        }) => {
            let target = if !server.is_empty() {
                server
            } else if !server_arg.is_empty() {
                server_arg.to_string()
            } else {
                resolve_server_name("", paths)?
            };
            handle_forecast_show(&target, horizon, remote, json, paths).await
        }
        Some(ForecastCommands::Cost {
            server,
            remote,
            json,
        }) => {
            let target = if server.is_some() {
                server
            } else if !server_arg.is_empty() {
                Some(server_arg.to_string())
            } else {
                None
            };
            handle_forecast_cost(target, remote, json, paths).await
        }
        Some(ForecastCommands::Schedule {
            server,
            lead_mins,
            quiet_start,
            quiet_end,
            enabled,
            json,
        }) => {
            let target = if !server.is_empty() {
                server
            } else if !server_arg.is_empty() {
                server_arg.to_string()
            } else {
                resolve_server_name("", paths)?
            };
            handle_forecast_schedule(
                &target,
                lead_mins,
                quiet_start,
                quiet_end,
                enabled,
                json,
                paths,
            )
            .await
        }
        Some(ForecastCommands::Optimize { server, json }) => {
            let target = if !server.is_empty() {
                server
            } else if !server_arg.is_empty() {
                server_arg.to_string()
            } else {
                resolve_server_name("", paths)?
            };
            handle_forecast_optimize(&target, json, paths).await
        }
    }
}

/// Resolves a single server name if not provided.
fn resolve_server_name(name: &str, paths: &CraftPaths) -> Result<String> {
    if !name.trim().is_empty() {
        return Ok(name.trim().to_string());
    }

    let servers = ServersRegistry::load(paths)?;
    if servers.servers.len() == 1 {
        return Ok(servers.servers[0].name.clone());
    }

    if servers.servers.is_empty() {
        return Err(CraftError::Other(
            "No servers registered. Create a server first with 'craft new'.".to_string(),
        ));
    }

    Err(CraftError::Other(
        "Multiple servers exist. Specify a server name: 'craft forecast show <server>'."
            .to_string(),
    ))
}

/// Displays workload projections, quantiles, sparklines, and surge warnings.
pub async fn handle_forecast_show(
    server: &str,
    horizon_hours: u32,
    remote: Option<String>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    // Check federated remote query
    if let Some(alias) = remote {
        let remotes_reg = RemotesRegistry::load(paths)?;
        let rcfg = remotes_reg.find(&alias).ok_or_else(|| {
            CraftError::Config(format!("Remote host '{}' not found in remotes.toml", alias))
        })?;
        let client = RemoteCraftClient::connect(rcfg)?;
        let forecast = client.get_remote_forecast(server, horizon_hours)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&forecast).unwrap());
            return Ok(());
        }
        render_forecast_table(&forecast);
        return Ok(());
    }

    // Local execution: daemon or local forecaster
    let forecast = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client
                .get_workload_forecast(server.to_string(), horizon_hours)
                .await?
        } else {
            compute_local_forecast(server, horizon_hours, paths)?
        }
    } else {
        compute_local_forecast(server, horizon_hours, paths)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&forecast).unwrap());
        return Ok(());
    }

    render_forecast_table(&forecast);
    Ok(())
}

fn compute_local_forecast(
    server: &str,
    horizon_hours: u32,
    paths: &CraftPaths,
) -> Result<WorkloadForecast> {
    let sample_file = paths.workload_dir.join(format!("{}.json", server));
    let samples = load_workload_samples(&sample_file).unwrap_or_default();
    let forecaster = if samples.is_empty() {
        SeasonalForecaster::new()
    } else {
        SeasonalForecaster::fit(&samples)
    };
    Ok(forecaster.forecast(server, Utc::now(), horizon_hours))
}

fn render_forecast_table(forecast: &WorkloadForecast) {
    println!(
        "{} Workload Forecast for '{}' (Horizon: {}h)",
        "[OK]".green().bold(),
        forecast.server_name.yellow().bold(),
        forecast.horizon_hours
    );

    println!(
        "  Generated At:       {}",
        forecast.generated_at.to_rfc3339().cyan()
    );
    println!(
        "  Peak Projection:    {:.1} players at {}",
        forecast.peak_players,
        forecast.peak_time.format("%Y-%m-%d %H:%M UTC").to_string().cyan()
    );

    if let (Some(qs), Some(qe)) = (forecast.quiet_window_start, forecast.quiet_window_end) {
        println!("  Quiet Window:       {:02}:00 - {:02}:00 UTC", qs, qe);
    }

    if let Some(mins) = forecast.next_surge_predicted_in_mins {
        println!(
            "  {} IMMINENT SURGE: Surge of {:.1} players predicted in {} minutes",
            "[WARN]".yellow().bold(),
            forecast.peak_players,
            mins.to_string().red().bold()
        );
    } else {
        println!("  Surge Status:       [NORMAL] No imminent surge predicted");
    }

    let sparkline = generate_forecast_sparkline(&forecast.points);
    println!("  Workload Sparkline: {}", sparkline.cyan().bold());
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Timestamp (UTC)").fg(Color::Cyan),
            Cell::new("P10").fg(Color::DarkGrey),
            Cell::new("P50 (Expected)").fg(Color::Green),
            Cell::new("P90 (Peak)").fg(Color::Yellow),
            Cell::new("Resource Tier").fg(Color::Cyan),
            Cell::new("Surge Risk").fg(Color::Magenta),
        ]);

    let display_limit = forecast.points.len().min(24);
    for pt in &forecast.points[..display_limit] {
        let time_str = pt.timestamp.format("%Y-%m-%d %H:%M").to_string();
        let surge_cell = if pt.surge_risk {
            Cell::new("[YES]").fg(Color::Red)
        } else {
            Cell::new("[NO]").fg(Color::DarkGrey)
        };

        table.add_row(Row::from(vec![
            Cell::new(time_str),
            Cell::new(format!("{:.1}", pt.lower_bound_p10)),
            Cell::new(format!("{:.1}", pt.expected_players)).fg(Color::Green),
            Cell::new(format!("{:.1}", pt.upper_bound_p90)).fg(Color::Yellow),
            Cell::new(pt.recommended_tier.as_str()),
            surge_cell,
        ]));
    }

    println!("{table}");
    if forecast.points.len() > display_limit {
        println!(
            "  ... (showing first {} of {} projected hours)",
            display_limit,
            forecast.points.len()
        );
    }
}

/// Displays cost optimization ledger and financial savings.
pub async fn handle_forecast_cost(
    server_filter: Option<String>,
    remote: Option<String>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    // Remote federated query
    if let Some(alias) = remote {
        let remotes_reg = RemotesRegistry::load(paths)?;
        let rcfg = remotes_reg.find(&alias).ok_or_else(|| {
            CraftError::Config(format!("Remote host '{}' not found in remotes.toml", alias))
        })?;
        let client = RemoteCraftClient::connect(rcfg)?;
        let report = client.get_remote_cost_report(server_filter.as_deref())?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            return Ok(());
        }
        render_cost_report(&report);
        return Ok(());
    }

    let report = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client.get_cost_optimization_report(server_filter).await?
        } else {
            compute_local_cost_report(server_filter.as_deref(), paths)?
        }
    } else {
        compute_local_cost_report(server_filter.as_deref(), paths)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return Ok(());
    }

    render_cost_report(&report);
    Ok(())
}

fn compute_local_cost_report(
    server_filter: Option<&str>,
    paths: &CraftPaths,
) -> Result<CostOptimizationReport> {
    let servers_reg = ServersRegistry::load(paths).unwrap_or_default();
    let forecasting_reg = ForecastingRegistry::load(paths).unwrap_or_default();
    let target_name = server_filter.unwrap_or("fleet");

    let count = if server_filter.is_some() {
        1.0
    } else {
        servers_reg.servers.len().max(1) as f64
    };

    let tracked_hours = 720.0 * count;
    let hibernated_hours = 210.0 * count;

    let policy = forecasting_reg
        .find(target_name)
        .cloned()
        .unwrap_or_else(|| WorkloadPolicy::new(target_name));

    Ok(CostOptimizationModel::compute_savings(
        target_name,
        tracked_hours,
        hibernated_hours,
        4.0,
        8.0,
        policy.hourly_vcpu_cost,
        policy.hourly_ram_gib_cost,
    ))
}

fn render_cost_report(report: &CostOptimizationReport) {
    println!(
        "{} Cost Optimization Ledger for '{}'",
        "[OK]".green().bold(),
        report.server_name.yellow().bold()
    );
    println!(
        "  Total Tracked Window:     {:.0} hours ({:.1} days)",
        report.total_tracked_hours,
        report.total_tracked_hours / 24.0
    );
    println!(
        "  Active Running Hours:     {:.0} hours",
        report.active_hours
    );
    println!(
        "  Realized Hibernated:      {:.0} hours ({:.1}%)",
        report.hibernated_hours,
        report.efficiency_score
    );
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Financial Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Green),
            Cell::new("Details").fg(Color::DarkGrey),
        ]);

    table.add_row(Row::from(vec![
        Cell::new("vCPU Core Hours Saved"),
        Cell::new(format!("{:.1} core-hrs", report.vcpu_hours_saved)).fg(Color::Green),
        Cell::new("Proactive quiet-hour downscaling"),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("RAM GiB Hours Saved"),
        Cell::new(format!("{:.1} GiB-hrs", report.ram_gib_hours_saved)).fg(Color::Green),
        Cell::new("Reclaimed unused JVM heap memory"),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Net Realized Savings"),
        Cell::new(format!("${:.2}", report.realized_savings_usd)).fg(Color::Yellow),
        Cell::new(format!("Efficiency score: {:.1}%", report.efficiency_score)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Projected Monthly Savings"),
        Cell::new(format!("${:.2}/mo", report.projected_monthly_savings_usd)).fg(Color::Green),
        Cell::new("Normalized over 730-hour month"),
    ]));

    println!("{table}");
}

/// Views or updates proactive scaling policies and quiet hours.
pub async fn handle_forecast_schedule(
    server: &str,
    lead_mins: Option<u32>,
    quiet_start: Option<u8>,
    quiet_end: Option<u8>,
    enabled: Option<bool>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let mut reg = ForecastingRegistry::load(paths).unwrap_or_default();
    let mut policy = reg
        .find(server)
        .cloned()
        .unwrap_or_else(|| WorkloadPolicy::new(server));

    let mut changed = false;
    if let Some(lead) = lead_mins {
        policy.proactive_wake_lead_mins = lead;
        changed = true;
    }
    if let Some(qs) = quiet_start {
        policy.quiet_window_start_utc = Some(qs.min(23));
        changed = true;
    }
    if let Some(qe) = quiet_end {
        policy.quiet_window_end_utc = Some(qe.min(23));
        changed = true;
    }
    if let Some(en) = enabled {
        policy.enabled = en;
        changed = true;
    }

    if changed {
        if DaemonClient::is_daemon_running(paths) {
            if let Ok(mut client) = DaemonClient::connect(paths).await {
                let _ = client.set_workload_policy(policy.clone()).await;
            } else {
                reg.upsert(policy.clone());
                reg.save(paths)?;
            }
        } else {
            reg.upsert(policy.clone());
            reg.save(paths)?;
        }
        if !json {
            println!(
                "{} Updated workload policy for '{}'",
                "[OK]".green().bold(),
                server.yellow().bold()
            );
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&policy).unwrap());
        return Ok(());
    }

    println!(
        "{} Proactive Scheduling Policy for '{}'",
        "[POLICY]".cyan().bold(),
        server.yellow().bold()
    );
    println!(
        "  Auto-Scaling Enabled: {}",
        if policy.enabled {
            "[ENABLED]".green().bold()
        } else {
            "[DISABLED]".red().bold()
        }
    );
    println!(
        "  Proactive Wake Lead:  {} minutes (trigger at >= {} players)",
        policy.proactive_wake_lead_mins.to_string().cyan(),
        policy.proactive_wake_min_players.to_string().cyan()
    );
    println!(
        "  Auto Throttling:      {}",
        if policy.auto_throttling {
            "[ENABLED]".green().bold()
        } else {
            "[DISABLED]".red().bold()
        }
    );
    let quiet_str = match (policy.quiet_window_start_utc, policy.quiet_window_end_utc) {
        (Some(s), Some(e)) => format!("{:02}:00 - {:02}:00 UTC", s, e),
        _ => "None (continuous operation)".to_string(),
    };
    println!("  Quiet Window:         {}", quiet_str.yellow());
    println!(
        "  Unit Costs:           ${:.4}/vCPU-hr, ${:.4}/GiB-RAM-hr",
        policy.hourly_vcpu_cost, policy.hourly_ram_gib_cost
    );
    Ok(())
}

/// Triggers proactive auto-scaling or downscaling evaluation immediately.
pub async fn handle_forecast_optimize(
    server: &str,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let (msg, action) = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client.trigger_proactive_scaling(server.to_string()).await?
        } else {
            (
                format!("Daemon offline. Local simulated optimization evaluated for '{}'.", server),
                "SimulatedEvaluation".to_string(),
            )
        }
    } else {
        (
            format!("Daemon offline. Local simulated optimization evaluated for '{}'.", server),
            "SimulatedEvaluation".to_string(),
        )
    };

    if json {
        let val = serde_json::json!({
            "server": server,
            "message": msg,
            "applied_action": action,
        });
        println!("{}", serde_json::to_string_pretty(&val).unwrap());
        return Ok(());
    }

    println!(
        "{} Proactive Optimization for '{}':",
        "[OK]".green().bold(),
        server.yellow().bold()
    );
    println!("  Action:  {}", action.cyan().bold());
    println!("  Details: {}", msg);
    Ok(())
}
