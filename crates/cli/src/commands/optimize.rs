use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    CraftError, CraftPaths, MemoryOptimizer, OptimizationProfile, Result, ServerProperties,
    ServersRegistry,
};
use sysinfo::System;

pub fn handle_optimize(
    name: &str,
    profile_str: &str,
    apply: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let mut registry = ServersRegistry::load(paths)?;
    let server_index = registry
        .servers
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| CraftError::Other(format!("Server '{}' not found in registry", name)))?;

    let profile = match profile_str.to_lowercase().as_str() {
        "conservative" | "safe" => OptimizationProfile::Conservative,
        "aggressive" | "max" | "perf" => OptimizationProfile::Aggressive,
        _ => OptimizationProfile::Balanced,
    };

    let mut sys = System::new_all();
    sys.refresh_all();
    let total_ram_mb = sys.total_memory() / (1024 * 1024);
    let avail_ram_mb = sys.available_memory() / (1024 * 1024);
    let cpu_cores = sys.cpus().len();

    // Attempt to read max-players from server.properties
    let player_cap = {
        let server = &registry.servers[server_index];
        let prop_path = server.path.join("server.properties");
        if prop_path.exists() {
            ServerProperties::load(&prop_path)
                .ok()
                .and_then(|p| p.get("max-players").and_then(|v| v.parse::<u32>().ok()))
        } else {
            None
        }
    };

    let server = &registry.servers[server_index];
    let recommendation = MemoryOptimizer::evaluate(
        total_ram_mb,
        avail_ram_mb,
        Some(21),
        &server.software,
        player_cap,
        profile,
        None,
    );

    println!("{}", "\n=== Craft JVM & Memory Resource Optimizer ===".cyan().bold());
    println!("Server:       {}", server.name.yellow().bold());
    println!("Software:     {} ({})", server.software.green(), server.version);
    println!("Host Specs:   {} RAM, {} logical cores", format!("{} MB", total_ram_mb).cyan(), cpu_cores.to_string().cyan());
    println!("Profile:      {}", format!("{:?}", profile).purple().bold());
    if let Some(cap) = player_cap {
        println!("Player Cap:   {}", cap.to_string().yellow());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Parameter").fg(Color::Cyan),
            Cell::new("Current Setting").fg(Color::Yellow),
            Cell::new("Recommended Setting").fg(Color::Green),
        ]);

    let curr_mem = server.memory.clone().unwrap_or_else(|| "[Unset / Default]".to_string());
    let rec_mem = format!("{} ({} {})", recommendation.allocated_heap_mb, recommendation.xms_flag, recommendation.xmx_flag);
    table.add_row(Row::from(vec![
        Cell::new("Heap Memory"),
        Cell::new(curr_mem),
        Cell::new(rec_mem),
    ]));

    let curr_flags = server
        .jvm_args
        .as_ref()
        .map(|f| f.join(" "))
        .unwrap_or_else(|| "[Default]".to_string());
    let rec_gc = format!("[{}] {}", recommendation.strategy, recommendation.gc_flags.join(" "));
    table.add_row(Row::from(vec![
        Cell::new("GC & JVM Flags"),
        Cell::new(if curr_flags.len() > 30 { format!("{}...", &curr_flags[..30]) } else { curr_flags }),
        Cell::new(if rec_gc.len() > 60 { format!("{}...", &rec_gc[..60]) } else { rec_gc }),
    ]));

    println!("\n{}", table);

    println!("\n{}", "Optimization Details & Rationale:".cyan().bold());
    for exp in &recommendation.rationale {
        println!("  - {}", exp.dimmed());
    }

    let mut combined_flags = vec![recommendation.xms_flag.clone(), recommendation.xmx_flag.clone()];
    combined_flags.extend(recommendation.gc_flags.clone());
    combined_flags.extend(recommendation.general_flags.clone());

    println!("\n{}", "Complete Recommended JVM Arguments:".green().bold());
    println!("  {}", combined_flags.join(" "));

    if apply {
        let server = &mut registry.servers[server_index];
        server.memory = Some(format!("{}M", recommendation.allocated_heap_mb));
        let mut jvm_args = recommendation.gc_flags.clone();
        jvm_args.extend(recommendation.general_flags);
        server.jvm_args = Some(jvm_args);
        registry.save(paths)?;
        println!(
            "\n{} Recommended configuration successfully applied to server '{}'!",
            "[OK]".green().bold(),
            name
        );
    } else {
        println!(
            "\n{} Run '{}' to apply these settings automatically.",
            "[INFO]".yellow().bold(),
            format!("craft optimize {} --profile {} --apply", name, profile_str).cyan()
        );
    }

    Ok(())
}
