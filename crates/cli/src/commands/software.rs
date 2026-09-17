use colored::Colorize;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use craft_core::{CraftError, CraftPaths, Result};
use craft_providers::SoftwareRegistry;
use craft_scripting::{load_bundle, package_directory, SoftwareDefinition};
use std::fs;
use std::path::{Path, PathBuf};

pub async fn handle_software(
    action: crate::cli::SoftwareCommands,
    paths: &CraftPaths,
) -> Result<()> {
    match action {
        crate::cli::SoftwareCommands::List { game } => handle_list(game, paths),
        crate::cli::SoftwareCommands::Inspect { id } => handle_inspect(&id, paths),
        crate::cli::SoftwareCommands::Package { dir, output } => {
            handle_package(&dir, output.as_deref())
        }
        crate::cli::SoftwareCommands::Install { package, force } => {
            handle_install(&package, force, paths)
        }
        crate::cli::SoftwareCommands::Remove { id } => handle_remove(&id, paths),
        crate::cli::SoftwareCommands::Reset { id, all } => {
            handle_reset(id.as_deref(), all, paths)
        }
        crate::cli::SoftwareCommands::Template { id, path } => {
            handle_template(&id, path.as_deref())
        }
    }
}

fn handle_list(game_filter: Option<String>, paths: &CraftPaths) -> Result<()> {
    let registry = SoftwareRegistry::load(paths);
    let all_softwares = registry.get_all();

    let filtered: Vec<_> = all_softwares
        .into_iter()
        .filter(|s| {
            if let Some(ref g) = game_filter {
                s.game_id().eq_ignore_ascii_case(g)
            } else {
                true
            }
        })
        .collect();

    println!(
        "\n{}",
        "Craft Server Software Definitions Catalog".cyan().bold()
    );
    if let Some(ref g) = game_filter {
        println!("Filtered by game: {}", g.yellow().bold());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").set_alignment(CellAlignment::Left),
            Cell::new("Name").set_alignment(CellAlignment::Left),
            Cell::new("Game").set_alignment(CellAlignment::Left),
            Cell::new("Edition").set_alignment(CellAlignment::Left),
            Cell::new("Port").set_alignment(CellAlignment::Right),
            Cell::new("Type").set_alignment(CellAlignment::Center),
            Cell::new("Description").set_alignment(CellAlignment::Left),
        ]);

    for sw in &filtered {
        let bundle = registry.get_bundle(sw.id());
        let is_builtin = registry.is_builtin_id(sw.id());
        let sw_type = if !is_builtin {
            "Custom".yellow().bold().to_string()
        } else if bundle.is_some_and(|b| b.source_path.is_some()) {
            "Default (Editable)".green().to_string()
        } else {
            "Built-in".green().to_string()
        };

        let edition_str = match sw.edition() {
            craft_providers::ServerEdition::Java => "Java",
            craft_providers::ServerEdition::Bedrock => "Bedrock",
            craft_providers::ServerEdition::Proxy => "Proxy",
            craft_providers::ServerEdition::Native => "Native",
        };

        let (port, _) = sw.default_ports();

        table.add_row(vec![
            Cell::new(sw.id()).set_alignment(CellAlignment::Left),
            Cell::new(sw.name()).set_alignment(CellAlignment::Left),
            Cell::new(sw.game_id()).set_alignment(CellAlignment::Left),
            Cell::new(edition_str).set_alignment(CellAlignment::Left),
            Cell::new(port.to_string()).set_alignment(CellAlignment::Right),
            Cell::new(sw_type).set_alignment(CellAlignment::Center),
            Cell::new(sw.description()).set_alignment(CellAlignment::Left),
        ]);
    }

    println!("{table}");
    println!(
        "Total definitions: {} (Install custom packages in {})\n",
        filtered.len().to_string().cyan().bold(),
        paths.softwares_dir.display().to_string().dimmed()
    );

    Ok(())
}

fn handle_inspect(id: &str, paths: &CraftPaths) -> Result<()> {
    let registry = SoftwareRegistry::load(paths);
    let sw = registry
        .find(id)
        .ok_or_else(|| CraftError::Other(format!("Software definition '{}' not found", id)))?;

    let bundle = registry.get_bundle(sw.id());

    println!(
        "\n{} {}",
        "Server Software Definition:".cyan().bold(),
        sw.name().white().bold()
    );
    println!("{}", "=".repeat(60).dimmed());

    println!("  {:<20}: {}", "ID".dimmed(), sw.id().cyan().bold());
    println!("  {:<20}: {}", "Game".dimmed(), sw.game_id().yellow());
    println!("  {:<20}: {:?}", "Edition".dimmed(), sw.edition());
    println!("  {:<20}: {}", "Description".dimmed(), sw.description());

    let (port, query_port) = sw.default_ports();
    println!("  {:<20}: {}", "Default Port".dimmed(), port);
    if let Some(qp) = query_port {
        println!("  {:<20}: {}", "Query Port".dimmed(), qp);
    }
    println!(
        "  {:<20}: {}",
        "Server File".dimmed(),
        sw.default_server_file()
    );

    let bundled = sw.bundled_versions();
    println!(
        "  {:<20}: {} versions ({})",
        "Bundled Versions".dimmed(),
        bundled.len(),
        sw.recommended_version().green().bold()
    );

    if let Some(b) = bundle {
        let is_builtin = registry.is_builtin_id(sw.id());
        let source_str = if let Some(ref src) = b.source_path {
            if is_builtin {
                format!("Default (Editable: {})", src.display())
            } else {
                format!("Custom ({})", src.display())
            }
        } else {
            "Built-in (Embedded)".to_string()
        };
        println!("  {:<20}: {}", "Source".dimmed(), source_str);

        println!(
            "  {:<20}: Plugins: {}, Mods: {}, Datapacks: {}, RCON: {}",
            "Capabilities".dimmed(),
            b.definition.capabilities.plugins,
            b.definition.capabilities.mods,
            b.definition.capabilities.datapacks,
            b.definition.capabilities.rcon
        );

        if !b.scripts.is_empty() {
            println!(
                "  {:<20}: {}",
                "Custom Scripts".dimmed(),
                b.scripts.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }

        if let Some(ref schema) = b.properties_schema {
            println!(
                "\n{} ({} properties across {} categories)",
                "Properties Schema:".cyan().bold(),
                schema.properties.len(),
                schema.categories.len()
            );
            println!("{}", "-".repeat(60).dimmed());
            for cat in &schema.categories {
                println!("  [Category: {}]", cat.name.white().bold());
                for prop in schema.properties_for_category(&cat.id) {
                    let type_str = match prop.property_type {
                        craft_scripting::PropertyType::String => "string",
                        craft_scripting::PropertyType::Integer => "integer",
                        craft_scripting::PropertyType::Boolean => "boolean",
                        craft_scripting::PropertyType::Enum => "enum",
                    };
                    println!(
                        "    * {:<24} ({}) - {}",
                        prop.key.cyan(),
                        type_str.dimmed(),
                        prop.label
                    );
                }
            }
        }
    }

    println!();
    Ok(())
}

fn handle_package(dir: &Path, output: Option<&Path>) -> Result<()> {
    if !dir.exists() || !dir.is_dir() {
        return Err(CraftError::Other(format!(
            "Directory '{}' does not exist",
            dir.display()
        )));
    }

    let manifest_path = dir.join("software.toml");
    if !manifest_path.exists() {
        return Err(CraftError::Other(format!(
            "Directory '{}' is missing software.toml",
            dir.display()
        )));
    }

    let def = SoftwareDefinition::load_from_file(&manifest_path).map_err(CraftError::Other)?;

    let out_path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        let filename = format!("{}.zip", def.id());
        PathBuf::from(filename)
    };

    println!(
        "Packaging server software '{}' from {}...",
        def.id().cyan().bold(),
        dir.display()
    );

    package_directory(dir, &out_path).map_err(CraftError::Other)?;

    let meta = fs::metadata(&out_path)?;
    println!(
        "{} Successfully created package: {} ({} bytes)",
        "[OK]".green().bold(),
        out_path.display().to_string().cyan().bold(),
        meta.len()
    );

    Ok(())
}

fn handle_install(package: &Path, force: bool, paths: &CraftPaths) -> Result<()> {
    if !package.exists() {
        return Err(CraftError::Other(format!(
            "Package path '{}' does not exist",
            package.display()
        )));
    }

    let bundle = load_bundle(package).map_err(CraftError::Other)?;
    let id = bundle.id();

    fs::create_dir_all(&paths.softwares_dir)?;

    let target_file = paths.softwares_dir.join(format!("{}.zip", id));
    if target_file.exists() && !force {
        return Err(CraftError::Other(format!(
            "Software '{}' already exists at {}. Use --force to overwrite.",
            id,
            target_file.display()
        )));
    }

    if package.is_dir() {
        // Package the directory directly into ~/.craft/softwares/<id>.zip
        package_directory(package, &target_file).map_err(CraftError::Other)?;
    } else {
        fs::copy(package, &target_file)?;
    }

    println!(
        "{} Installed software definition '{}' into {}",
        "[OK]".green().bold(),
        id.cyan().bold(),
        target_file.display().to_string().dimmed()
    );
    println!(
        "You can now create servers using: craft new <name> -s {}",
        id.yellow()
    );

    Ok(())
}

fn handle_remove(id: &str, paths: &CraftPaths) -> Result<()> {
    let lower = id.to_lowercase();
    let zip_file = paths.softwares_dir.join(format!("{}.zip", lower));
    let craft_file = paths.softwares_dir.join(format!("{}.craft", lower));
    let sw_dir = paths.softwares_dir.join(&lower);

    let mut removed = false;
    if zip_file.exists() {
        fs::remove_file(&zip_file)?;
        removed = true;
    }
    if craft_file.exists() {
        fs::remove_file(&craft_file)?;
        removed = true;
    }
    if sw_dir.exists() && sw_dir.is_dir() {
        fs::remove_dir_all(&sw_dir)?;
        removed = true;
    }

    if removed {
        println!(
            "{} Removed custom software definition '{}'",
            "[OK]".green().bold(),
            lower.cyan().bold()
        );
    } else {
        println!(
            "Software '{}' was not found in installed custom software definitions.",
            lower
        );
    }

    Ok(())
}

fn handle_reset(id: Option<&str>, all: bool, paths: &CraftPaths) -> Result<()> {
    if !all && id.is_none() {
        return Err(CraftError::Other(
            "Please specify a software definition ID to reset, or pass --all to restore all defaults.".to_string(),
        ));
    }

    let count = SoftwareRegistry::reset_defaults(paths, id).map_err(CraftError::Other)?;

    if let Some(specific) = id {
        println!(
            "{} Restored default software definition '{}' in {}",
            "[OK]".green().bold(),
            specific.cyan().bold(),
            paths.softwares_dir.join(specific).display().to_string().dimmed()
        );
    } else {
        println!(
            "{} Restored {} default software definitions in {}",
            "[OK]".green().bold(),
            count.to_string().cyan().bold(),
            paths.softwares_dir.display().to_string().dimmed()
        );
    }

    Ok(())
}

fn handle_template(id: &str, path: Option<&Path>) -> Result<()> {
    let target_dir = if let Some(p) = path {
        p.to_path_buf()
    } else {
        PathBuf::from(id)
    };

    if target_dir.exists() {
        return Err(CraftError::Other(format!(
            "Target directory '{}' already exists",
            target_dir.display()
        )));
    }

    fs::create_dir_all(target_dir.join("scripts"))?;
    fs::create_dir_all(target_dir.join("templates"))?;

    let manifest = format!(
        r#"[software]
id = "{id}"
name = "{name}"
display_name = "{name} Server"
game = "{id}"
edition = "native"
description = "Custom dedicated server for {name}"
author = "Your Name"
version = "1.0.0"

[capabilities]
plugins = false
mods = false
datapacks = false
rcon = false

[runtime]
kind = "native"
default_server_file = "server"
arguments = ["--port", "{{port}}"]
stop_method = "sigterm"
stop_timeout_seconds = 10

[network]
default_port = 8080
protocol = "tcp"

[versions]
recommended = "1.0.0"
bundled = ["1.0.0"]
fetch_mode = "static"

[assets]
download_mode = "url_template"
url_template = "https://example.com/downloads/v{{version}}/server.tar.gz"
filename = "server.tar.gz"
is_archive = true

[properties]
file = "server.properties"
format = "properties"
schema_file = "properties.toml"

[developer]
reload_command = "reload"
"#,
        id = id,
        name = id.replace('-', " ").to_uppercase()
    );

    let properties_schema = r#"[meta]
file = "server.properties"
format = "properties"

[[categories]]
id = "network"
name = "Network & Server"

[[categories]]
id = "gameplay"
name = "Gameplay Settings"

[[properties]]
key = "server-port"
category = "network"
label = "Server Port"
description = "Port to listen on"
type = "integer"
default = 8080
min = 1
max = 65535

[[properties]]
key = "server-name"
category = "network"
label = "Server Name"
type = "string"
default = "My Custom Server"

[[properties]]
key = "difficulty"
category = "gameplay"
label = "Difficulty"
type = "enum"
options = ["easy", "normal", "hard"]
default = "normal"
"#;

    let hooks_lua = r#"-- Lifecycle hooks for server
function on_pre_start(server)
    craft.info("Starting " .. server.name .. " on port " .. tostring(server.port))
end

function on_post_start(server, pid)
    craft.info("Started with PID: " .. tostring(pid))
end

function on_pre_stop(server, pid)
    craft.info("Stopping PID: " .. tostring(pid))
end

function on_post_stop(server, exit_code)
    craft.info("Server exited with code: " .. tostring(exit_code))
end
"#;

    let readme = format!(
        r#"# {name} Server Software Definition

This directory is a complete Craft Server Software Definition for **{name}**.

## Directory Structure
- `software.toml`: Primary definition manifest (ports, runtime, versions, assets)
- `properties.toml`: Categorized schema for server configuration in TUI forms
- `scripts/hooks.lua`: Optional lifecycle event hooks
- `templates/`: Optional scaffolding files for developer tools

## Testing & Packaging
1. Package this directory into a `.zip` bundle:
   ```bash
   craft software package . {id}.zip
   ```
2. Install it locally:
   ```bash
   craft software install {id}.zip
   ```
3. Create a server using your new software:
   ```bash
   craft new my-test-server -s {id}
   ```
"#,
        name = id.to_uppercase(),
        id = id
    );

    fs::write(target_dir.join("software.toml"), manifest)?;
    fs::write(target_dir.join("properties.toml"), properties_schema)?;
    fs::write(target_dir.join("scripts/hooks.lua"), hooks_lua)?;
    fs::write(target_dir.join("README.md"), readme)?;

    println!(
        "{} Created server software definition scaffolding in {}",
        "[OK]".green().bold(),
        target_dir.display().to_string().cyan().bold()
    );
    println!(
        "Edit {} to customize your server definition.",
        target_dir.join("software.toml").display()
    );

    Ok(())
}
