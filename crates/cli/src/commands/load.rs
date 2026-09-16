use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServerConfig, ServersRegistry};
use craft_providers::find_software;

pub async fn handle_load(
    server_path: PathBuf,
    software_id: &str,
    version: &str,
    name_arg: &str,
    paths: &CraftPaths,
) -> Result<()> {
    if !server_path.exists() || !server_path.is_dir() {
        return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
    }

    let software = find_software(software_id).ok_or_else(|| {
        CraftError::UnknownSoftware(software_id.to_string())
    })?;

    let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.clone());
    let mut registry = ServersRegistry::load(paths)?;

    if registry.find_by_path(&canonical).is_some() {
        return Err(CraftError::ServerAlreadyExists(canonical.to_string_lossy().to_string()));
    }

    let name = if !name_arg.is_empty() {
        name_arg.to_string()
    } else {
        canonical
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "server".to_string())
    };

    let (default_port, query_port) = software.default_ports();
    let server_config = ServerConfig {
        name: name.clone(),
        path: canonical.clone(),
        software: software.id().to_string(),
        version: version.to_string(),
        game: software.game_id().to_string(),
        auto: false,
        java_path: None,
        memory: Some("2G".to_string()),
        port: Some(default_port),
        query_port,
        rcon_port: None,
        jvm_args: None,
        start_args: None,
        binary_path: None,
        created_at: Some(chrono::Utc::now()),
        backup_method: None,
        jdwp_debug_port: None,
    };

    registry.add(server_config)?;
    registry.save(paths)?;

    println!("{}", format!("Successfully loaded existing server '{}' ({}) into Craft!", name, canonical.display()).green().bold());
    Ok(())
}
