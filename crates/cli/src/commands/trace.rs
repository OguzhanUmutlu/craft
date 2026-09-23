use crate::cli::TraceCommands;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use craft_core::{
    CraftPaths, RecordedSpan, Result, TraceTree, TraceTreeNode, TracingConfig,
    TracingRegistry, TracingStatusSummary,
};
use craft_daemon::{DaemonClient, TracingService};
use serde_json::json;

/// Entry point for `craft trace` / `craft tracing` / `craft otel` subcommands
pub async fn handle_trace(cmd: TraceCommands, paths: &CraftPaths) -> Result<()> {
    match cmd {
        TraceCommands::Status { json } => handle_status(paths, json).await,
        TraceCommands::List {
            service,
            min_duration_ms,
            limit,
            json,
        } => handle_list(paths, service.as_deref(), min_duration_ms, limit, json).await,
        TraceCommands::Get { trace_id, json } => handle_get(paths, &trace_id, json).await,
        TraceCommands::Export { json } => handle_export(paths, json).await,
        TraceCommands::Config {
            enabled,
            sampler,
            sample_ratio,
            otlp_endpoint,
            service_name,
            json,
        } => {
            handle_config(
                paths,
                enabled,
                sampler.as_deref(),
                sample_ratio,
                otlp_endpoint.as_deref(),
                service_name.as_deref(),
                json,
            )
            .await
        }
    }
}

async fn handle_status(paths: &CraftPaths, as_json: bool) -> Result<()> {
    let status: TracingStatusSummary = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_tracing_status().await {
            Ok(s) => s,
            Err(_) => TracingService::global(paths).get_status(),
        }
    } else {
        TracingService::global(paths).get_status()
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("[OK] Distributed Tracing & OpenTelemetry (OTel) Status");
    println!("--------------------------------------------------------------------------------");
    println!(
        "  Tracing Enabled:        {}",
        if status.enabled { "[YES]" } else { "[NO]" }
    );
    println!("  Service Name:           {}", status.service_name);
    println!(
        "  Sample Ratio:           {:.2}",
        status.sample_ratio
    );
    println!(
        "  Buffered Spans:         {} / {}",
        status.spans_buffered, status.buffer_capacity
    );
    println!("  Total Spans Recorded:   {}", status.spans_recorded);
    println!("  Dropped Spans:          {}", status.spans_dropped);
    println!(
        "  OTLP Exporter:          {}",
        if status.otlp_endpoint.is_some() { "Configured" } else { "Disabled" }
    );
    if let Some(ep) = &status.otlp_endpoint {
        println!("  OTLP Endpoint:          {}", ep);
    }
    println!("--------------------------------------------------------------------------------");

    Ok(())
}

async fn handle_list(
    paths: &CraftPaths,
    service: Option<&str>,
    min_duration_ms: Option<u64>,
    limit: usize,
    as_json: bool,
) -> Result<()> {
    let min_micros = min_duration_ms.map(|d| d * 1000);
    let spans: Vec<RecordedSpan> = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client
            .query_traces(
                service.map(str::to_string),
                None,
                min_micros,
                false,
                Some(limit),
            )
            .await
        {
            Ok(s) => s,
            Err(_) => {
                TracingService::global(paths).query_traces(
                    service.map(str::to_string),
                    None,
                    min_micros,
                    false,
                    Some(limit),
                )
            }
        }
    } else {
        TracingService::global(paths).query_traces(
            service.map(str::to_string),
            None,
            min_micros,
            false,
            Some(limit),
        )
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&spans)?);
        return Ok(());
    }

    if spans.is_empty() {
        println!("[OK] No recorded spans found matching query criteria.");
        return Ok(());
    }

    println!("[OK] Recorded Distributed Traces ({})", spans.len());

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Trace ID").fg(Color::Cyan),
            Cell::new("Span ID").fg(Color::Cyan),
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Service").fg(Color::Cyan),
            Cell::new("Kind").fg(Color::Cyan),
            Cell::new("Duration").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
        ]);

    for span in &spans {
        let dur_str = format_duration_micros(span.duration_micros);
        let status_color = match span.status.code.as_str() {
            "OK" => Color::Green,
            "ERROR" => Color::Red,
            _ => Color::DarkGrey,
        };

        table.add_row(vec![
            Cell::new(span.trace_id.to_hex()),
            Cell::new(span.span_id.to_hex()),
            Cell::new(&span.name),
            Cell::new(&span.service_name),
            Cell::new(span.kind.as_str()),
            Cell::new(dur_str),
            Cell::new(span.status.code.as_str()).fg(status_color),
        ]);
    }

    println!("{table}");
    Ok(())
}

async fn handle_get(paths: &CraftPaths, trace_id: &str, as_json: bool) -> Result<()> {
    let tree_opt: Option<TraceTree> = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.get_trace_details(trace_id.to_string()).await {
            Ok(t) => t,
            Err(_) => TracingService::global(paths).get_trace_details(trace_id),
        }
    } else {
        TracingService::global(paths).get_trace_details(trace_id)
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&tree_opt)?);
        return Ok(());
    }

    let tree = match tree_opt {
        Some(t) => t,
        None => {
            println!(
                "[WARN] Trace ID '{}' not found in span ring buffer.",
                trace_id
            );
            return Ok(());
        }
    };

    let root_name = tree.roots.first().map(|r| r.span.name.as_str()).unwrap_or("unknown");
    let root_service = tree.roots.first().map(|r| r.span.service_name.as_str()).unwrap_or("unknown");

    println!("[OK] Trace Details: {}", tree.trace_id.to_hex());
    println!("--------------------------------------------------------------------------------");
    println!("  Root Span:       {}", root_name);
    println!("  Service:         {}", root_service);
    println!("  Total Spans:     {}", tree.total_spans);
    println!(
        "  Duration:        {}",
        format_duration_micros(tree.total_duration_micros)
    );
    println!("--------------------------------------------------------------------------------");
    println!("Causal Span Tree:");

    for root in &tree.roots {
        render_tree_node(root, "", true);
    }

    Ok(())
}

fn render_tree_node(node: &TraceTreeNode, prefix: &str, is_last: bool) {
    let branch = if is_last { "`-- " } else { "|-- " };
    let dur_str = format_duration_micros(node.span.duration_micros);
    let status_str = format!("[{}]", node.span.status.code);

    println!(
        "{}{}[{}] {} ({}) [dur: {}] {}",
        prefix,
        branch,
        node.span.kind.as_str().to_uppercase(),
        node.span.name,
        node.span.service_name,
        dur_str,
        status_str
    );

    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "|   " });
    if !node.span.attributes.is_empty() {
        for (k, v) in &node.span.attributes {
            println!("{}  attr: {} = {:?}", child_prefix, k, v);
        }
    }

    for ev in &node.span.events {
        println!(
            "{}  event: {} (unix_nano: {})",
            child_prefix, ev.name, ev.timestamp_unix_nano
        );
    }

    let count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_tree_node(child, &child_prefix, i == count - 1);
    }
}

async fn handle_export(paths: &CraftPaths, as_json: bool) -> Result<()> {
    let (count, destination) = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.export_traces_now(None).await {
            Ok(res) => res,
            Err(_) => TracingService::global(paths).export_traces_now(None).await,
        }
    } else {
        TracingService::global(paths).export_traces_now(None).await
    };

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "exported_spans": count,
                "destination": destination,
                "success": count > 0,
            }))?
        );
        return Ok(());
    }

    if count > 0 {
        println!(
            "[OK] Successfully exported {} span(s): {}",
            count, destination
        );
    } else {
        println!(
            "[WARN] Export finished with 0 spans exported: {}",
            destination
        );
    }

    Ok(())
}

async fn handle_config(
    paths: &CraftPaths,
    enabled: Option<bool>,
    sampler: Option<&str>,
    sample_ratio: Option<f64>,
    otlp_endpoint: Option<&str>,
    service_name: Option<&str>,
    as_json: bool,
) -> Result<()> {
    let mut current_cfg = {
        let reg = TracingRegistry::load(paths)?;
        reg.config
    };

    if let Some(en) = enabled {
        current_cfg.enabled = en;
    }
    if let Some(sn) = service_name {
        current_cfg.service_name = sn.to_string();
    }
    if let Some(r) = sample_ratio {
        current_cfg.sample_ratio = r.clamp(0.0, 1.0);
    } else if let Some(s) = sampler {
        match s.to_lowercase().as_str() {
            "always_on" | "on" => current_cfg.sample_ratio = 1.0,
            "always_off" | "off" => current_cfg.sample_ratio = 0.0,
            _ => {}
        }
    }
    if let Some(ep) = otlp_endpoint {
        if ep == "none" || ep == "clear" || ep.is_empty() {
            current_cfg.otlp_endpoint = None;
        } else {
            current_cfg.otlp_endpoint = Some(ep.to_string());
        }
    }

    let cfg: TracingConfig = if let Ok(mut client) = DaemonClient::connect(paths).await {
        match client.set_tracing_config(current_cfg.clone()).await {
            Ok(c) => c,
            Err(_) => TracingService::global(paths).set_config(current_cfg)?,
        }
    } else {
        TracingService::global(paths).set_config(current_cfg)?
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&cfg)?);
        return Ok(());
    }

    println!("[OK] Tracing configuration updated successfully.");
    println!("--------------------------------------------------------------------------------");
    println!("  Enabled:            {}", cfg.enabled);
    println!("  Service Name:       {}", cfg.service_name);
    println!("  Sample Ratio:       {:.2}", cfg.sample_ratio);
    if let Some(ep) = &cfg.otlp_endpoint {
        println!("  OTLP Endpoint:      {}", ep);
    } else {
        println!("  OTLP Endpoint:      [none]");
    }
    println!("  Export Batch Size:  {}", cfg.export_batch_size);
    println!("  Buffer Capacity:    {}", cfg.buffer_capacity);
    println!("--------------------------------------------------------------------------------");

    Ok(())
}

fn format_duration_micros(micros: u64) -> String {
    if micros < 1000 {
        format!("{}µs", micros)
    } else if micros < 1_000_000 {
        format!("{:.2}ms", micros as f64 / 1000.0)
    } else {
        format!("{:.3}s", micros as f64 / 1_000_000.0)
    }
}
