use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{AutoscaleRegistry, CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_autoscale(
    name: &str,
    enable: bool,
    disable: bool,
    idle_timeout: Option<u64>,
    motd: Option<String>,
    status: bool,
    paths: &CraftPaths,
) -> Result<()> {
    // If status requested or no action flags provided, display status
    if status || (!enable && !disable && idle_timeout.is_none() && motd.is_none() && name.is_empty()) {
        return display_autoscale_status(paths).await;
    }

    if name.is_empty() {
        return Err(CraftError::Other(
            "Server name must be provided to configure autoscale policy".to_string(),
        ));
    }

    let servers = ServersRegistry::load(paths)?;
    if !servers.servers.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
        return Err(CraftError::Other(format!("Server '{}' not found in registry", name)));
    }

    let mut reg = AutoscaleRegistry::load(paths).unwrap_or_default();
    let mut policy = reg.get_policy(name).cloned().unwrap_or_else(|| {
        craft_core::ServerAutoscalePolicy::new(name)
    });

    if enable {
        policy.hibernation_enabled = true;
    }
    if disable {
        policy.hibernation_enabled = false;
    }
    if let Some(timeout) = idle_timeout {
        policy.idle_timeout_mins = timeout;
    }
    if let Some(m) = motd {
        policy.sleep_motd = Some(m);
    }

    reg.set_policy(policy.clone());
    reg.save(paths)?;

    println!(
        "{} Autoscale policy updated for server '{}':",
        "[OK]".green().bold(),
        name.yellow()
    );
    println!("  Hibernation:  {}", if policy.hibernation_enabled { "[ENABLED]".green().bold() } else { "[DISABLED]".red().bold() });
    println!("  Idle Timeout: {} minutes", policy.idle_timeout_mins.to_string().cyan());
    println!("  Wake Packet:  {}", if policy.wake_on_packet { "[ENABLED]".green() } else { "[DISABLED]".red() });
    if let Some(ref m) = policy.sleep_motd {
        println!("  Sleep MOTD:   {}", m.dimmed());
    }

    Ok(())
}

async fn display_autoscale_status(paths: &CraftPaths) -> Result<()> {
    println!("{}", "\n=== Craft Server Auto-Scaling & Hibernation Status ===".cyan().bold());

    if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            if let Ok(statuses) = client.get_autoscale_status().await {
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec![
                        Cell::new("Server").fg(Color::Cyan),
                        Cell::new("Autoscale").fg(Color::Green),
                        Cell::new("State").fg(Color::Yellow),
                        Cell::new("Idle Timeout").fg(Color::Cyan),
                        Cell::new("Idle Time").fg(Color::Magenta),
                        Cell::new("Players").fg(Color::Yellow),
                    ]);

                for s in statuses {
                    let enabled_str = if s.enabled {
                        Cell::new("ENABLED").fg(Color::Green)
                    } else {
                        Cell::new("DISABLED").fg(Color::Red)
                    };

                    let state_str = if s.is_sleeping {
                        Cell::new("SLEEPING (SleepProxy)").fg(Color::Cyan)
                    } else {
                        Cell::new("ACTIVE / RUNNING").fg(Color::Green)
                    };

                    let idle_str = if s.idle_seconds > 0 {
                        format!("{}s", s.idle_seconds)
                    } else {
                        "0s".to_string()
                    };

                    table.add_row(Row::from(vec![
                        Cell::new(s.server_name),
                        enabled_str,
                        state_str,
                        Cell::new(format!("{}m", s.idle_timeout_mins)),
                        Cell::new(idle_str),
                        Cell::new(s.player_count.to_string()),
                    ]));
                }

                println!("{}\n", table);
                return Ok(());
            }
        }
    }

    // Fallback: Daemon offline, read local config
    println!("{}", "[WARN] Craft background supervisor daemon is currently offline; displaying stored configuration:".yellow());
    let reg = AutoscaleRegistry::load(paths).unwrap_or_default();
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Server").fg(Color::Cyan),
            Cell::new("Hibernation").fg(Color::Green),
            Cell::new("Idle Timeout").fg(Color::Cyan),
            Cell::new("Wake On Packet").fg(Color::Yellow),
        ]);

    for p in reg.policies {
        table.add_row(Row::from(vec![
            Cell::new(p.server_name),
            Cell::new(if p.hibernation_enabled { "ENABLED" } else { "DISABLED" }),
            Cell::new(format!("{}m", p.idle_timeout_mins)),
            Cell::new(if p.wake_on_packet { "YES" } else { "NO" }),
        ]));
    }

    println!("{}\n", table);
    Ok(())
}

pub async fn handle_hibernate(name: &str, wake: bool, paths: &CraftPaths) -> Result<()> {
    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other(
            "Craft daemon must be running to manage hibernation and SleepProxy".to_string(),
        ));
    }

    let mut client = DaemonClient::connect(paths).await?;

    if wake {
        println!("{} Waking server '{}'...", "[INFO]".cyan(), name.yellow().bold());
        let msg = client.wake_server(name).await?;
        println!("{} {}", "[OK]".green().bold(), msg);
    } else {
        println!("{} Hibernating server '{}' into SleepProxy...", "[INFO]".cyan(), name.yellow().bold());
        let msg = client.hibernate_server(name).await?;
        println!("{} {}", "[OK]".green().bold(), msg);
    }

    Ok(())
}
