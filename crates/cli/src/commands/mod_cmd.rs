use crate::cli::ModCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_plugins::{
    apply_atomic_update, check_server_updates, evaluate_compatibility, inspect_jar_manifest,
    resolve_missing_dependencies, PluginManager,
};
use std::collections::{HashMap, HashSet};

pub async fn handle_mod(action: ModCommands, paths: &CraftPaths) -> Result<()> {
    let pm = PluginManager::new();

    match action {
        ModCommands::Search {
            query,
            game_version,
            loader,
        } => {
            let mut filter_desc = Vec::new();
            if let Some(ref gv) = game_version {
                filter_desc.push(format!("game version: {}", gv));
            }
            if let Some(ref l) = loader {
                filter_desc.push(format!("loader: {}", l));
            }
            let filter_str = if filter_desc.is_empty() {
                String::new()
            } else {
                format!(" ({})", filter_desc.join(", "))
            };

            println!(
                "{}",
                format!("Searching mods for '{}' on Modrinth{}...", query, filter_str).cyan()
            );
            let results = pm.search_mods(&query).await;

            if results.is_empty() {
                println!("{}", "No mods found.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("ID / Slug").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            for hit in results {
                let desc = craft_core::truncate_ellipsis(&hit.description, 60);

                table.add_row(Row::from(vec![
                    Cell::new(hit.name).fg(Color::Green),
                    Cell::new(hit.id_or_slug),
                    Cell::new(desc),
                ]));
            }

            println!("{table}");
            println!("\nInstall via: craft mod install <id> <server_name>");
        }
        ModCommands::Install {
            project_id,
            server,
            version: _explicit_ver,
            resolve_deps,
        } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.mods {
                return Err(CraftError::Other(format!(
                    "Server '{}' (software: {}) does not support mods.\nMods only exist in modded server softwares (e.g. Fabric, Quilt, NeoForge, Forge).",
                    s.name, s.software
                )));
            }

            println!(
                "{}",
                format!(
                    "Installing mod '{}' to '{}' (software: {}, Minecraft: {})...",
                    project_id,
                    server_path.display(),
                    s.software,
                    s.version
                )
                .cyan()
            );

            let loaders = vec![s.software.as_str()];
            let gvs = vec![s.version.as_str()];
            let dest = pm
                .install_mod_from_modrinth_compatible(&server_path, &project_id, &loaders, &gvs)
                .await?;

            println!(
                "{}",
                format!("[OK] Successfully installed mod to '{}'!", dest.display())
                    .green()
                    .bold()
            );

            // Manifest inspection and compatibility check
            if let Ok(manifest) = inspect_jar_manifest(&dest) {
                let report = evaluate_compatibility(&manifest, &s.version, &s.software);
                for warning in &report.warnings {
                    println!("  {} {}", "[WARN]".yellow().bold(), warning);
                }
                for error in &report.errors {
                    println!("  {} {}", "[ERROR]".red().bold(), error);
                }

                // Automatic dependency resolution
                if resolve_deps && !manifest.dependencies.is_empty() {
                    println!(
                        "{}",
                        "Checking and resolving mod dependencies...".cyan().dimmed()
                    );
                    let dep_res = resolve_missing_dependencies(
                        &server_path,
                        &manifest,
                        Some(&s.version),
                        Some(&s.software),
                    )
                    .await?;

                    for installed in &dep_res.already_installed {
                        println!(
                            "  {} Dependency '{}' is already satisfied.",
                            "[OK]".green().bold(),
                            installed
                        );
                    }

                    for resolved in dep_res.resolved {
                        println!(
                            "  {} Resolving missing dependency '{}' from {}...",
                            "->".blue().bold(),
                            resolved.name,
                            resolved.source
                        );
                        if let Ok(dep_dest) = pm
                            .install_mod_from_modrinth_compatible(
                                &server_path,
                                &resolved.project_id,
                                &loaders,
                                &gvs,
                            )
                            .await
                        {
                            println!(
                                "  {} Installed dependency '{}' to '{}'.",
                                "[OK]".green().bold(),
                                resolved.name,
                                dep_dest.display()
                            );
                        } else {
                            println!(
                                "  {} Could not automatically install dependency '{}'.",
                                "[WARN]".yellow().bold(),
                                resolved.name
                            );
                        }
                    }

                    for missing in &dep_res.missing {
                        println!(
                            "  {} Unsatisfied dependency '{}' (required by '{}'). Please install it manually.",
                            "[WARN]".yellow().bold(),
                            missing,
                            manifest.id_or_name
                        );
                    }
                }
            }
        }
        ModCommands::Update { server, check, yes } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            let mods_dir = server_path.join("mods");

            println!(
                "{}",
                format!(
                    "Checking for mod updates on server '{}' ({} {})...",
                    s.name, s.software, s.version
                )
                .cyan()
            );

            let candidates = check_server_updates(
                &server_path,
                Some(&s.version),
                Some(&s.software),
            )
            .await?;

            let candidates: Vec<_> = candidates
                .into_iter()
                .filter(|c| c.file_path.starts_with(&mods_dir))
                .collect();

            if candidates.is_empty() {
                println!(
                    "{}",
                    "No installed mods found or no upstream update hashes matched.".yellow()
                );
                return Ok(());
            }

            let updates_available: Vec<&_> = candidates.iter().filter(|c| c.has_update).collect();

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Mod").fg(Color::Cyan),
                Cell::new("Installed").fg(Color::Cyan),
                Cell::new("Latest Upstream").fg(Color::Cyan),
                Cell::new("Status").fg(Color::Cyan),
            ]);

            for c in &candidates {
                let status_cell = if c.has_update {
                    Cell::new("[UPDATE AVAILABLE]").fg(Color::Yellow)
                } else {
                    Cell::new("[UP TO DATE]").fg(Color::Green)
                };

                table.add_row(Row::from(vec![
                    Cell::new(&c.project_name),
                    Cell::new(&c.installed_version),
                    Cell::new(&c.latest_version),
                    status_cell,
                ]));
            }

            println!("{table}");

            if updates_available.is_empty() {
                println!("{}", "\nAll mods are up to date!".green().bold());
                return Ok(());
            }

            println!(
                "\n{} update(s) available.",
                updates_available.len().to_string().yellow().bold()
            );

            if check {
                println!(
                    "Run 'craft mod update {}' to apply all updates with atomic rollback protection.",
                    s.name
                );
                return Ok(());
            }

            if !yes {
                println!(
                    "{}",
                    "Applying updates atomically with backup rollback protection...".cyan()
                );
            }

            for update in updates_available {
                println!(
                    "  {} Updating '{}' ({} -> {})...",
                    "->".blue().bold(),
                    update.project_name,
                    update.installed_version,
                    update.latest_version
                );
                match apply_atomic_update(update, paths).await {
                    Ok(dest) => {
                        println!(
                            "  {} Successfully updated to '{}'.",
                            "[OK]".green().bold(),
                            dest.display()
                        );
                    }
                    Err(e) => {
                        println!(
                            "  {} Failed to update '{}': {}. Original file restored.",
                            "[ERROR]".red().bold(),
                            update.project_name,
                            e
                        );
                    }
                }
            }

            println!("{}", "\n[SUCCESS] Mod update process complete.".green().bold());
        }
        ModCommands::Inspect { file } => {
            if !file.exists() {
                return Err(CraftError::InvalidPath(format!(
                    "File not found: {}",
                    file.display()
                )));
            }

            println!(
                "{}",
                format!("Inspecting JAR manifest for '{}'...", file.display()).cyan()
            );

            let manifest = inspect_jar_manifest(&file)?;

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Property").fg(Color::Cyan),
                Cell::new("Value").fg(Color::Cyan),
            ]);

            table.add_row(Row::from(vec![
                Cell::new("Plugin / Mod ID"),
                Cell::new(&manifest.id_or_name).fg(Color::Green),
            ]));
            if let Some(ref dn) = manifest.display_name {
                table.add_row(Row::from(vec![
                    Cell::new("Display Name"),
                    Cell::new(dn),
                ]));
            }
            table.add_row(Row::from(vec![
                Cell::new("Version"),
                Cell::new(&manifest.version).fg(Color::Yellow),
            ]));
            table.add_row(Row::from(vec![
                Cell::new("Manifest Kind"),
                Cell::new(manifest.kind.to_string()),
            ]));
            if let Some(ref api) = manifest.api_version {
                table.add_row(Row::from(vec![
                    Cell::new("API Version"),
                    Cell::new(api),
                ]));
            }
            if let Some(ref mc) = manifest.main_class {
                table.add_row(Row::from(vec![
                    Cell::new("Main Class"),
                    Cell::new(mc),
                ]));
            }
            if !manifest.authors.is_empty() {
                table.add_row(Row::from(vec![
                    Cell::new("Authors"),
                    Cell::new(manifest.authors.join(", ")),
                ]));
            }
            if let Some(ref desc) = manifest.description {
                table.add_row(Row::from(vec![
                    Cell::new("Description"),
                    Cell::new(desc),
                ]));
            }

            println!("{table}");

            if !manifest.dependencies.is_empty() {
                println!("\n{}", "Declared Dependencies:".cyan().bold());
                let mut dep_table = Table::new();
                dep_table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS);
                dep_table.set_header(vec![
                    Cell::new("Dependency").fg(Color::Cyan),
                    Cell::new("Required / Hard").fg(Color::Cyan),
                    Cell::new("Version Range").fg(Color::Cyan),
                ]);

                for dep in &manifest.dependencies {
                    dep_table.add_row(Row::from(vec![
                        Cell::new(&dep.name_or_id),
                        Cell::new(if dep.required { "[REQUIRED]" } else { "[OPTIONAL]" }),
                        Cell::new(dep.version_range.as_deref().unwrap_or("-")),
                    ]));
                }
                println!("{dep_table}");
            }
        }
        ModCommands::Doctor { server } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            println!(
                "{}",
                format!(
                    "Running mod diagnostic audit for server '{}' ({} {})...",
                    s.name, s.software, s.version
                )
                .bold()
                .cyan()
            );

            let mods_dir = server_path.join("mods");
            if !mods_dir.exists() {
                println!("{}", "No 'mods' directory found on server.".yellow());
                return Ok(());
            }

            let mut manifests = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&mods_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("jar") {
                        if let Ok(m) = inspect_jar_manifest(&p) {
                            manifests.push((p, m));
                        }
                    }
                }
            }

            if manifests.is_empty() {
                println!("{}", "No mod JAR files found to audit.".yellow());
                return Ok(());
            }

            println!("Discovered {} installed mod JAR(s).\n", manifests.len());

            let mut installed_names: HashSet<String> = HashSet::new();
            let mut name_counts: HashMap<String, usize> = HashMap::new();

            for (_, m) in &manifests {
                let lower = m.id_or_name.to_lowercase();
                installed_names.insert(lower.clone());
                *name_counts.entry(lower).or_insert(0) += 1;
            }

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Mod").fg(Color::Cyan),
                Cell::new("Version").fg(Color::Cyan),
                Cell::new("Type").fg(Color::Cyan),
                Cell::new("Dependencies").fg(Color::Cyan),
                Cell::new("Health Status").fg(Color::Cyan),
            ]);

            let mut total_warnings = 0;
            let mut total_errors = 0;

            for (_path, m) in &manifests {
                let mut issues = Vec::new();

                // Check duplicate names
                if *name_counts.get(&m.id_or_name.to_lowercase()).unwrap_or(&0) > 1 {
                    issues.push("Duplicate mod identifier detected".to_string());
                    total_errors += 1;
                }

                // Check missing hard dependencies
                for dep in &m.dependencies {
                    if dep.required {
                        let lower = dep.name_or_id.to_lowercase();
                        if lower != "minecraft"
                            && lower != "java"
                            && lower != "forge"
                            && lower != "neoforge"
                            && lower != "fabricloader"
                            && !installed_names.contains(&lower)
                        {
                            issues.push(format!("Missing dependency: '{}'", dep.name_or_id));
                            total_errors += 1;
                        }
                    }
                }

                // Check compatibility
                let report = evaluate_compatibility(m, &s.version, &s.software);
                for w in report.warnings {
                    issues.push(w);
                    total_warnings += 1;
                }
                for e in report.errors {
                    issues.push(e);
                    total_errors += 1;
                }

                let (status_text, status_color) = if issues.is_empty() {
                    ("[PASS] Healthy".to_string(), Color::Green)
                } else if !report.is_compatible
                    || !issues
                        .iter()
                        .all(|i| i.contains("Warning") || i.contains("may not satisfy"))
                {
                    (format!("[FAIL] {}", issues.join("; ")), Color::Red)
                } else {
                    (format!("[WARN] {}", issues.join("; ")), Color::Yellow)
                };

                let dep_summary = if m.dependencies.is_empty() {
                    "-".to_string()
                } else {
                    format!("{} total", m.dependencies.len())
                };

                table.add_row(Row::from(vec![
                    Cell::new(&m.id_or_name),
                    Cell::new(&m.version),
                    Cell::new(m.kind.to_string()),
                    Cell::new(dep_summary),
                    Cell::new(status_text).fg(status_color),
                ]));
            }

            println!("{table}");

            println!();
            if total_errors == 0 && total_warnings == 0 {
                println!(
                    "{}",
                    "[AUDIT PASSED] All mods are healthy, compatible, and dependencies are satisfied."
                        .green()
                        .bold()
                );
            } else {
                println!(
                    "Audit summary: {} error(s), {} warning(s) detected.",
                    total_errors.to_string().red().bold(),
                    total_warnings.to_string().yellow().bold()
                );
            }
        }
        ModCommands::List { server } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let registry = ServersRegistry::load(paths)?;

            let s = match registry.find_by_path(&server_path) {
                Some(s) => s,
                None => {
                    return Err(CraftError::ServerNotFound(format!(
                        "Server '{}' is not registered.",
                        server
                    )))
                }
            };

            let caps = craft_providers::get_content_capabilities(&s.software);
            if !caps.mods {
                println!(
                    "{}",
                    format!(
                        "[NOTE] Server '{}' (software: {}) does not support mods.",
                        s.name, s.software
                    )
                    .yellow()
                );
                return Ok(());
            }

            let mods_dir = server_path.join("mods");
            let mut installed = Vec::new();
            if mods_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&mods_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("jar")
                        {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                            let manifest = inspect_jar_manifest(&path).ok();
                            installed.push((name, size, manifest));
                        }
                    }
                }
            }

            installed.sort_by(|a, b| a.0.cmp(&b.0));

            if installed.is_empty() {
                println!(
                    "{}",
                    format!("No mods installed in '{}'.", mods_dir.display()).yellow()
                );
                return Ok(());
            }

            println!(
                "{}",
                format!("Installed mods in '{}' ({}):", s.name, installed.len())
                    .cyan()
                    .bold()
            );
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Mod File").fg(Color::Cyan),
                Cell::new("Version").fg(Color::Cyan),
                Cell::new("Size").fg(Color::Cyan),
                Cell::new("Type").fg(Color::Cyan),
            ]);

            for (name, size, manifest) in installed {
                let (ver, kind) = if let Some(ref m) = manifest {
                    (m.version.clone(), m.kind.to_string())
                } else {
                    ("-".to_string(), "Unknown".to_string())
                };

                table.add_row(Row::from(vec![
                    Cell::new(name).fg(Color::Green),
                    Cell::new(ver),
                    Cell::new(craft_core::format_size(size)).fg(Color::Yellow),
                    Cell::new(kind),
                ]));
            }
            println!("{table}");
        }
        ModCommands::Remove { server, filename } => {
            let server_path = paths.resolve_server_path(None, Some(&server), true)?;
            let mods_dir = server_path.join("mods");
            let target = mods_dir.join(&filename);

            let file_to_delete = if target.exists() && target.is_file() {
                target
            } else {
                let target_with_ext = mods_dir.join(format!("{}.jar", filename));
                if target_with_ext.exists() && target_with_ext.is_file() {
                    target_with_ext
                } else {
                    return Err(CraftError::Other(format!(
                        "Mod file '{}' not found in '{}'.",
                        filename,
                        mods_dir.display()
                    )));
                }
            };

            std::fs::remove_file(&file_to_delete)?;
            println!(
                "{}",
                format!("[OK] Removed mod '{}'.", file_to_delete.display())
                    .green()
                    .bold()
            );
        }
    }

    Ok(())
}
