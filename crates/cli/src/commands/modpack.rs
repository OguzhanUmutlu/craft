use crate::cli::ModpackCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    format_size, CacheStore, CraftError, CraftPaths, GlobalSettings, ModSide, ModpackRegistry,
    Result, ServersRegistry,
};
use craft_plugins::{
    inspect_modpack_archive, install_modpack_archive, BinaryDeltaEngine, ModpackBuilder,
    ModpackKind,
};
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub async fn handle_modpack(action: ModpackCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        ModpackCommands::Inspect { archive } => handle_inspect(&archive),
        ModpackCommands::Install {
            server,
            archive,
            no_cache,
        } => handle_install(&server, &archive, no_cache, paths).await,
        ModpackCommands::Build {
            dir,
            name,
            version,
            loader,
            mc_version,
            target,
            output,
            json,
        } => handle_build(
            &dir,
            name.as_deref(),
            &version,
            &loader,
            &mc_version,
            &target,
            output.as_deref(),
            json,
            paths,
        ),
        ModpackCommands::Delta {
            source,
            target,
            output,
            name,
            src_version,
            target_version,
            json,
        } => handle_delta(
            &source,
            &target,
            output.as_deref(),
            &name,
            &src_version,
            &target_version,
            json,
            paths,
        ),
        ModpackCommands::Patch {
            base,
            patch,
            output,
            json,
        } => handle_patch(&base, &patch, &output, json),
        ModpackCommands::Sync {
            name,
            version,
            client_dir,
            json,
        } => handle_sync(&name, version.as_deref(), &client_dir, json, paths),
    }
}

fn handle_inspect(archive: &Path) -> Result<()> {
    println!("{}", "Inspecting modpack archive...".dimmed());
    let summary = inspect_modpack_archive(archive)?;

    println!("{}", "\n=== Craft Modpack Inspection ===".cyan().bold());
    println!("File:         {}", archive.display().to_string().yellow());
    println!(
        "Format:       {}",
        match summary.kind {
            ModpackKind::Modrinth => "Modrinth (.mrpack)".green().bold(),
            ModpackKind::CurseForge => "CurseForge (manifest.json)".magenta().bold(),
            ModpackKind::Unknown => "Unknown".dimmed(),
        }
    );
    println!("Pack Name:    {}", summary.name.yellow().bold());
    println!("Pack Version: {}", summary.version.cyan());

    if let Some(ref gv) = summary.game_version {
        println!("Game Version: {}", gv.green());
    }
    if let Some(ref loader) = summary.loader {
        println!("Mod Loader:   {}", loader.purple());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Count / Status").fg(Color::Green),
        ]);

    table.add_row(Row::from(vec![
        Cell::new("Total Pack Files"),
        Cell::new(summary.total_files.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Server-Eligible Files"),
        Cell::new(format!("{} (Will be installed)", summary.server_eligible_files)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Client-Only Files"),
        Cell::new(format!("{} (Excluded from server)", summary.client_only_files)),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Has overrides/"),
        Cell::new(if summary.has_overrides { "[YES]" } else { "[NO]" }),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("Has server-overrides/"),
        Cell::new(if summary.has_server_overrides { "[YES]" } else { "[NO]" }),
    ]));

    println!("\n{}", table);
    Ok(())
}

async fn handle_install(
    server_name: &str,
    archive: &Path,
    no_cache: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry
        .find_by_name(server_name)
        .ok_or_else(|| CraftError::Other(format!("Server '{}' not found in registry", server_name)))?;

    if !archive.exists() {
        return Err(CraftError::Other(format!(
            "Modpack archive '{}' does not exist",
            archive.display()
        )));
    }

    let cache_store = if !no_cache {
        let settings = GlobalSettings::load(paths).unwrap_or_default();
        CacheStore::new(paths.cache_dir.clone(), settings.cache_max_bytes).ok()
    } else {
        None
    };

    println!(
        "{} Installing modpack '{}' into server '{}' ({})...",
        "[INFO]".cyan().bold(),
        archive.display(),
        server.name.yellow().bold(),
        server.path.display()
    );

    let summary = install_modpack_archive(archive, &server.path, cache_store.as_ref()).await?;

    println!("\n{}", "=== Modpack Installation Complete ===".green().bold());
    println!("Pack Name:          {}", summary.name.yellow().bold());
    println!("Pack Version:       {}", summary.version.cyan());
    println!("Files Downloaded:   {}", summary.files_downloaded.to_string().green());
    println!("Files from Cache:   {}", summary.files_cached.to_string().cyan());
    println!("Overrides Extracted:{}", summary.overrides_applied.to_string().yellow());
    println!(
        "Data Transferred:   {}",
        format_size(summary.total_bytes).green()
    );

    println!(
        "\n{} Modpack '{}' installed successfully into server '{}'!",
        "[OK]".green().bold(),
        summary.name,
        server_name
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_build(
    dir: &Path,
    explicit_name: Option<&str>,
    version: &str,
    loader: &str,
    mc_version: &str,
    target_dist: &str,
    output_dir: Option<&Path>,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let pack_name = explicit_name.map(|s| s.to_string()).unwrap_or_else(|| {
        dir.canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_else(|| "modpack".to_string())
    });

    let out_dir = match output_dir {
        Some(p) => p.to_path_buf(),
        None => paths.modpack_ci_dir.clone(),
    };
    fs::create_dir_all(&out_dir)?;

    if !json {
        println!(
            "{} Running modpack CI analysis for '{}' ({}) in {}...",
            "[INFO]".cyan().bold(),
            pack_name.yellow().bold(),
            version.cyan(),
            dir.display()
        );
    }

    // Step 1: Validate dependencies
    let missing_deps = ModpackBuilder::validate_dependencies(dir)?;
    if !missing_deps.is_empty() && !json {
        for missing in &missing_deps {
            println!("{} {}", "[WARN]".yellow().bold(), missing);
        }
    }

    // Step 2: Build manifest
    let mut manifest = ModpackBuilder::build_manifest(
        &pack_name,
        version,
        loader,
        mc_version,
        dir,
    )?;

    // Step 3: Export bundles based on target distribution
    let target_norm = target_dist.to_lowercase();
    let export_server = target_norm == "both" || target_norm == "server";
    let export_client = target_norm == "both" || target_norm == "client";

    if export_server {
        let server_archive = out_dir.join(format!("{}-{}-server.tar.zst", pack_name, version));
        let server_hash = ModpackBuilder::export_bundle(
            &manifest,
            dir,
            Some(ModSide::ServerOnly),
            &server_archive,
        )?;
        manifest.server_archive_hash = Some(server_hash);
        manifest.metadata.insert("server_archive".to_string(), server_archive.to_string_lossy().to_string());
    }

    if export_client {
        let client_archive = out_dir.join(format!("{}-{}-client.tar.zst", pack_name, version));
        let client_hash = ModpackBuilder::export_bundle(
            &manifest,
            dir,
            Some(ModSide::ClientOnly),
            &client_archive,
        )?;
        manifest.client_archive_hash = Some(client_hash);
        manifest.metadata.insert("client_archive".to_string(), client_archive.to_string_lossy().to_string());
    }

    manifest.metadata.insert("output_dir".to_string(), out_dir.to_string_lossy().to_string());

    // Step 4: Register build in ModpackRegistry
    let mut registry = ModpackRegistry::load(paths)?;
    registry.register_build(manifest.clone());
    registry.save(paths)?;

    // Step 5: Dispatch scripting lifecycle hook
    let mut ctx = HookContext::new(LifecycleEvent::ModpackBuildCompleted);
    ctx.modpack_name = Some(pack_name.clone());
    ctx.modpack_version = Some(version.to_string());
    HookBus::dispatch_async(
        paths.clone(),
        LifecycleEvent::ModpackBuildCompleted,
        ctx,
        5,
    );

    if json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
    } else {
        println!("\n{}", "=== Modpack Build Complete ===".green().bold());
        println!("Modpack Name:       {}", manifest.name.yellow().bold());
        println!("Version:            {}", manifest.version.cyan());
        println!("Loader:             {}", manifest.loader.purple());
        println!("Minecraft Version:  {}", manifest.minecraft_version.green());
        println!("Total Components:   {}", manifest.components.len().to_string().cyan());

        let client_count = manifest.components.iter().filter(|c| c.side == ModSide::ClientOnly).count();
        let server_count = manifest.components.iter().filter(|c| c.side == ModSide::ServerOnly).count();
        let universal_count = manifest.components.iter().filter(|c| c.side == ModSide::Both).count();

        println!("Client-Only Mods:   {}", client_count.to_string().yellow());
        println!("Server-Only Mods:   {}", server_count.to_string().yellow());
        println!("Universal Mods:     {}", universal_count.to_string().green());

        if let Some(ref sh) = manifest.server_archive_hash {
            println!("Server SHA-256:     {}", sh.dimmed());
        }
        if let Some(ref ch) = manifest.client_archive_hash {
            println!("Client SHA-256:     {}", ch.dimmed());
        }
        println!("Artifacts Dir:      {}", out_dir.display().to_string().green());
        println!("{} Modpack release '{}' packaged successfully!", "[OK]".green().bold(), pack_name);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_delta(
    source_path: &Path,
    target_path: &Path,
    output_path: Option<&Path>,
    pack_name: &str,
    src_version: &str,
    target_version: &str,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    if !source_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "Source archive not found: {}",
            source_path.display()
        )));
    }
    if !target_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "Target archive not found: {}",
            target_path.display()
        )));
    }

    let out_file = match output_path {
        Some(p) => p.to_path_buf(),
        None => {
            fs::create_dir_all(&paths.delta_cache_dir)?;
            paths.delta_cache_dir.join(format!(
                "{}_{}_to_{}.delta",
                pack_name, src_version, target_version
            ))
        }
    };

    if !json {
        println!(
            "{} Computing block-level binary delta between {} and {}...",
            "[INFO]".cyan().bold(),
            source_path.display(),
            target_path.display()
        );
    }

    let delta_manifest = BinaryDeltaEngine::compute_file_delta(
        pack_name,
        src_version,
        target_version,
        source_path,
        target_path,
        &out_file,
        4096,
    )?;

    // Register delta in ModpackRegistry
    let mut registry = ModpackRegistry::load(paths)?;
    registry.register_delta(delta_manifest.clone());
    registry.save(paths)?;

    // Dispatch scripting lifecycle hook
    let mut ctx = HookContext::new(LifecycleEvent::ModpackDeltaPublished);
    ctx.modpack_name = Some(pack_name.to_string());
    ctx.modpack_version = Some(target_version.to_string());
    ctx.delta_size_bytes = Some(delta_manifest.delta_size);
    ctx.savings_percent = Some(delta_manifest.reduction_percent);
    HookBus::dispatch_async(
        paths.clone(),
        LifecycleEvent::ModpackDeltaPublished,
        ctx,
        5,
    );

    if json {
        println!("{}", serde_json::to_string_pretty(&delta_manifest)?);
    } else {
        println!("\n{}", "=== Binary Delta Patch Generated ===".green().bold());
        println!("Modpack Name:       {}", delta_manifest.pack_name.yellow().bold());
        println!("Transition:         {} -> {}", delta_manifest.source_version.cyan(), delta_manifest.target_version.green());
        println!("Delta Patch File:   {}", out_file.display().to_string().yellow());
        println!("Full Target Size:   {}", format_size(delta_manifest.full_size).dimmed());
        println!("Delta Patch Size:   {}", format_size(delta_manifest.delta_size).green().bold());
        println!(
            "Bandwidth Reduction:{} (saved {})",
            format!("{:.1}%", delta_manifest.reduction_percent).green().bold(),
            format_size(delta_manifest.full_size.saturating_sub(delta_manifest.delta_size))
        );
        println!("Target SHA-256:     {}", delta_manifest.target_sha256.dimmed());
        println!("{} Delta patch generated successfully!", "[OK]".green().bold());
    }

    Ok(())
}

fn handle_patch(
    base_path: &Path,
    patch_path: &Path,
    output_path: &Path,
    json: bool,
) -> Result<()> {
    if !base_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "Base archive not found: {}",
            base_path.display()
        )));
    }
    if !patch_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "Delta patch not found: {}",
            patch_path.display()
        )));
    }

    if !json {
        println!(
            "{} Applying binary delta patch '{}' onto base '{}'...",
            "[INFO]".cyan().bold(),
            patch_path.display(),
            base_path.display()
        );
    }

    BinaryDeltaEngine::apply_file_delta(base_path, patch_path, output_path)?;

    if json {
        let resp = serde_json::json!({
            "status": "success",
            "base": base_path.to_string_lossy(),
            "patch": patch_path.to_string_lossy(),
            "output": output_path.to_string_lossy(),
        });
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "{} Successfully reconstructed archive: {}",
            "[OK]".green().bold(),
            output_path.display().to_string().green()
        );
    }

    Ok(())
}

fn handle_sync(
    pack_name: &str,
    target_version: Option<&str>,
    client_dir: &Path,
    json: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let registry = ModpackRegistry::load(paths)?;
    let record = registry.modpacks.get(pack_name).ok_or_else(|| {
        CraftError::Other(format!("Modpack '{}' not found in registry", pack_name))
    })?;

    let manifest = match target_version {
        Some(v) => record.versions.iter().find(|m| m.version == v).ok_or_else(|| {
            CraftError::Other(format!(
                "Version '{}' not found for modpack '{}'",
                v, pack_name
            ))
        })?,
        None => record.versions.last().ok_or_else(|| {
            CraftError::Other(format!("No releases available for modpack '{}'", pack_name))
        })?,
    };

    if !json {
        println!(
            "{} Synchronizing modpack '{}' ({}) into {}...",
            "[INFO]".cyan().bold(),
            pack_name.yellow().bold(),
            manifest.version.cyan(),
            client_dir.display()
        );
    }

    // Look for client or server bundle
    let mut candidate_paths = Vec::new();
    if let Some(archive_str) = manifest.metadata.get("client_archive") {
        candidate_paths.push(PathBuf::from(archive_str));
    }
    if let Some(archive_str) = manifest.metadata.get("server_archive") {
        candidate_paths.push(PathBuf::from(archive_str));
    }
    if let Some(out_dir_str) = manifest.metadata.get("output_dir") {
        let out = PathBuf::from(out_dir_str);
        candidate_paths.push(out.join(format!("{}-{}-client.tar.zst", pack_name, manifest.version)));
        candidate_paths.push(out.join(format!("{}-{}-server.tar.zst", pack_name, manifest.version)));
    }
    candidate_paths.push(paths.modpack_ci_dir.join(format!("{}-{}-client.tar.zst", pack_name, manifest.version)));
    candidate_paths.push(paths.modpack_ci_dir.join(format!("{}-{}-server.tar.zst", pack_name, manifest.version)));

    let archive_path = candidate_paths
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            CraftError::Other(format!(
                "No pre-built archive found for modpack '{}' version '{}'. Run 'craft modpack build' first.",
                pack_name, manifest.version
            ))
        })?;

    fs::create_dir_all(client_dir)?;

    // Extract .tar.zst archive into client_dir
    let file = File::open(&archive_path)?;
    let zstd_decoder = zstd::Decoder::new(file)
        .map_err(|e| CraftError::Other(format!("Failed to initialize zstd decoder: {}", e)))?;
    let mut tar_archive = tar::Archive::new(zstd_decoder);
    tar_archive
        .unpack(client_dir)
        .map_err(|e| CraftError::Other(format!("Failed to unpack modpack archive: {}", e)))?;

    // Dispatch scripting lifecycle hook
    let mut ctx = HookContext::new(LifecycleEvent::ClientSyncRequested);
    ctx.modpack_name = Some(pack_name.to_string());
    ctx.modpack_version = Some(manifest.version.clone());
    HookBus::dispatch_async(
        paths.clone(),
        LifecycleEvent::ClientSyncRequested,
        ctx,
        5,
    );

    if json {
        let resp = serde_json::json!({
            "status": "success",
            "modpack": pack_name,
            "version": manifest.version,
            "client_dir": client_dir.to_string_lossy(),
            "components_count": manifest.components.len(),
        });
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("\n{}", "=== Client Synchronization Complete ===".green().bold());
        println!("Modpack:            {}", pack_name.yellow().bold());
        println!("Version:            {}", manifest.version.cyan());
        println!("Loader:             {}", manifest.loader.purple());
        println!("Client Directory:   {}", client_dir.display().to_string().green());
        println!("Components Synced:  {}", manifest.components.len().to_string().cyan());
        println!("{} Files synchronized and ready to launch!", "[OK]".green().bold());
    }

    Ok(())
}
