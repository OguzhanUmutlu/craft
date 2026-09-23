use crate::cli::AiCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    AnomalySeverity, AutopilotMode, CraftError, CraftPaths, IntelligenceRegistry,
    RemediationAction, Result, ServersRegistry,
};
use craft_daemon::DaemonClient;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn handle_ai(action: AiCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        AiCommands::Status { server } => handle_status(server, paths).await,
        AiCommands::Analyze { server } => handle_analyze(&server, paths).await,
        AiCommands::Profile { server, duration } => handle_profile(&server, duration, paths).await,
        AiCommands::Remediate {
            server,
            action,
            dry_run,
        } => handle_remediate(&server, &action, dry_run, paths).await,
        AiCommands::Policy {
            server,
            mode,
            enable,
            disable,
            warn_mspt,
            crit_mspt,
        } => handle_policy(&server, mode, enable, disable, warn_mspt, crit_mspt, paths),
    }
}

async fn handle_status(server_filter: Option<String>, paths: &CraftPaths) -> Result<()> {
    println!("{}", "=== Craft Autopilot Operational Intelligence ===".cyan().bold());

    let mut client = DaemonClient::connect(paths).await.ok();
    let reports = if let Some(ref mut c) = client {
        c.get_intelligence_status(server_filter.clone()).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let servers_reg = ServersRegistry::load(paths).unwrap_or_default();
    let int_reg = IntelligenceRegistry::load(paths).unwrap_or_default();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Server").fg(Color::Cyan),
            Cell::new("Mode").fg(Color::Yellow),
            Cell::new("TPS").fg(Color::Green),
            Cell::new("MSPT (P50 / P95)").fg(Color::Magenta),
            Cell::new("Memory RSS").fg(Color::Blue),
            Cell::new("Gradient").fg(Color::Cyan),
            Cell::new("TTE Forecast").fg(Color::White),
            Cell::new("Health").fg(Color::Green),
        ]));

    for server in &servers_reg.servers {
        if let Some(ref filter) = server_filter {
            if &server.name != filter {
                continue;
            }
        }

        let policy = int_reg.get_policy(&server.name);
        let rep_opt = reports.iter().find(|r| r.server_name == server.name);

        let (tps_str, mspt_str, mem_str, grad_str, tte_str, health_cell) = if let Some(rep) = rep_opt {
            let tps = format!("{:.1}", rep.tps_current);
            let mspt = format!("{:.1} / {:.1} ms", rep.mspt_p50, rep.mspt_p95);
            let mem = format!("{:.1} MB", (rep.memory_rss_bytes as f64) / (1024.0 * 1024.0));
            let grad = format!("{:+.1} MB/m", rep.memory_growth_rate_mb_min);
            let tte = if let Some(secs) = rep.predicted_tte_seconds {
                format!("{}m", secs / 60)
            } else {
                "Stable".to_string()
            };

            let has_crit = rep.anomalies.iter().any(|a| a.severity == AnomalySeverity::Critical);
            let has_warn = rep.anomalies.iter().any(|a| a.severity == AnomalySeverity::Warning);

            let health = if has_crit {
                Cell::new("[CRITICAL]").fg(Color::Red)
            } else if has_warn {
                Cell::new("[WARNING]").fg(Color::Yellow)
            } else {
                Cell::new("[NOMINAL]").fg(Color::Green)
            };

            (tps, mspt, mem, grad, tte, health)
        } else {
            (
                "--".to_string(),
                "-- / --".to_string(),
                "--".to_string(),
                "--".to_string(),
                "--".to_string(),
                Cell::new("[OFFLINE]").fg(Color::DarkGrey),
            )
        };

        let mode_color = match policy.mode {
            AutopilotMode::Autonomous => Color::Green,
            AutopilotMode::Advisory => Color::Yellow,
            AutopilotMode::Disabled => Color::DarkGrey,
        };

        table.add_row(Row::from(vec![
            Cell::new(&server.name).fg(Color::White),
            Cell::new(policy.mode.to_string()).fg(mode_color),
            Cell::new(tps_str),
            Cell::new(mspt_str),
            Cell::new(mem_str),
            Cell::new(grad_str),
            Cell::new(tte_str),
            health_cell,
        ]));
    }

    println!("{table}");
    Ok(())
}

async fn handle_analyze(server_name: &str, paths: &CraftPaths) -> Result<()> {
    println!(
        "{} Performance analysis for '{}'...",
        "[ANALYZE]".cyan().bold(),
        server_name.yellow()
    );

    let mut client = match DaemonClient::connect(paths).await {
        Ok(c) => c,
        Err(_) => {
            return Err(CraftError::Other(
                "Daemon is not running. Please start craft-daemon to run live performance analysis."
                    .to_string(),
            ));
        }
    };

    let (_report, markdown) = client.trigger_diagnostic_run(server_name.to_string(), 15).await?;

    // Save report to ~/.craft/diagnostics/<server>/
    let server_diag_dir = paths.diagnostics_dir.join(server_name);
    fs::create_dir_all(&server_diag_dir).map_err(CraftError::Io)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let file_path = server_diag_dir.join(format!("report_{}.md", now));
    fs::write(&file_path, &markdown).map_err(CraftError::Io)?;

    println!("\n{}", markdown);
    println!(
        "\n{} Diagnostic report saved to: {}",
        "[SAVED]".green().bold(),
        file_path.display().to_string().cyan()
    );

    Ok(())
}

async fn handle_profile(server_name: &str, duration: u64, paths: &CraftPaths) -> Result<()> {
    println!(
        "{} Capturing {}s execution profile for '{}'...",
        "[PROFILE]".magenta().bold(),
        duration,
        server_name.yellow()
    );

    let mut client = match DaemonClient::connect(paths).await {
        Ok(c) => c,
        Err(_) => {
            return Err(CraftError::Other(
                "Daemon is not running. Please start craft-daemon to capture JFR profiles."
                    .to_string(),
            ));
        }
    };

    let (report, _) = client
        .trigger_diagnostic_run(server_name.to_string(), duration)
        .await?;

    println!("{} Profile complete for '{}'", "[OK]".green().bold(), server_name);
    println!("- Current MSPT: {:.2} ms | TPS: {:.2}", report.mspt_current, report.tps_current);
    println!("- Memory RSS: {:.2} MB", (report.memory_rss_bytes as f64) / (1024.0 * 1024.0));
    println!("- Active anomalies: {}", report.anomalies.len());

    Ok(())
}

async fn handle_remediate(
    server_name: &str,
    action_str: &str,
    dry_run: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let action = match action_str.to_lowercase().as_str() {
        "cull" | "entity_cull" | "entity-cull" => RemediationAction::EntityCull,
        "gc" | "gc_hint" | "gc-hint" => RemediationAction::GarbageCollectionHint,
        "restart" | "off_peak_restart" | "off-peak-restart" => RemediationAction::OffPeakRestart,
        other => {
            return Err(CraftError::Other(format!(
                "Unknown remediation action '{}'. Valid actions: cull, gc, restart",
                other
            )));
        }
    };

    println!(
        "{} Executing remediation '{}' on server '{}' (dry_run: {})...",
        "[REMEDIATE]".yellow().bold(),
        action,
        server_name.cyan(),
        dry_run
    );

    let mut client = match DaemonClient::connect(paths).await {
        Ok(c) => c,
        Err(_) => {
            return Err(CraftError::Other(
                "Daemon is not running. Please start craft-daemon to execute remediation."
                    .to_string(),
            ));
        }
    };

    let outcome = client
        .execute_remediation(server_name.to_string(), action, dry_run)
        .await?;

    println!("{} {}", "[RESULT]".green().bold(), outcome);
    Ok(())
}

fn handle_policy(
    server_name: &str,
    mode_str: Option<String>,
    enable: bool,
    disable: bool,
    warn_mspt: Option<f64>,
    crit_mspt: Option<f64>,
    paths: &CraftPaths,
) -> Result<()> {
    let mut reg = IntelligenceRegistry::load(paths)?;
    let mut policy = reg.get_policy(server_name);

    let mut modified = false;

    if enable {
        policy.mode = AutopilotMode::Advisory;
        modified = true;
    }
    if disable {
        policy.mode = AutopilotMode::Disabled;
        modified = true;
    }

    if let Some(m) = mode_str {
        match m.to_lowercase().as_str() {
            "autonomous" => {
                policy.mode = AutopilotMode::Autonomous;
                modified = true;
            }
            "advisory" => {
                policy.mode = AutopilotMode::Advisory;
                modified = true;
            }
            "disabled" => {
                policy.mode = AutopilotMode::Disabled;
                modified = true;
            }
            other => {
                return Err(CraftError::Other(format!(
                    "Invalid mode '{}'. Allowed: advisory, autonomous, disabled",
                    other
                )));
            }
        }
    }

    if let Some(w) = warn_mspt {
        policy.mspt_warning_ms = w;
        modified = true;
    }

    if let Some(c) = crit_mspt {
        policy.mspt_critical_ms = c;
        modified = true;
    }

    if modified {
        reg.set_policy(server_name.to_string(), policy.clone());
        reg.save(paths)?;
        println!(
            "{} Updated Autopilot policy for server '{}'",
            "[OK]".green().bold(),
            server_name.cyan()
        );
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Setting").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Yellow),
        ]));

    table.add_row(Row::from(vec!["Server", server_name]));
    table.add_row(Row::from(vec!["Autopilot Mode", &policy.mode.to_string()]));
    table.add_row(Row::from(vec![
        "MSPT Warning",
        &format!("{:.1} ms", policy.mspt_warning_ms),
    ]));
    table.add_row(Row::from(vec![
        "MSPT Critical",
        &format!("{:.1} ms", policy.mspt_critical_ms),
    ]));
    table.add_row(Row::from(vec![
        "Memory Leak Threshold",
        &format!("{:.1} MB/min", policy.memory_leak_slope_mb_min),
    ]));
    table.add_row(Row::from(vec![
        "Profiling Duration",
        &format!("{}s", policy.profiling_duration_secs),
    ]));
    table.add_row(Row::from(vec![
        "Off-Peak Window",
        &format!("{:02}:00 - {:02}:00", policy.off_peak_start_hour, policy.off_peak_end_hour),
    ]));
    table.add_row(Row::from(vec![
        "Min Confidence",
        &format!("{:.0}%", policy.min_confidence * 100.0),
    ]));

    println!("{table}");
    Ok(())
}
