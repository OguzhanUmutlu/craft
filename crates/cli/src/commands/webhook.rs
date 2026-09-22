use crate::cli::WebhookCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, Result, WebhookEndpoint, WebhookEvent, WebhookKind, WebhooksRegistry};
use craft_daemon::webhooks::{WebhookDispatcher, WebhookPayload};
use std::str::FromStr;

pub async fn handle_webhook(action: WebhookCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        WebhookCommands::Add {
            name,
            url,
            kind,
            events,
            secret,
        } => handle_add(&name, &url, &kind, events, secret, paths),
        WebhookCommands::Remove { name } => handle_remove(&name, paths),
        WebhookCommands::List => handle_list(paths),
        WebhookCommands::Test { name } => handle_test(&name, paths).await,
    }
}

fn handle_add(
    name: &str,
    url: &str,
    kind_str: &str,
    events_raw: Vec<String>,
    secret: Option<String>,
    paths: &CraftPaths,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CraftError::Other("Webhook name cannot be empty".to_string()));
    }

    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(CraftError::Other(
            "Webhook URL must begin with http:// or https://".to_string(),
        ));
    }

    let kind = WebhookKind::from_str(kind_str)?;

    let mut parsed_events = Vec::new();
    let has_all = events_raw.iter().any(|e| {
        let trimmed = e.trim().to_lowercase();
        trimmed == "all" || trimmed == "*"
    });

    if has_all || events_raw.is_empty() {
        parsed_events.push(WebhookEvent::All);
    } else {
        for ev_str in &events_raw {
            for part in ev_str.split(',') {
                let p = part.trim();
                if !p.is_empty() {
                    parsed_events.push(WebhookEvent::from_str(p)?);
                }
            }
        }
    }

    let endpoint = WebhookEndpoint {
        id: name.to_string(),
        url: url.to_string(),
        kind,
        events: parsed_events.clone(),
        enabled: true,
        secret,
    };

    WebhooksRegistry::modify(paths, |reg| reg.add(endpoint))?;

    let event_names: Vec<String> = parsed_events.iter().map(|e| e.to_string()).collect();
    println!(
        "{} Webhook '{}' successfully registered.",
        "[OK]".green().bold(),
        name.cyan().bold()
    );
    println!("  Kind:    {}", kind_str.yellow());
    println!("  URL:     {}", url);
    println!("  Events:  {}", event_names.join(", ").dimmed());
    Ok(())
}

fn handle_remove(name: &str, paths: &CraftPaths) -> Result<()> {
    let name = name.trim();
    let removed = WebhooksRegistry::modify(paths, |reg| {
        if reg.remove(name) {
            Ok(true)
        } else {
            Ok(false)
        }
    })?;

    if removed {
        println!(
            "{} Webhook '{}' has been removed.",
            "[OK]".green().bold(),
            name.cyan()
        );
        Ok(())
    } else {
        Err(CraftError::Other(format!(
            "Webhook '{}' not found in configuration",
            name
        )))
    }
}

fn handle_list(paths: &CraftPaths) -> Result<()> {
    let registry = WebhooksRegistry::load(paths)?;

    if registry.webhooks.is_empty() {
        println!("No notification webhooks configured.");
        println!(
            "Use '{}' to configure a new webhook.",
            "craft webhook add <name> <url>".cyan()
        );
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Kind").fg(Color::Yellow),
            Cell::new("Status").fg(Color::Green),
            Cell::new("Events").fg(Color::White),
            Cell::new("Secret").fg(Color::Magenta),
            Cell::new("Target URL").fg(Color::Blue),
        ]);

    for ep in &registry.webhooks {
        let status_cell = if ep.enabled {
            Cell::new("[ACTIVE]").fg(Color::Green)
        } else {
            Cell::new("[DISABLED]").fg(Color::Red)
        };

        let secret_cell = if ep.secret.is_some() {
            Cell::new("[HMAC]").fg(Color::Magenta)
        } else {
            Cell::new("none").fg(Color::DarkGrey)
        };

        let events_str = if ep.events.is_empty() || ep.events.contains(&WebhookEvent::All) {
            "all".to_string()
        } else {
            ep.events
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        table.add_row(Row::from(vec![
            Cell::new(&ep.id),
            Cell::new(format!("{:?}", ep.kind).to_lowercase()),
            status_cell,
            Cell::new(events_str),
            secret_cell,
            Cell::new(&ep.url),
        ]));
    }

    println!("{}", table);
    Ok(())
}

async fn handle_test(name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = WebhooksRegistry::load(paths)?;
    let endpoint = registry.find(name).ok_or_else(|| {
        CraftError::Other(format!("Webhook endpoint '{}' not found", name))
    })?;

    let mut details = std::collections::HashMap::new();
    details.insert("test".to_string(), "true".to_string());
    details.insert("source".to_string(), "craft-cli".to_string());

    let payload = WebhookPayload {
        event: WebhookEvent::ServerStart,
        timestamp: chrono::Utc::now(),
        server_name: Some("test-server".to_string()),
        server_path: None,
        exit_code: None,
        crashes: None,
        title: "[TEST] Craft Webhook Notification".to_string(),
        message: format!(
            "Test notification from Craft CLI for webhook endpoint '{}'.",
            endpoint.id
        ),
        details,
    };

    println!(
        "Dispatching test notification to '{}' ({})...",
        endpoint.id.cyan(),
        endpoint.url.dimmed()
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Craft-CLI/1.0")
        .build()
        .unwrap_or_default();

    WebhookDispatcher::send_webhook(&client, endpoint, &payload).await?;

    println!(
        "{} Test notification delivered successfully.",
        "[OK]".green().bold()
    );
    Ok(())
}
