use std::fs;
use std::path::PathBuf;
use colored::Colorize;
use craft_core::{find_best_java, get_jar_java_version, CraftError, CraftPaths, Result, ServerConfig, ServersRegistry};
use craft_providers::{find_software, CacheManager};
use crate::commands::run::run_foreground_server;

#[allow(clippy::too_many_arguments)]
pub async fn handle_new(
    software_id: &str,
    version_arg: &str,
    name_arg: &str,
    custom_path: Option<PathBuf>,
    memory: &str,
    agree_eula: bool,
    tmp: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let software = find_software(software_id).ok_or_else(|| {
        CraftError::UnknownSoftware(software_id.to_string())
    })?;

    // Determine target version
    let version = if version_arg == "latest" {
        software.bundled_versions().first().cloned().unwrap_or_else(|| "latest".to_string())
    } else {
        version_arg.to_string()
    };

    // Determine target directory
    let target_dir = if let Some(p) = custom_path {
        p
    } else {
        let folder_name = if !name_arg.is_empty() {
            name_arg.to_string()
        } else {
            format!("{}_{}", software.id(), version.replace('.', "_"))
        };
        paths.servers_dir.join(&folder_name)
    };

    if target_dir.exists() {
        let is_empty = fs::read_dir(&target_dir)
            .map(|mut r| r.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            return Err(CraftError::DirectoryNotEmpty(target_dir.to_string_lossy().to_string()));
        }
    } else {
        fs::create_dir_all(&target_dir)?;
    }

    println!("{}", format!("Setting up {} version {} in '{}'...", software.name(), version, target_dir.display()).cyan());

    // Download assets
    let assets = software.get_assets(&version)?;
    let cache = CacheManager::new(paths);

    for asset in assets {
        cache.fetch_and_install(
            software.id(),
            &version,
            &asset.filename,
            &asset.url,
            &target_dir,
            asset.sha256.as_deref(),
        ).await?;
    }

    // Run post download hooks
    software.post_download(&target_dir, &version).await?;

    // Check Java version requirements for Java edition
    let mut java_path = None;
    if software.edition() == craft_providers::ServerEdition::Java {
        let jar_path = target_dir.join(software.default_server_file());
        if jar_path.exists() {
            if let Ok(req_ver) = get_jar_java_version(&jar_path) {
                println!("{}", format!("Detected bytecode requirement: Java {}", req_ver).dimmed());
                if let Ok(inst) = find_best_java(req_ver) {
                    println!("{}", format!("Selected Java runtime: Java {} ({})", inst.major_version, inst.path.display()).green());
                    java_path = Some(inst.path);
                }
            }
        }
    }

    // Generate start scripts
    software.generate_start_script(&target_dir, &version, java_path.as_deref(), memory)?;

    // Handle EULA
    if agree_eula {
        let eula_file = target_dir.join("eula.txt");
        let _ = fs::write(eula_file, "eula=true\n");
        println!("{}", "EULA accepted automatically via --agree-eula.".green());
    }

    // Register server
    let server_name = target_dir.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "server".to_string());

    let server_config = ServerConfig {
        name: server_name.clone(),
        path: target_dir.clone(),
        software: software.id().to_string(),
        version: version.clone(),
        auto: false,
        java_path,
        memory: Some(memory.to_string()),
        port: None,
        jvm_args: None,
        created_at: Some(chrono::Utc::now()),
    };

    let mut registry = ServersRegistry::load(paths)?;
    let _ = registry.add(server_config);
    registry.save(paths)?;

    println!("{}", format!("Server '{}' successfully installed!", server_name).green().bold());

    // Launch server in foreground
    println!("{}", "Starting server...".cyan());
    let run_res = run_foreground_server(&target_dir).await;

    // If temporary server, clean up on exit
    if tmp {
        println!("{}", "Temporary server: Cleaning up files...".yellow());
        let _ = fs::remove_dir_all(&target_dir);
        let mut reg = ServersRegistry::load(paths)?;
        reg.remove(&target_dir);
        let _ = reg.save(paths);
        println!("{}", "Temporary server removed.".dimmed());
    }

    run_res
}
