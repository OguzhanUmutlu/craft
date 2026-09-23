use crate::cli::MeshCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_backup::{BackupManifest, StorageMesh};
use craft_core::{CraftError, CraftPaths, MeshRegistry, MeshTarget, MeshTargetKind, Result};
use std::fs;
use std::str::FromStr;

pub fn handle_mesh(action: MeshCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        MeshCommands::Ls => handle_list(paths),
        MeshCommands::Add {
            id,
            name,
            kind,
            endpoint,
            bucket_or_path,
            access_key,
            secret_key,
            priority,
        } => handle_add(
            &id,
            &name,
            &kind,
            endpoint,
            &bucket_or_path,
            access_key,
            secret_key,
            priority,
            paths,
        ),
        MeshCommands::Rm { id } => handle_remove(&id, paths),
        MeshCommands::Sync { server } => handle_sync(&server, paths),
        MeshCommands::Health => handle_health(paths),
    }
}

fn handle_list(paths: &CraftPaths) -> Result<()> {
    let registry = MeshRegistry::load_or_init(paths)?;

    if registry.targets.is_empty() {
        println!("{}", "No storage mesh targets configured.".dimmed());
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Name").fg(Color::Yellow),
            Cell::new("Kind").fg(Color::Blue),
            Cell::new("Bucket / Path").fg(Color::White),
            Cell::new("Endpoint").fg(Color::DarkGrey),
            Cell::new("Enabled").fg(Color::Magenta),
            Cell::new("Priority").fg(Color::Cyan),
        ]));

    for target in &registry.targets {
        let enabled_cell = if target.enabled {
            Cell::new("Yes").fg(Color::Green)
        } else {
            Cell::new("No").fg(Color::Red)
        };

        table.add_row(Row::from(vec![
            Cell::new(&target.id).fg(Color::Cyan),
            Cell::new(&target.name),
            Cell::new(target.kind.to_string()),
            Cell::new(&target.bucket_or_path),
            Cell::new(target.endpoint.as_deref().unwrap_or("-")),
            enabled_cell,
            Cell::new(target.priority.to_string()),
        ]));
    }

    println!("\n{}", "Distributed Multi-Cloud Storage Mesh Targets:".bold());
    println!("{}", table);
    Ok(())
}

fn handle_add(
    id: &str,
    name: &str,
    kind_str: &str,
    endpoint: Option<String>,
    bucket_or_path: &str,
    access_key: Option<String>,
    secret_key: Option<String>,
    priority: u32,
    paths: &CraftPaths,
) -> Result<()> {
    let kind = MeshTargetKind::from_str(kind_str)?;
    let mut registry = MeshRegistry::load_or_init(paths)?;

    let target = MeshTarget {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        endpoint,
        bucket_or_path: bucket_or_path.to_string(),
        region: None,
        access_key,
        secret_key,
        enabled: true,
        priority,
    };

    registry.add_target(target)?;
    registry.save(paths)?;

    println!(
        "{} Storage mesh target '{}' added successfully.",
        "[OK]".green().bold(),
        id
    );
    Ok(())
}

fn handle_remove(id: &str, paths: &CraftPaths) -> Result<()> {
    let mut registry = MeshRegistry::load_or_init(paths)?;
    registry.remove_target(id)?;
    registry.save(paths)?;

    println!(
        "{} Storage mesh target '{}' removed successfully.",
        "[OK]".green().bold(),
        id
    );
    Ok(())
}

fn handle_sync(server: &str, paths: &CraftPaths) -> Result<()> {
    println!(
        "{}",
        format!(
            "Synchronizing deduplicated backup snapshots for server '{}' to storage mesh...",
            server
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

    let mesh = StorageMesh::new(paths)?;
    let mut synced_chunks = 0;

    let mut chunk_hashes = Vec::new();
    for file in &manifest.files {
        for c in &file.chunks {
            chunk_hashes.push(c.hash.clone());
        }
    }
    chunk_hashes.sort();
    chunk_hashes.dedup();

    for hash in &chunk_hashes {
        if mesh.replicate_chunk(hash).is_ok() {
            synced_chunks += 1;
        }
    }

    mesh.replicate_manifest(&manifest)?;

    println!(
        "\n{} Synchronized {}/{} unique chunks to storage mesh with quorum.",
        "[OK]".green().bold(),
        synced_chunks,
        chunk_hashes.len()
    );
    Ok(())
}

fn handle_health(paths: &CraftPaths) -> Result<()> {
    println!("{}", "Probing multi-cloud storage mesh endpoints...".cyan());

    let mesh = StorageMesh::new(paths)?;
    let reports = mesh.probe_health();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Target ID").fg(Color::Cyan),
            Cell::new("Name").fg(Color::Yellow),
            Cell::new("Kind").fg(Color::Blue),
            Cell::new("Status").fg(Color::Magenta),
            Cell::new("Latency").fg(Color::Cyan),
            Cell::new("Error").fg(Color::Red),
        ]));

    for r in &reports {
        let status_cell = if r.reachable {
            Cell::new("[ONLINE]").fg(Color::Green)
        } else {
            Cell::new("[DOWN]").fg(Color::Red)
        };

        table.add_row(Row::from(vec![
            Cell::new(&r.target_id),
            Cell::new(&r.name),
            Cell::new(&r.kind),
            status_cell,
            Cell::new(format!("{} ms", r.latency_ms)),
            Cell::new(r.error_message.as_deref().unwrap_or("-")),
        ]));
    }

    println!("\n{}", "Storage Mesh Health Matrix:".bold());
    println!("{}", table);
    Ok(())
}
