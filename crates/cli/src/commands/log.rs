use colored::Colorize;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, ContentArrangement, Table,
};
use craft_core::{
    audit::DEFAULT_AUDIT_SECRET,
    process::{get_server_running_pid, is_server_locked},
    CraftError, CraftPaths, LogLevel, LogQuery, RemotesRegistry, Result,
    ServersRegistry,
};
use craft_daemon::{DaemonClient, LogIngestionService};
use craft_remote::RemoteCraftClient;
use dialoguer::{theme::ColorfulTheme, Select};
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use crate::cli::LogCommands;
use crate::commands::view::handle_view;

/// Redacts passwords, tokens, and secret credentials from configuration files.
pub fn redact_sensitive_config(content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.lines() {
        if let Some(eq_idx) = line.find('=') {
            let key = line[..eq_idx].trim().to_lowercase();
            if key.contains("password")
                || key.contains("secret")
                || key.contains("token")
                || key.contains("auth")
                || key.contains("api_key")
                || key.contains("private_key")
                || key == "rcon.password"
            {
                lines.push(format!("{}=********", &line[..eq_idx]));
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines.join("\n")
}

/// Dispatches log commands: live console viewing, diagnostic bundle export, search, forensics, and indexing.
pub async fn handle_log(
    name_arg: &str,
    path_arg: Option<PathBuf>,
    action: Option<LogCommands>,
    paths: &CraftPaths,
) -> Result<()> {
    match action {
        None => handle_view(name_arg, path_arg, paths).await,
        Some(LogCommands::View { name, path }) => {
            let eff_name = if !name.is_empty() { &name } else { name_arg };
            let eff_path = path.or(path_arg);
            handle_view(eff_name, eff_path, paths).await
        }
        Some(LogCommands::Export {
            name,
            path,
            output,
            format,
        }) => {
            let eff_name = if !name.is_empty() { &name } else { name_arg };
            let eff_path = path.or(path_arg);
            handle_log_export(eff_name, eff_path, output, &format, paths).await
        }
        Some(LogCommands::Search {
            pattern,
            server,
            level,
            regex,
            since,
            until,
            limit,
            remote,
            json,
        }) => {
            handle_log_search(pattern, server, level, regex, since, until, limit, remote, json, paths).await
        }
        Some(LogCommands::Forensics {
            server,
            incident,
            json,
            export,
        }) => {
            handle_log_forensics(server, incident, json, export, paths).await
        }
        Some(LogCommands::Incidents {
            server,
            limit,
            json,
        }) => {
            handle_log_incidents(server, limit, json, paths).await
        }
        Some(LogCommands::Index { server, force }) => {
            handle_log_index(server, force, paths).await
        }
    }
}

/// Generates a diagnostic archive bundling logs, crash reports, sanitized configuration, and host metrics.
pub async fn handle_log_export(
    name_arg: &str,
    path_arg: Option<PathBuf>,
    output_arg: Option<PathBuf>,
    format_arg: &str,
    paths: &CraftPaths,
) -> Result<()> {
    let mut resolved_name = name_arg.to_string();

    // Prompt user interactively if server name is omitted in an interactive terminal
    if resolved_name.is_empty() && path_arg.is_none() && std::io::stdin().is_terminal() {
        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            return Err(CraftError::Other(
                "No registered servers found to export diagnostics for.".to_string(),
            ));
        }

        let options: Vec<String> = registry
            .servers
            .iter()
            .map(|s| format!("{:<20} [{} {}]", s.name, s.software, s.version))
            .collect();

        let idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select server to export diagnostic bundle")
            .items(&options)
            .default(0)
            .interact()?;

        resolved_name = registry.servers[idx].name.clone();
    }

    let server_path = paths.resolve_server_path(
        path_arg.as_deref(),
        if resolved_name.is_empty() {
            None
        } else {
            Some(&resolved_name)
        },
        true,
    )?;

    if !server_path.exists() {
        return Err(CraftError::InvalidPath(
            server_path.to_string_lossy().to_string(),
        ));
    }

    let registry = ServersRegistry::load(paths)?;
    let entry = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!(
            "Server at '{}' is not registered.",
            server_path.display()
        ))
    })?;

    let server_name = entry.name.clone();
    let is_zstd = !format_arg.eq_ignore_ascii_case("gzip")
        && !format_arg.eq_ignore_ascii_case("gz")
        && !format_arg.eq_ignore_ascii_case("tar.gz");
    let ext = if is_zstd { "tar.zst" } else { "tar.gz" };
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let default_output = PathBuf::from(format!("{}-diagnostics-{}.{}", server_name, timestamp, ext));
    let output_path = output_arg.unwrap_or(default_output);

    println!(
        "{} {}",
        "[DIAGNOSTICS]".cyan().bold(),
        format!("Gathering diagnostic data for '{}'...", server_name).bold()
    );

    // Collect host system telemetry
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let host_info = serde_json::json!({
        "hostname": sysinfo::System::host_name(),
        "os_name": sysinfo::System::name(),
        "os_version": sysinfo::System::os_version(),
        "kernel_version": sysinfo::System::kernel_version(),
        "cpu_arch": sysinfo::System::cpu_arch(),
        "cpu_cores": sys.cpus().len(),
        "total_memory_bytes": sys.total_memory(),
        "used_memory_bytes": sys.used_memory(),
        "available_memory_bytes": sys.available_memory(),
    });

    let is_running =
        is_server_locked(&server_path) || get_server_running_pid(&server_path).is_some();
    let pid = get_server_running_pid(&server_path);

    // Query daemon status if running
    let mut circuit_breaker_info = None;
    let mut backup_schedule_info = None;
    if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut client) = DaemonClient::connect(paths).await {
            if let Ok(breakers) = client.get_circuit_breakers().await {
                circuit_breaker_info = breakers
                    .into_iter()
                    .find(|b| b.path == server_path || b.server_name == server_name);
            }
            if let Ok(schedules) = client.get_backup_schedules().await {
                backup_schedule_info = schedules
                    .into_iter()
                    .find(|s| s.server_name == server_name);
            }
        }
    }

    // Assemble diagnostics.json manifest
    let diag_manifest = serde_json::json!({
        "craft_version": env!("CARGO_PKG_VERSION"),
        "export_timestamp_utc": chrono::Utc::now().to_rfc3339(),
        "server": {
            "name": server_name,
            "path": server_path.to_string_lossy(),
            "software": entry.software,
            "version": entry.version,
            "port": entry.port,
            "is_running": is_running,
            "pid": pid,
        },
        "host": host_info,
        "circuit_breaker": circuit_breaker_info,
        "backup_schedule": backup_schedule_info,
    });
    let diag_bytes = serde_json::to_vec_pretty(&diag_manifest)?;

    // Create compressed archive
    let out_file = File::create(&output_path)?;
    let mut tar_builder = if is_zstd {
        let enc = zstd::stream::write::Encoder::new(out_file, 3)?.auto_finish();
        tar::Builder::new(Box::new(enc) as Box<dyn Write>)
    } else {
        let enc = flate2::write::GzEncoder::new(out_file, flate2::Compression::default());
        tar::Builder::new(Box::new(enc) as Box<dyn Write>)
    };

    let mut log_count = 0usize;
    let mut crash_count = 0usize;
    let mut config_count = 0usize;

    // 1. Write diagnostics.json
    append_memory_file(&mut tar_builder, "diagnostics.json", &diag_bytes)?;

    // 2. Collect log files
    let logs_dir = server_path.join("logs");
    if logs_dir.exists() && logs_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&logs_dir) {
            let mut log_files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && (p.extension().map_or(false, |ext| ext == "log" || ext == "gz"))
                })
                .collect();

            // Sort newest first, take up to 15 files
            log_files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
            log_files.reverse();
            log_files.truncate(15);

            for log_file in log_files {
                if let Some(file_name) = log_file.file_name() {
                    let rel_path = format!("logs/{}", file_name.to_string_lossy());
                    if let Ok(mut f) = File::open(&log_file) {
                        let mut buf = Vec::new();
                        if f.read_to_end(&mut buf).is_ok() {
                            append_memory_file(&mut tar_builder, &rel_path, &buf)?;
                            log_count += 1;
                        }
                    }
                }
            }
        }
    } else {
        // Root server.log fallback
        let root_log = server_path.join("server.log");
        if root_log.exists() && root_log.is_file() {
            if let Ok(mut f) = File::open(&root_log) {
                let mut buf = Vec::new();
                if f.read_to_end(&mut buf).is_ok() {
                    append_memory_file(&mut tar_builder, "logs/server.log", &buf)?;
                    log_count += 1;
                }
            }
        }
    }

    // 3. Collect crash reports
    let crash_dir = server_path.join("crash-reports");
    if crash_dir.exists() && crash_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&crash_dir) {
            let mut crash_files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            crash_files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
            crash_files.reverse();
            crash_files.truncate(10);

            for crash_file in crash_files {
                if let Some(file_name) = crash_file.file_name() {
                    let rel_path = format!("crash-reports/{}", file_name.to_string_lossy());
                    if let Ok(mut f) = File::open(&crash_file) {
                        let mut buf = Vec::new();
                        if f.read_to_end(&mut buf).is_ok() {
                            append_memory_file(&mut tar_builder, &rel_path, &buf)?;
                            crash_count += 1;
                        }
                    }
                }
            }
        }
    }

    // 4. Collect configuration files (sanitized)
    let prop_file = server_path.join("server.properties");
    if prop_file.exists() && prop_file.is_file() {
        if let Ok(raw) = fs::read_to_string(&prop_file) {
            let sanitized = redact_sensitive_config(&raw);
            append_memory_file(&mut tar_builder, "config/server.properties", sanitized.as_bytes())?;
            config_count += 1;
        }
    }

    let custom_toml = server_path.join("craft.custom.toml");
    if custom_toml.exists() && custom_toml.is_file() {
        if let Ok(raw) = fs::read_to_string(&custom_toml) {
            let sanitized = redact_sensitive_config(&raw);
            append_memory_file(&mut tar_builder, "config/craft.custom.toml", sanitized.as_bytes())?;
            config_count += 1;
        }
    }

    let eula_file = server_path.join("eula.txt");
    if eula_file.exists() && eula_file.is_file() {
        if let Ok(raw) = fs::read(&eula_file) {
            append_memory_file(&mut tar_builder, "config/eula.txt", &raw)?;
            config_count += 1;
        }
    }

    let mut inner = tar_builder.into_inner()?;
    inner.flush()?;

    let final_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    let size_display = if final_size > 1024 * 1024 {
        format!("{:.2} MB", final_size as f64 / (1024.0 * 1024.0))
    } else if final_size > 1024 {
        format!("{:.1} KB", final_size as f64 / 1024.0)
    } else {
        format!("{} B", final_size)
    };

    println!();
    println!("{}", "[OK] Diagnostic bundle exported successfully.".green().bold());
    println!("  Archive : {}", output_path.display().to_string().cyan());
    println!("  Size    : {}", size_display.yellow());
    println!(
        "  Payload : diagnostics.json, {} log file(s), {} crash report(s), {} config file(s)",
        log_count, crash_count, config_count
    );
    println!();

    Ok(())
}

fn append_memory_file(
    tar: &mut tar::Builder<Box<dyn Write>>,
    rel_path: &str,
    data: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(rel_path)?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append(&header, data)?;
    Ok(())
}

/// Executes sub-millisecond search across local or remote inverted index blocks.
pub async fn handle_log_search(
    pattern: String,
    server: Option<String>,
    level: Option<String>,
    regex: bool,
    since: Option<String>,
    until: Option<String>,
    limit: usize,
    remote: Option<String>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let parsed_level = level.as_ref().map(|l| LogLevel::parse_from_str(l));
    let min_timestamp = since.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&chrono::Utc));
    let max_timestamp = until.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&chrono::Utc));

    let query = LogQuery {
        server_name: server.clone(),
        query_pattern: pattern.clone(),
        is_regex: regex,
        level: parsed_level,
        min_timestamp,
        max_timestamp,
        limit,
        offset: 0,
    };

    let result = if let Some(ref remote_alias) = remote {
        let remotes_reg = RemotesRegistry::load(paths)?;
        let remote_config = remotes_reg.find(remote_alias).ok_or_else(|| {
            CraftError::Other(format!("Remote alias '{}' not found in registry.", remote_alias))
        })?;
        let client = RemoteCraftClient::connect(remote_config)?;
        client.search_remote_logs(&query)?
    } else if DaemonClient::is_daemon_running(paths) {
        let mut client = DaemonClient::connect(paths).await?;
        client.search_logs(query).await?
    } else {
        let service = LogIngestionService::new(paths);
        service.search(&query).await?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!();
    println!(
        "{} Query: \"{}\" | Server: {} | Level: {} | Matches: {} | Scanned: {} | Duration: {:.3}ms",
        "[SEARCH]".cyan().bold(),
        if pattern.is_empty() { "*" } else { &pattern },
        server.as_deref().unwrap_or("ALL"),
        level.as_deref().unwrap_or("ALL"),
        result.total_matches,
        result.scanned_lines,
        result.duration_micros as f64 / 1000.0,
    );
    println!();

    if result.matches.is_empty() {
        println!("  {}", "No log lines matching query criteria.".yellow());
        println!();
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("TIMESTAMP").fg(Color::Cyan),
            Cell::new("SERVER").fg(Color::Cyan),
            Cell::new("LEVEL").fg(Color::Cyan),
            Cell::new("THREAD").fg(Color::Cyan),
            Cell::new("MESSAGE").fg(Color::Cyan),
        ]);

    for entry in &result.matches {
        let lvl_cell = match entry.level {
            LogLevel::Fatal | LogLevel::Error => Cell::new(entry.level.as_str()).fg(Color::Red),
            LogLevel::Warn => Cell::new(entry.level.as_str()).fg(Color::Yellow),
            LogLevel::Info => Cell::new(entry.level.as_str()).fg(Color::Green),
            LogLevel::Debug | LogLevel::Trace => Cell::new(entry.level.as_str()).fg(Color::Blue),
            LogLevel::Unknown => Cell::new(entry.level.as_str()).fg(Color::White),
        };

        table.add_row(vec![
            Cell::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S")),
            Cell::new(&entry.server_name).fg(Color::Magenta),
            lvl_cell,
            Cell::new(entry.thread.as_deref().unwrap_or("-")),
            Cell::new(&entry.message),
        ]);
    }

    println!("{table}");
    println!();

    Ok(())
}

/// Generates and renders automated incident timeline post-mortems with stack demangling.
pub async fn handle_log_forensics(
    server: String,
    incident: Option<String>,
    json: bool,
    export: Option<PathBuf>,
    paths: &CraftPaths,
) -> Result<()> {
    let timeline = if DaemonClient::is_daemon_running(paths) {
        let mut client = DaemonClient::connect(paths).await?;
        client.get_incident_forensics(server.clone(), incident).await?
    } else {
        let service = LogIngestionService::new(paths);
        service.get_incident_forensics(&server, incident.as_deref())?
    };

    let is_authentic = timeline.verify_authenticity(DEFAULT_AUDIT_SECRET);

    if json {
        println!("{}", serde_json::to_string_pretty(&timeline)?);
        return Ok(());
    }

    println!();
    println!("{}", "================================================================================".bold());
    println!("{} POST-MORTEM FORENSIC INCIDENT REPORT: {}", "[INCIDENT]".red().bold(), timeline.incident_id.bold());
    println!("{}", "================================================================================".bold());
    println!("  Server     : {}", timeline.server_name.cyan().bold());
    println!("  Timestamp  : {}", timeline.timestamp.to_rfc3339().yellow());

    let auth_status = if is_authentic {
        "[AUTHENTIC] Cryptographic HMAC-SHA256 signature verified".green().bold()
    } else {
        "[TAMPERED] Warning: HMAC signature verification failed".red().bold()
    };
    println!("  Integrity  : {}", auth_status);
    println!("  Exception  : {}", timeline.culprit_exception.red().bold());

    if let Some(ref msg) = timeline.culprit_message {
        println!("  Message    : {}", msg);
    }
    if let Some(ref plugin) = timeline.suspected_plugin {
        println!("  Culprit    : {} {}", "[CULPRIT]".magenta().bold(), plugin.magenta().bold());
    }
    if !timeline.concurrency_threads.is_empty() {
        println!("  Threads    : {}", timeline.concurrency_threads.join(", ").cyan());
    }
    println!();

    // Render stack frames
    if !timeline.demangled_stack_trace.is_empty() {
        println!("  {}", "Demangled Stack Trace Frames:".bold());
        let mut frame_table = Table::new();
        frame_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("#").fg(Color::Cyan),
                Cell::new("ORIGIN").fg(Color::Cyan),
                Cell::new("CLASS").fg(Color::Cyan),
                Cell::new("METHOD").fg(Color::Cyan),
                Cell::new("LOCATION").fg(Color::Cyan),
                Cell::new("JAR SOURCE").fg(Color::Cyan),
            ]);

        for (idx, frame) in timeline.demangled_stack_trace.iter().enumerate() {
            let origin_cell = match frame.culprit_type {
                craft_core::CulpritType::Plugin => Cell::new("[CULPRIT: Plugin]").fg(Color::Red),
                craft_core::CulpritType::Core => Cell::new("Core").fg(Color::Yellow),
                craft_core::CulpritType::JavaRuntime => Cell::new("JavaRuntime").fg(Color::DarkGrey),
                craft_core::CulpritType::Native => Cell::new("Native").fg(Color::Blue),
                craft_core::CulpritType::Unknown => Cell::new("Unknown").fg(Color::White),
            };

            let loc = match (&frame.file_name, frame.line_number) {
                (Some(f), Some(l)) => format!("{}:{}", f, l),
                (Some(f), None) => f.clone(),
                (None, _) => "Unknown".to_string(),
            };

            frame_table.add_row(vec![
                Cell::new(idx + 1),
                origin_cell,
                Cell::new(&frame.class_name),
                Cell::new(&frame.method_name),
                Cell::new(loc),
                Cell::new(frame.jar_source.as_deref().unwrap_or("-")),
            ]);
        }

        println!("{frame_table}");
        println!();
    }

    // Render preceding context events
    if !timeline.preceding_events.is_empty() {
        println!("  {}", format!("Preceding Context Events (last {} events):", timeline.preceding_events.len()).bold());
        for entry in &timeline.preceding_events {
            println!(
                "    {} [{}] {}",
                entry.timestamp.format("%H:%M:%S").to_string().cyan(),
                entry.level.as_str(),
                entry.message
            );
        }
        println!();
    }

    // Export report if requested
    if let Some(export_path) = export {
        let mut report = String::new();
        report.push_str(&format!("# Incident Forensic Report: {}\n\n", timeline.incident_id));
        report.push_str(&format!("- **Server**: {}\n", timeline.server_name));
        report.push_str(&format!("- **Timestamp**: {}\n", timeline.timestamp.to_rfc3339()));
        report.push_str(&format!("- **Authenticity**: {}\n", if is_authentic { "VERIFIED" } else { "TAMPERED" }));
        report.push_str(&format!("- **Exception**: `{}`\n", timeline.culprit_exception));
        if let Some(ref msg) = timeline.culprit_message {
            report.push_str(&format!("- **Message**: {}\n", msg));
        }
        if let Some(ref plugin) = timeline.suspected_plugin {
            report.push_str(&format!("- **Suspected Culprit**: `{}`\n", plugin));
        }
        report.push_str("\n## Stack Frames\n\n");
        report.push_str("| # | Origin | Class | Method | Location | Jar Source |\n");
        report.push_str("|---|---|---|---|---|---|\n");
        for (i, f) in timeline.demangled_stack_trace.iter().enumerate() {
            report.push_str(&format!(
                "| {} | {} | `{}` | `{}` | {}:{} | {} |\n",
                i + 1,
                f.culprit_type,
                f.class_name,
                f.method_name,
                f.file_name.as_deref().unwrap_or("?"),
                f.line_number.unwrap_or(0),
                f.jar_source.as_deref().unwrap_or("-")
            ));
        }
        report.push_str("\n## Preceding Events\n\n```\n");
        for e in &timeline.preceding_events {
            report.push_str(&format!("[{}] [{}] {}\n", e.timestamp.format("%H:%M:%S"), e.level, e.message));
        }
        report.push_str("```\n");

        fs::write(&export_path, report)?;
        println!("{} Forensic post-mortem report exported to '{}'", "[OK]".green().bold(), export_path.display());
        println!();
    }

    Ok(())
}

/// Lists all historical crash incident reports discovered in the forensics directory.
pub async fn handle_log_incidents(
    server: Option<String>,
    limit: usize,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let incidents = if DaemonClient::is_daemon_running(paths) {
        let mut client = DaemonClient::connect(paths).await?;
        client.list_incidents(server.clone()).await?
    } else {
        let service = LogIngestionService::new(paths);
        service.list_incidents(server.as_deref())?
    };

    if json {
        let limited: Vec<_> = incidents.into_iter().take(limit).collect();
        println!("{}", serde_json::to_string_pretty(&limited)?);
        return Ok(());
    }

    if incidents.is_empty() {
        println!();
        println!("{} No incident post-mortems recorded.", "[OK]".green().bold());
        println!();
        return Ok(());
    }

    println!();
    println!("{}", "[INCIDENTS] Historical Crash Forensics & Post-Mortem Records:".cyan().bold());
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("INCIDENT ID").fg(Color::Cyan),
            Cell::new("SERVER").fg(Color::Cyan),
            Cell::new("TIMESTAMP").fg(Color::Cyan),
            Cell::new("EXCEPTION").fg(Color::Cyan),
            Cell::new("CULPRIT PLUGIN").fg(Color::Cyan),
            Cell::new("FRAMES").fg(Color::Cyan),
            Cell::new("AUTHENTIC").fg(Color::Cyan),
        ]);

    for inc in incidents.iter().take(limit) {
        let auth_cell = if inc.authenticity_valid {
            Cell::new("[AUTHENTIC]").fg(Color::Green)
        } else {
            Cell::new("[TAMPERED]").fg(Color::Red)
        };

        table.add_row(vec![
            Cell::new(&inc.incident_id).fg(Color::Yellow),
            Cell::new(&inc.server_name).fg(Color::Magenta),
            Cell::new(inc.timestamp.format("%Y-%m-%d %H:%M:%S")),
            Cell::new(&inc.culprit_exception).fg(Color::Red),
            Cell::new(inc.suspected_plugin.as_deref().unwrap_or("-")),
            Cell::new(inc.frames_count),
            auth_cell,
        ]);
    }

    println!("{table}");
    println!();

    Ok(())
}

/// Triggers on-demand ingestion and compression of server log files into inverted index blocks.
pub async fn handle_log_index(
    server: Option<String>,
    _force: bool,
    paths: &CraftPaths,
) -> Result<()> {
    println!();
    println!(
        "{} Ingesting and indexing log files for {}...",
        "[INDEX]".cyan().bold(),
        server.as_deref().unwrap_or("all registered servers")
    );

    let (indexed_lines, blocks_created, duration_ms) = if DaemonClient::is_daemon_running(paths) {
        let mut client = DaemonClient::connect(paths).await?;
        client.ingest_logs_now(server.clone()).await?
    } else {
        let start = std::time::Instant::now();
        let service = LogIngestionService::new(paths);
        let (lines, blocks) = service.ingest_all_servers(server.as_deref())?;
        (lines, blocks, start.elapsed().as_millis() as u64)
    };

    println!(
        "{} Indexed {} lines across {} inverted block(s) in {}ms.",
        "[INDEXED]".green().bold(),
        indexed_lines,
        blocks_created,
        duration_ms
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_sensitive_config() {
        let sample = "server-port=25565\nrcon.password=SuperSecret123!\nmotd=Welcome to our server\nsecret_token=abc-xyz-token\n";
        let redacted = redact_sensitive_config(sample);
        assert!(!redacted.contains("SuperSecret123!"));
        assert!(!redacted.contains("abc-xyz-token"));
        assert!(redacted.contains("rcon.password=********"));
        assert!(redacted.contains("secret_token=********"));
        assert!(redacted.contains("server-port=25565"));
        assert!(redacted.contains("motd=Welcome to our server"));
    }

    #[test]
    fn test_export_bundle_generation() {
        let temp_dir = std::env::temp_dir().join(format!("craft_test_diag_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let logs_dir = temp_dir.join("logs");
        let _ = fs::create_dir_all(&logs_dir);
        fs::write(logs_dir.join("latest.log"), "Hello server log").unwrap();

        let prop_file = temp_dir.join("server.properties");
        fs::write(&prop_file, "server-port=25565\nrcon.password=TopSecret\n").unwrap();

        let out_archive = temp_dir.join("test-diag.tar.zst");
        let out_file = File::create(&out_archive).unwrap();
        let enc = zstd::stream::write::Encoder::new(out_file, 3).unwrap().auto_finish();
        let mut tar_builder = tar::Builder::new(Box::new(enc) as Box<dyn Write>);

        append_memory_file(&mut tar_builder, "diagnostics.json", b"{\"test\": true}").unwrap();
        let sanitized = redact_sensitive_config(&fs::read_to_string(&prop_file).unwrap());
        append_memory_file(&mut tar_builder, "config/server.properties", sanitized.as_bytes()).unwrap();

        let mut inner = tar_builder.into_inner().unwrap();
        inner.flush().unwrap();

        assert!(out_archive.exists());
        assert!(fs::metadata(&out_archive).unwrap().len() > 0);

        // Verify reading back archive
        let read_file = File::open(&out_archive).unwrap();
        let dec = zstd::stream::read::Decoder::new(read_file).unwrap();
        let mut archive = tar::Archive::new(dec);

        let mut found_diag = false;
        let mut found_prop = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().to_string();
            if path == "diagnostics.json" {
                found_diag = true;
            } else if path == "config/server.properties" {
                found_prop = true;
                let mut content = String::new();
                entry.read_to_string(&mut content).unwrap();
                assert!(!content.contains("TopSecret"));
                assert!(content.contains("rcon.password=********"));
            }
        }

        assert!(found_diag);
        assert!(found_prop);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
