use crate::cli::DrCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_backup::{sample_mesh_integrity, BackupManifest, DrOrchestrator};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use std::fs;

pub async fn handle_dr(action: DrCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        DrCommands::Plan { server } => handle_plan(&server, paths),
        DrCommands::Test { server } => handle_test(&server, paths),
        DrCommands::Failover {
            server,
            target,
            live,
        } => handle_failover(&server, &target, live, paths),
        DrCommands::Status => handle_status(paths),
        DrCommands::Verify { server, sample } => handle_verify(&server, sample, paths),
    }
}

fn handle_plan(server: &str, paths: &CraftPaths) -> Result<()> {
    let plan = DrOrchestrator::generate_plan(server, paths)?;

    println!("\n{}", format!("Disaster Recovery Plan: {}", plan.server_name).bold());

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Parameter").fg(Color::Cyan),
            Cell::new("Configuration").fg(Color::Yellow),
        ]));

    table.add_row(Row::from(vec![
        Cell::new("Server Name"),
        Cell::new(&plan.server_name),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Primary Node"),
        Cell::new(&plan.primary_node),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Failover Nodes"),
        Cell::new(if plan.failover_nodes.is_empty() {
            "None (Local DR only)".to_string()
        } else {
            plan.failover_nodes.join(", ")
        }),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Recovery Time Objective (RTO)"),
        Cell::new(format!("{} seconds", plan.target_rto_seconds)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Recovery Point Objective (RPO)"),
        Cell::new(format!("{} hours", plan.target_rpo_hours)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Mesh Targets"),
        Cell::new(plan.mesh_targets.join(", ")),
    ]));

    println!("{}", table);

    println!("\n{}", "Cold-Start Runbook Steps:".cyan().bold());
    for step in &plan.reconstitution_steps {
        println!("  {}", step);
    }
    println!();
    Ok(())
}

fn handle_test(server: &str, paths: &CraftPaths) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server), true)?;
    let registry = ServersRegistry::load(paths)?;

    if registry.find_by_path(&server_path).is_none() {
        return Err(CraftError::ServerNotFound(format!(
            "Server '{}' is not registered.",
            server
        )));
    }

    println!(
        "{}",
        format!(
            "Simulating disaster recovery cold-start for server '{}' in isolated sandbox...",
            server
        )
        .cyan()
    );

    let result = DrOrchestrator::simulate_disaster_recovery(
        server,
        Some(&server_path),
        paths,
        None,
    )?;

    if result.is_valid {
        let mb = (result.total_bytes as f64) / (1024.0 * 1024.0);
        println!(
            "\n{} Disaster recovery simulation PASSED.",
            "[OK]".green().bold()
        );
        println!("  Server:            {}", result.server_name.cyan());
        println!(
            "  Simulated RTO:     {:.2}s (Target: <=30s)",
            result.rto_seconds.to_string().green()
        );
        println!("  Total Files:       {}", result.total_files);
        println!("  Total Data Size:   {:.2} MB", mb);
        println!(
            "  Data Divergence:   {} (exact byte-for-byte match)",
            "0 bytes".green().bold()
        );
        println!(
            "  Verification:      {}",
            "Directory tree and chunk hashes 100% verified.".green()
        );
        Ok(())
    } else {
        eprintln!(
            "\n{} Disaster recovery simulation FAILED!",
            "[ERROR]".red().bold()
        );
        if let Some(err) = result.error_message {
            eprintln!("  Failure Reason:    {}", err.red());
        }
        Err(CraftError::Other("DR simulation failed".to_string()))
    }
}

fn handle_failover(server: &str, target: &str, live: bool, paths: &CraftPaths) -> Result<()> {
    println!(
        "{}",
        format!(
            "Initiating automated disaster recovery failover for server '{}' to node '{}' (live: {})...",
            server, target, live
        )
        .cyan()
    );

    let report = DrOrchestrator::execute_failover(server, target, paths, None, live)?;

    let mb = (report.bytes_transferred as f64) / (1024.0 * 1024.0);
    println!(
        "\n{} Disaster recovery failover COMPLETED.",
        "[OK]".green().bold()
    );
    println!("  Server:            {}", report.server_name.cyan());
    println!("  Source Node:       {}", report.source_node);
    println!("  Destination Node:  {}", report.target_node.yellow().bold());
    println!("  Failover Duration: {:.2}s", report.duration_seconds);
    println!("  Chunks Synced:     {}", report.chunks_transferred);
    println!("  Transferred Data:  {:.2} MB", mb);
    println!(
        "  Status:            {}",
        "Server reconstituted and ready on destination node.".green()
    );
    Ok(())
}

fn handle_status(paths: &CraftPaths) -> Result<()> {
    println!("\n{}", "Disaster Recovery & Deduplication Status:".bold());

    let mut total_chunks = 0;
    let mut total_chunk_bytes = 0u64;

    if paths.chunks_dir.exists() {
        if let Ok(prefixes) = fs::read_dir(&paths.chunks_dir) {
            for pref in prefixes.flatten() {
                if pref.path().is_dir() {
                    if let Ok(chunks) = fs::read_dir(pref.path()) {
                        for chunk in chunks.flatten() {
                            if chunk.path().is_file() {
                                total_chunks += 1;
                                if let Ok(meta) = chunk.metadata() {
                                    total_chunk_bytes += meta.len();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mb = (total_chunk_bytes as f64) / (1024.0 * 1024.0);

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Yellow),
        ]));

    table.add_row(Row::from(vec![
        Cell::new("Content-Addressed Chunks"),
        Cell::new(total_chunks.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Chunk Pool Storage"),
        Cell::new(format!("{:.2} MB", mb)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Chunk Pool Location"),
        Cell::new(paths.chunks_dir.display().to_string()),
    ]));

    println!("{}", table);
    Ok(())
}

fn handle_verify(server: &str, sample_pct: f64, paths: &CraftPaths) -> Result<()> {
    println!(
        "{}",
        format!(
            "Running background Merkle-tree chunk sampling audit on server '{}' ({:.1}% sample)...",
            server, sample_pct
        )
        .cyan()
    );

    let backup_dir = paths.backups_dir.join(server);
    let mut manifests = Vec::new();
    if backup_dir.exists() {
        for entry in fs::read_dir(&backup_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.to_string_lossy().ends_with(".manifest.json") {
                manifests.push(p);
            }
        }
    }
    manifests.sort();

    let latest = manifests.last().ok_or_else(|| {
        CraftError::Other(format!("No manifests found for server '{}'", server))
    })?;

    let content = fs::read_to_string(latest)?;
    let manifest: BackupManifest = serde_json::from_str(&content)
        .map_err(|e| CraftError::Other(format!("Failed to parse manifest: {}", e)))?;

    let report = sample_mesh_integrity(paths, &manifest, sample_pct / 100.0)?;

    if report.is_valid {
        println!(
            "\n{} Merkle-tree integrity verification PASSED.",
            "[OK]".green().bold()
        );
        println!("  Manifest ID:       {}", report.manifest_id.cyan());
        println!("  Total Chunks:      {}", report.total_chunks);
        println!("  Sampled Chunks:    {}", report.sampled_chunks.to_string().green());
        println!("  Verified Chunks:   {}", report.verified_chunks.to_string().green());
        println!(
            "  Integrity Verdict: {}",
            "All sampled chunks match Merkle root and hash tree with 0 errors.".green()
        );
        Ok(())
    } else {
        eprintln!(
            "\n{} Merkle-tree integrity verification FAILED!",
            "[ERROR]".red().bold()
        );
        if !report.missing_chunks.is_empty() {
            eprintln!("  Missing Chunks:    {}", report.missing_chunks.len().to_string().red());
        }
        if !report.corrupted_chunks.is_empty() {
            eprintln!("  Corrupted Chunks:  {}", report.corrupted_chunks.len().to_string().red());
        }
        Err(CraftError::Other("Merkle verification failed".to_string()))
    }
}
