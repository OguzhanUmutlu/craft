use crate::cli::GatewayCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftPaths, GlobalSettings, Result};
use craft_daemon::Supervisor;
use std::time::Duration;

pub async fn handle_gateway(action: GatewayCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        GatewayCommands::Status => handle_status(paths).await,
        GatewayCommands::Enable { enabled } => handle_enable(enabled, paths),
        GatewayCommands::SetToken { token } => handle_set_token(token, paths),
        GatewayCommands::Metrics { raw } => handle_metrics(raw, paths).await,
    }
}

async fn handle_status(paths: &CraftPaths) -> Result<()> {
    let settings = GlobalSettings::load(paths).unwrap_or_default();

    println!("{}", "Craft Gateway & Telemetry Status".cyan().bold());
    println!("{}", "=".repeat(40).dimmed());

    let enabled_str = if settings.gateway_enabled {
        "[ENABLED]".green().bold()
    } else {
        "[DISABLED]".red().bold()
    };
    println!("  Status:     {}", enabled_str);
    println!("  Bind Host:  {}", settings.gateway_bind.yellow());
    println!("  HTTP Port:  {}", settings.gateway_port.to_string().yellow());

    let token_str = if settings.gateway_token.is_some() {
        "[CONFIGURED (Protected)]".green()
    } else {
        "[NONE (Open Access)]".yellow()
    };
    println!("  Auth Token: {}", token_str);

    let host = if settings.gateway_bind == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &settings.gateway_bind
    };

    println!("\n{}", "Available Endpoints:".cyan());
    println!(
        "  Prometheus: http://{}:{}/metrics",
        host, settings.gateway_port
    );
    println!(
        "  Health:     http://{}:{}/health",
        host, settings.gateway_port
    );
    println!(
        "  WebSocket:  ws://{}:{}/ws/console?server=<name>",
        host, settings.gateway_port
    );

    println!("\n{}", "Storage Monitor Thresholds:".cyan());
    println!(
        "  Bytes:   {} ({} GB)",
        settings.storage_warning_threshold_bytes,
        settings.storage_warning_threshold_bytes / (1024 * 1024 * 1024)
    );
    println!(
        "  Percent: {}%",
        settings.storage_warning_threshold_percent
    );

    // Probe daemon health check if running
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap_or_default();

    let health_url = format!("http://{}:{}/health", host, settings.gateway_port);
    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            println!(
                "\n{} Gateway daemon service is actively responding at {}.",
                "[OK]".green().bold(),
                health_url.dimmed()
            );
        }
        _ => {
            if settings.gateway_enabled {
                println!(
                    "\n{} Gateway service not responding at {}. (Is daemon running?)",
                    "[WARN]".yellow().bold(),
                    health_url.dimmed()
                );
            }
        }
    }

    Ok(())
}

fn handle_enable(enabled: bool, paths: &CraftPaths) -> Result<()> {
    let mut settings = GlobalSettings::load(paths).unwrap_or_default();
    settings.gateway_enabled = enabled;
    settings.save(paths)?;

    if enabled {
        println!(
            "{} Gateway server has been {}.",
            "[OK]".green().bold(),
            "enabled".green().bold()
        );
    } else {
        println!(
            "{} Gateway server has been {}.",
            "[OK]".green().bold(),
            "disabled".yellow().bold()
        );
    }
    println!("Note: If the background daemon is already running, restart it to apply changes:");
    println!("  craft service restart");
    Ok(())
}

fn handle_set_token(token: Option<String>, paths: &CraftPaths) -> Result<()> {
    let final_token = match token {
        Some(t) => {
            let trimmed = t.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        None => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let pid = std::process::id();
            let rand_val = (now ^ ((pid as u128) << 32)) ^ 0x5a5a_a5a5_5a5a_a5a5;
            Some(format!("{:016x}{:016x}", now, rand_val))
        }
    };

    let mut settings = GlobalSettings::load(paths).unwrap_or_default();
    settings.gateway_token = final_token.clone();
    settings.save(paths)?;

    if let Some(ref t) = final_token {
        println!(
            "{} Gateway Bearer token updated successfully:",
            "[OK]".green().bold()
        );
        println!("  Token: {}", t.cyan().bold());
        println!("\nAuthenticate requests with:");
        println!("  HTTP Header:      Authorization: Bearer {}", t);
        println!("  WebSocket Query:  ws://.../ws/console?server=<name>&token={}", t);
    } else {
        println!(
            "{} Gateway Bearer token cleared (open access mode).",
            "[OK]".green().bold()
        );
    }

    println!("\nNote: Restart the daemon if currently running:");
    println!("  craft service restart");
    Ok(())
}

async fn handle_metrics(raw: bool, paths: &CraftPaths) -> Result<()> {
    let settings = GlobalSettings::load(paths).unwrap_or_default();
    let host = if settings.gateway_bind == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &settings.gateway_bind
    };
    let metrics_url = format!("http://{}:{}/metrics", host, settings.gateway_port);

    // Try fetching from gateway HTTP endpoint first
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    let mut req = client.get(&metrics_url);
    if let Some(ref token) = settings.gateway_token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let metrics_text = match req.send().await {
        Ok(resp) if resp.status().is_success() => resp.text().await.unwrap_or_default(),
        _ => {
            // Fallback: generate metrics locally on demand
            let sup = Supervisor::new(paths.clone());
            craft_daemon::generate_prometheus_metrics(&sup, paths).await
        }
    };

    if raw {
        print!("{}", metrics_text);
        return Ok(());
    }

    // Format human-readable table
    println!("{}", "Craft Prometheus Telemetry Summary".cyan().bold());
    println!("{}", "=".repeat(40).dimmed());

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::White),
            Cell::new("Description").fg(Color::DarkGrey),
        ]);

    for line in metrics_text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let metric_name = parts[0];
            let value = parts[1];

            let desc = match metric_name {
                m if m.starts_with("craft_daemon_uptime_seconds") => "Daemon uptime in seconds",
                m if m.starts_with("craft_host_cpu_usage_percent") => "Total host CPU usage %",
                m if m.starts_with("craft_host_memory_total_bytes") => "Total host physical RAM",
                m if m.starts_with("craft_host_memory_used_bytes") => "Used host physical RAM",
                m if m.starts_with("craft_host_disk_total_bytes") => "Total disk storage capacity",
                m if m.starts_with("craft_host_disk_available_bytes") => "Available free disk storage",
                m if m.starts_with("craft_server_status") => "1 = running, 0 = stopped",
                m if m.starts_with("craft_server_players_online") => "Current online player count",
                m if m.starts_with("craft_server_players_max") => "Maximum player slots",
                m if m.starts_with("craft_server_query_latency_ms") => "SLP/RakNet/A2S query ping latency",
                m if m.starts_with("craft_server_crash_count") => "Consecutive crash count",
                m if m.starts_with("craft_server_circuit_breaker_state") => "0 = closed, 1 = half-open, 2 = open",
                _ => "",
            };

            table.add_row(Row::from(vec![
                Cell::new(metric_name),
                Cell::new(value).fg(Color::Yellow),
                Cell::new(desc),
            ]));
        }
    }

    println!("{}", table);
    println!("\nUse '{}' for scrapable Prometheus exposition format.", "craft gateway metrics --raw".cyan());
    Ok(())
}
