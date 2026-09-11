use std::collections::HashSet;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_ls(paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;

    if registry.servers.is_empty() {
        println!("{}", "No servers found. Use 'craft new' to create one!".yellow());
        return Ok(());
    }

    let running_paths: HashSet<_> = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            client.get_running().await.unwrap_or_default().into_iter().collect()
        } else {
            HashSet::new()
        }
    } else {
        HashSet::new()
    };

    let mut table = Table::new();
    table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    table.set_header(vec![
        Cell::new("Name").fg(Color::Cyan),
        Cell::new("Software").fg(Color::Cyan),
        Cell::new("Version").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
        Cell::new("Auto-Run").fg(Color::Cyan),
        Cell::new("Path").fg(Color::Cyan),
    ]);

    for server in &registry.servers {
        let is_running = running_paths.contains(&server.path)
            || server.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);

        let status_cell = if is_running {
            Cell::new("RUNNING").fg(Color::Green)
        } else {
            Cell::new("STOPPED").fg(Color::DarkGrey)
        };

        let auto_cell = if server.auto {
            Cell::new("ENABLED").fg(Color::Yellow)
        } else {
            Cell::new("NO").fg(Color::DarkGrey)
        };

        table.add_row(Row::from(vec![
            Cell::new(&server.name),
            Cell::new(&server.software),
            Cell::new(&server.version),
            status_cell,
            auto_cell,
            Cell::new(server.path.to_string_lossy()),
        ]));
    }

    println!("{table}");
    Ok(())
}
