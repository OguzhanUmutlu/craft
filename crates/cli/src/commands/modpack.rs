use crate::cli::ModpackCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CacheStore, CraftError, CraftPaths, GlobalSettings, Result, ServersRegistry};
use craft_plugins::{inspect_modpack_archive, install_modpack_archive, ModpackKind};
use std::path::Path;

pub async fn handle_modpack(action: ModpackCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        ModpackCommands::Inspect { archive } => handle_inspect(&archive),
        ModpackCommands::Install {
            server,
            archive,
            no_cache,
        } => handle_install(&server, &archive, no_cache, paths).await,
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
        craft_core::format_size(summary.total_bytes).green()
    );

    println!(
        "\n{} Modpack '{}' installed successfully into server '{}'!",
        "[OK]".green().bold(),
        summary.name,
        server_name
    );

    Ok(())
}
