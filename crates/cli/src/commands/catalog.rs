use crate::cli::CatalogCommands;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result};
use craft_providers::{CatalogBuilder, CatalogManager};
use std::fs;
use std::path::PathBuf;

pub async fn handle_catalog(action: Option<CatalogCommands>, _paths: &CraftPaths) -> Result<()> {
    match action.unwrap_or(CatalogCommands::Info) {
        CatalogCommands::Build { output } => handle_build(output).await,
        CatalogCommands::Update { url } => handle_update(url).await,
        CatalogCommands::Info => handle_info(),
        CatalogCommands::List { software } => handle_list(software),
    }
}

async fn handle_build(output: PathBuf) -> Result<()> {
    println!(
        "{}",
        "Craft Centralized Version Catalog Builder".cyan().bold()
    );
    println!("Querying upstream providers (PaperMC, Purpur, Mojang, Fabric, NeoForge, Spigot, PocketMine, etc.)...");

    let start = std::time::Instant::now();
    let catalog = CatalogBuilder::build_full_catalog().await;
    let total_softwares = catalog.softwares.len();
    let total_versions: usize = catalog.softwares.values().map(|s| s.versions.len()).sum();

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let encoded = catalog.encode_zstd()?;
    fs::write(&output, &encoded)?;

    let elapsed = start.elapsed();
    let size_kb = encoded.len() as f64 / 1024.0;

    println!();
    println!("{}", "Catalog Build Successful:".green().bold());
    println!(
        "  Output File:      {}",
        output.display().to_string().cyan()
    );
    println!("  Compressed Size:  {:.2} KB (zstd level 19)", size_kb);
    println!("  Total Softwares:  {}", total_softwares);
    println!("  Total Versions:   {}", total_versions);
    println!("  Elapsed Time:     {:.2?}", elapsed);
    println!();
    println!("Ready for release publishing or static asset hosting.");

    Ok(())
}

async fn handle_update(custom_url: Option<String>) -> Result<()> {
    let mut mgr = CatalogManager::new()?;
    if let Some(url) = custom_url {
        mgr = mgr.with_remote_url(url);
    }

    println!("{}", "Updating local version catalog...".cyan().bold());
    let catalog = mgr.update().await?;
    let total_versions: usize = catalog.softwares.values().map(|s| s.versions.len()).sum();

    println!("{}", "Version catalog updated successfully:".green().bold());
    println!("  Cached to:       {}", mgr.cache_path().display());
    println!("  Softwares:       {}", catalog.softwares.len());
    println!("  Total Versions:  {}", total_versions);

    Ok(())
}

fn handle_info() -> Result<()> {
    let mgr = CatalogManager::new()?;
    let cache_path = mgr.cache_path();

    println!("{}", "Craft Version Catalog Information".cyan().bold());
    println!("  Cache Path: {}", cache_path.display());

    if cache_path.exists() {
        let meta = fs::metadata(cache_path)?;
        let size_kb = meta.len() as f64 / 1024.0;
        let is_fresh = mgr.is_cache_fresh();
        let status = if is_fresh {
            "Fresh (within 6h TTL)".green().bold()
        } else {
            "Stale (older than 6h TTL)".yellow().bold()
        };

        println!("  Status:     {}", status);
        println!("  File Size:  {:.2} KB", size_kb);

        match mgr.load_from_cache() {
            Ok(catalog) => {
                let total_versions: usize =
                    catalog.softwares.values().map(|s| s.versions.len()).sum();
                println!("  Schema:     v{}", catalog.schema_version);
                println!("  Softwares:  {}", catalog.softwares.len());
                println!("  Versions:   {}", total_versions);
            }
            Err(e) => {
                println!("  Warning:    Failed to parse cache: {}", e);
            }
        }
    } else {
        println!(
            "  Status:     {}",
            "Not Cached (using bundled fallbacks)".yellow()
        );
        println!("  Note:       Run 'craft catalog update' to fetch the latest catalog.");
    }

    Ok(())
}

fn handle_list(software_filter: Option<String>) -> Result<()> {
    let mgr = CatalogManager::new()?;
    let catalog = mgr.load();

    if let Some(sw_id) = software_filter {
        let sw = catalog.get_software(&sw_id).ok_or_else(|| {
            CraftError::UnknownSoftware(format!(
                "Software '{}' not found in catalog. Run 'craft catalog list' to view available software.",
                sw_id
            ))
        })?;

        println!("{} ({})", sw.name.cyan().bold(), sw.edition.dimmed());
        println!("  Game:         {}", sw.game_id);
        println!("  Description:  {}", sw.description);
        println!("  Recommended:  {}", sw.recommended_version.green().bold());
        println!("  Latest:       {}", sw.latest_version);
        println!("  Versions ({} total):", sw.versions.len());
        println!("    {}", sw.versions.join(", "));
    } else {
        println!("{}", "Indexed Server Software Catalog:".cyan().bold());
        println!(
            "  {:<16} {:<24} {:<12} {:<16} {:<8}",
            "ID", "NAME", "EDITION", "RECOMMENDED", "VERSIONS"
        );
        println!("  {}", "-".repeat(80));

        let mut softwares: Vec<_> = catalog.softwares.values().collect();
        softwares.sort_by(|a, b| a.id.cmp(&b.id));

        for sw in softwares {
            println!(
                "  {:<16} {:<24} {:<12} {:<16} {:<8}",
                sw.id.green(),
                sw.name,
                sw.edition,
                sw.recommended_version.cyan(),
                sw.versions.len()
            );
        }
        println!();
        println!(
            "Use 'craft catalog list <software>' to view version details for a specific software."
        );
    }

    Ok(())
}
