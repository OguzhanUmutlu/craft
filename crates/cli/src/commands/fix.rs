use std::fs;
use std::path::PathBuf;
use colored::Colorize;
use craft_core::{find_best_java, get_jar_java_version, CraftError, CraftPaths, Result, ServersRegistry};
use craft_providers::find_software;

pub async fn handle_fix(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if name_arg.is_empty() { None } else { Some(name_arg) },
        true,
    )?;

    if !server_path.exists() {
        return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
    }

    let mut registry = ServersRegistry::load(paths)?;
    let server_config = registry.find_by_path(&server_path).cloned().ok_or_else(|| {
        CraftError::ServerNotFound(format!("Server at '{}' is not registered.", server_path.display()))
    })?;

    println!("{}", format!("Diagnosing and repairing server '{}'...", server_path.display()).cyan());

    let software = find_software(&server_config.software).ok_or_else(|| {
        CraftError::UnknownSoftware(server_config.software.clone())
    })?;

    let mut fixed_items = Vec::new();

    // 1. Inspect Java and re-detect if needed
    let mut resolved_java = server_config.java_path.clone();
    if software.edition() == craft_providers::ServerEdition::Java {
        let jar = server_path.join(software.default_server_file());
        if jar.exists() {
            if let Ok(ver) = get_jar_java_version(&jar) {
                let java_needed = if let Some(ref current_java) = resolved_java {
                    !current_java.exists()
                } else {
                    true
                };

                if java_needed {
                    if let Ok(best) = find_best_java(ver) {
                        resolved_java = Some(best.path);
                        fixed_items.push(format!("Configured Java {} runtime", best.major_version));
                    }
                }
            }
        }
    }

    // 2. Regenerate start script if missing or damaged
    let script_name = if cfg!(windows) { "start.cmd" } else { "start.sh" };
    let script_path = server_path.join(script_name);
    if !script_path.exists() {
        let memory = server_config.memory.as_deref().unwrap_or("2G");
        software.generate_start_script(&server_path, &server_config.version, resolved_java.as_deref(), memory)?;
        fixed_items.push(format!("Regenerated missing {}", script_name));
    }

    // 3. Fix Unix permissions
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if script_path.exists() {
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&script_path, perms);
            fixed_items.push(format!("Set executable permissions on {}", script_name));
        }

        let bedrock_bin = server_path.join("bedrock_server");
        if bedrock_bin.exists() {
            let mut perms = fs::metadata(&bedrock_bin)?.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&bedrock_bin, perms);
            fixed_items.push("Set executable permissions on bedrock_server".to_string());
        }
    }

    // 4. Update registry config
    if let Some(s) = registry.servers.iter_mut().find(|s| s.path == server_path) {
        s.java_path = resolved_java;
    }
    registry.save(paths)?;

    if fixed_items.is_empty() {
        println!("{}", "No issues found. Server configuration is healthy.".green());
    } else {
        println!("{}", "Server repaired successfully:".green().bold());
        for item in fixed_items {
            println!("  ✓ {}", item);
        }
    }

    Ok(())
}
