use std::fs::{self, File};
use std::path::{Path, PathBuf};
use colored::Colorize;
use craft_core::{
    get_dimension_worlds, set_default_world, set_end_world, set_nether_world, CraftError,
    NbtFile, NbtTag, Result, ServerConfig,
};
use craft_plugins::world::{
    inspect_world_metadata, install_world_from_url, install_world_from_zip,
    list_installed_worlds, list_world_advancements, list_world_data_storages,
    list_world_player_data, search_curated_maps, get_curated_maps, InstalledWorldItem,
};
use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};

pub async fn manage_installed_worlds_menu(server: &ServerConfig) -> Result<()> {
    if server.game != "minecraft" {
        return manage_non_minecraft_saves_menu(server).await;
    }

    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Manage Worlds");
    let mut selected = 0;

    loop {
        let worlds = list_installed_worlds(&server.path);
        let (default_world, nether_world, end_world) = get_dimension_worlds(&server.path);

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:   {:<18} | Overworld: {}\r\n Nether:   {:<18} | The End:   {}\r\n Select a world to inspect or manage dimensions:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("WORLD MANAGER: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            default_world.green().bold(),
            nether_world.as_deref().unwrap_or("None (Default)").yellow(),
            end_world.as_deref().unwrap_or("None (Default)").yellow(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, w) in worlds.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else if i < 35 {
                ((b'a' + (i - 9) as u8) as char).to_string()
            } else {
                format!("{}", i + 1)
            };

            let mut badges = Vec::new();
            if w.is_default {
                badges.push("[OVERWORLD]".green().bold().to_string());
            }
            if w.is_nether {
                badges.push("[NETHER]".red().bold().to_string());
            }
            if w.is_end {
                badges.push("[THE END]".magenta().bold().to_string());
            }
            let badge_str = if badges.is_empty() {
                "[AVAILABLE]".dimmed().to_string()
            } else {
                badges.join(" ")
            };

            let mb = (w.size_bytes as f64) / (1024.0 * 1024.0);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<22} {:>7.1} MB  {}", w.name, mb, badge_str),
            ));
        }

        entries.push(MenuEntry::new("i", "Import World (ZIP or URL)").with_aliases(&["import"]));
        entries.push(MenuEntry::new("c", "Curated Community Maps Catalog").with_aliases(&["maps"]));
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < worlds.len() => {
                let chosen = &worlds[idx];
                world_detail_menu(server, chosen).await?;
            }
            Some(idx) if idx == worlds.len() => {
                // Import World
                let target = match run_input_prompt(
                    "IMPORT WORLD",
                    "Enter local .zip file path or remote HTTP/HTTPS download link:",
                    None,
                )? {
                    Some(t) if !t.trim().is_empty() => t.trim().to_string(),
                    _ => continue,
                };

                let custom_name = match run_input_prompt(
                    "WORLD NAME",
                    "Enter folder name for imported world (or leave blank for automatic):",
                    None,
                )? {
                    Some(n) if !n.trim().is_empty() => Some(n.trim().to_string()),
                    _ => None,
                };

                let _ = print_in_place_status("IMPORTING WORLD", &[format!("Processing '{}'...", target)]);

                let res = if target.starts_with("http://") || target.starts_with("https://") {
                    install_world_from_url(&server.path, &target, custom_name.as_deref()).await
                } else {
                    let zip_path = PathBuf::from(&target);
                    install_world_from_zip(&server.path, &zip_path, custom_name.as_deref())
                };

                match res {
                    Ok((dest, installed_name)) => {
                        show_modal_message(
                            "WORLD IMPORTED",
                            &[
                                format!("[OK] Successfully imported world '{}'!", installed_name).green().bold().to_string(),
                                format!("Path: {}", dest.display()),
                            ],
                            false,
                        )?;
                        prompt_set_as_default_world(&server.path, &installed_name)?;
                    }
                    Err(e) => {
                        show_modal_message("IMPORT FAILED", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(idx) if idx == worlds.len() + 1 => {
                // Curated Community Maps
                curated_maps_menu(&server.path).await?;
            }
            _ => return Ok(()),
        }
    }
}

async fn world_detail_menu(server: &ServerConfig, world: &InstalledWorldItem) -> Result<()> {
    let _nav = NavGuard::enter(&world.name);
    let mut selected = 0;

    loop {
        let (default_world, nether_world, end_world) = get_dimension_worlds(&server.path);
        let is_def = world.name.eq_ignore_ascii_case(&default_world);
        let is_neth = nether_world.as_deref().map(|n| n.eq_ignore_ascii_case(&world.name)).unwrap_or(false);
        let is_the_end = end_world.as_deref().map(|e| e.eq_ignore_ascii_case(&world.name)).unwrap_or(false);

        let width = get_content_width(80);
        let mb = (world.size_bytes as f64) / (1024.0 * 1024.0);

        let mut role_tags = Vec::new();
        if is_def { role_tags.push("[OVERWORLD]".green().bold().to_string()); }
        if is_neth { role_tags.push("[NETHER]".red().bold().to_string()); }
        if is_the_end { role_tags.push("[THE END]".magenta().bold().to_string()); }
        if role_tags.is_empty() { role_tags.push("[AVAILABLE]".dimmed().to_string()); }

        let header = format!(
            "{}\r\n{}\r\n{}\r\n World:     {:<18} | Size: {:.1} MB\r\n Roles:     {}\r\n Location:  {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("WORLD DETAILS: {}", world.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            world.name.white().bold(),
            mb,
            role_tags.join(" "),
            world.path.display(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "World Overview & Metadata (level.dat)"),
            MenuEntry::new("2", "Dimension Roles (Set Default, Nether, End)"),
            MenuEntry::new("3", "Player Data Inspector"),
            MenuEntry::new("4", "Player Advancements Viewer"),
            MenuEntry::new("5", "Data Storages (.dat files in data/)"),
            MenuEntry::new("6", "NBT Tree Explorer & Editor"),
            MenuEntry::new("7", "Export World to ZIP Archive"),
            MenuEntry::new("8", "Delete World"),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // World Overview
                match inspect_world_metadata(&world.path) {
                    Ok(meta) => {
                        let lines = vec![
                            format!("Level Name:     {}", meta.level_name.white().bold()),
                            format!("Game Mode:      {}", meta.game_type.green()),
                            format!("Difficulty:     {}", meta.difficulty.yellow()),
                            format!("Hardcore:       {}", if meta.hardcore { "Yes".red().bold() } else { "No".dimmed() }),
                            format!("Spawn Location: X={}, Y={}, Z={}", meta.spawn_x, meta.spawn_y, meta.spawn_z),
                            format!("World Seed:     {}", meta.seed.map(|s| s.to_string()).unwrap_or_else(|| "Unknown".to_string()).cyan()),
                            format!("Version:        {}", meta.version_name.as_deref().unwrap_or("Unknown").dimmed()),
                            format!("World Age:      {} ticks (Day: {})", meta.time, meta.day_time / 24000),
                        ];
                        show_modal_message(&format!("METADATA: {}", world.name), &lines, false)?;
                    }
                    Err(e) => {
                        show_modal_message("ERROR", &[format!("Failed to read level.dat: {}", e)], true)?;
                    }
                }
            }
            Some(1) => {
                // Dimension Roles
                dimension_roles_menu(server, &world.name).await?;
            }
            Some(2) => {
                // Player Data Inspector
                player_data_inspector_menu(server, &world.path).await?;
            }
            Some(3) => {
                // Advancements Viewer
                advancements_viewer_menu(server, &world.path).await?;
            }
            Some(4) => {
                // Data Storages
                data_storages_menu(&world.path).await?;
            }
            Some(5) => {
                // NBT Tree Explorer & Editor
                let level_dat = world.path.join("level.dat");
                if level_dat.exists() {
                    nbt_explorer_menu(&level_dat).await?;
                } else {
                    show_modal_message("ERROR", &[format!("level.dat not found in '{}'", world.path.display())], true)?;
                }
            }
            Some(6) => {
                // Export World to ZIP
                let backup_dir = server.path.join("backups");
                let _ = fs::create_dir_all(&backup_dir);
                let zip_name = format!("{}_{}_export.zip", server.name, world.name);
                let zip_dest = backup_dir.join(&zip_name);

                let _ = print_in_place_status("EXPORTING WORLD", &[format!("Compressing '{}' into '{}'...", world.name, zip_dest.display())]);
                match compress_folder_to_zip(&world.path, &zip_dest) {
                    Ok(_) => {
                        show_modal_message(
                            "EXPORT SUCCESSFUL",
                            &[
                                "[OK] Exported world archive to:".green().bold().to_string(),
                                zip_dest.display().to_string(),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message("EXPORT FAILED", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(7) => {
                // Delete World
                let confirm = run_menu(
                    &format!(" CONFIRM WORLD DELETION\r\n Are you sure you want to permanently delete world '{}'?", world.name),
                    &[
                        MenuEntry::new("1", "Cancel").with_aliases(&["n", "no"]),
                        MenuEntry::new("2", "Delete World Permanently").with_aliases(&["y", "yes"]),
                    ],
                    &mut 0,
                )?;
                if confirm == Some(1) {
                    let _ = fs::remove_dir_all(&world.path);
                    show_modal_message("WORLD DELETED", &[format!("[OK] Removed world '{}'.", world.name)], false)?;
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn dimension_roles_menu(server: &ServerConfig, world_name: &str) -> Result<()> {
    let mut selected = 0;
    let width = get_content_width(80);

    let header = format!(
        "{}\r\n{}\r\n{}\r\n Configure which Minecraft dimension role '{}' serves:\r\n{}",
        box_top(width).cyan().bold(),
        box_title(&format!("DIMENSION ROLES: {}", world_name), width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        world_name.white().bold(),
        box_divider(width).dimmed(),
    );

    let entries = vec![
        MenuEntry::new("1", "Set as Default Overworld (level-name in server.properties)"),
        MenuEntry::new("2", "Set as Active Nether Dimension World"),
        MenuEntry::new("3", "Set as Active The End Dimension World"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    match run_menu(&header, &entries, &mut selected)? {
        Some(0) => {
            set_default_world(&server.path, world_name)?;
            show_modal_message("OVERWORLD UPDATED", &[format!("[OK] Set '{}' as default Overworld!", world_name)], false)?;
        }
        Some(1) => {
            set_nether_world(&server.path, world_name)?;
            show_modal_message("NETHER UPDATED", &[format!("[OK] Set '{}' as active Nether world!", world_name)], false)?;
        }
        Some(2) => {
            set_end_world(&server.path, world_name)?;
            show_modal_message("THE END UPDATED", &[format!("[OK] Set '{}' as active The End world!", world_name)], false)?;
        }
        _ => {}
    }
    Ok(())
}

async fn player_data_inspector_menu(server: &ServerConfig, world_path: &Path) -> Result<()> {
    let players = list_world_player_data(world_path, &server.path)?;
    if players.is_empty() {
        show_modal_message(
            "NO PLAYER DATA",
            &[format!("No playerdata (*.dat) files found in '{}/playerdata'.", world_path.display())],
            false,
        )?;
        return Ok(());
    }

    let width = get_content_width(80);
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Total Players Recorded: {}\r\n Select a player to view detailed inventory & attributes:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("PLAYER DATA INSPECTOR", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            players.len(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, p) in players.iter().enumerate() {
            let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
            let coords = format!("({:.0}, {:.0}, {:.0})", p.pos.0, p.pos.1, p.pos.2);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} HP: {:<4.0} XP: {:<3} Pos: {:<16} [{}]", p.name, p.health, p.xp_level, coords, p.gamemode),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < players.len() => {
                let p = &players[idx];
                let lines = vec![
                    format!("Player Name:    {}", p.name.white().bold()),
                    format!("Player UUID:    {}", p.uuid.dimmed()),
                    format!("Coordinates:    X={:.2}, Y={:.2}, Z={:.2}", p.pos.0, p.pos.1, p.pos.2),
                    format!("Dimension:      {}", p.dimension.cyan()),
                    format!("Health:         {:.1} / 20.0", p.health),
                    format!("Food Level:     {} / 20", p.food_level),
                    format!("Experience Lvl: {}", p.xp_level),
                    format!("Game Mode:      {}", p.gamemode),
                    format!("Inventory Items:{} slots occupied", p.inventory_count),
                ];
                show_modal_message(&format!("PLAYER: {}", p.name), &lines, false)?;
            }
            _ => return Ok(()),
        }
    }
}

async fn advancements_viewer_menu(server: &ServerConfig, world_path: &Path) -> Result<()> {
    let players = list_world_player_data(world_path, &server.path)?;
    if players.is_empty() {
        show_modal_message("NO PLAYERS", &["No player records found to check advancements.".to_string()], false)?;
        return Ok(());
    }

    let mut sel = 0;
    let width = get_content_width(80);
    let p_header = format!(
        "{}\r\n{}\r\n{}\r\n Select a player to view unlocked advancements:\r\n{}",
        box_top(width).cyan().bold(),
        box_title("SELECT PLAYER FOR ADVANCEMENTS", width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        box_divider(width).dimmed(),
    );

    let mut p_entries = Vec::new();
    for (i, p) in players.iter().enumerate() {
        p_entries.push(MenuEntry::new((i + 1).to_string(), p.name.clone()));
    }
    p_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

    if let Some(idx) = run_menu(&p_header, &p_entries, &mut sel)? {
        if idx < players.len() {
            let p = &players[idx];
            let advs = list_world_advancements(world_path, &p.uuid)?;
            if advs.is_empty() {
                show_modal_message("ADVANCEMENTS", &[format!("No advancements recorded yet for '{}'.", p.name)], false)?;
                return Ok(());
            }

            let mut lines = Vec::new();
            for a in advs.iter().take(25) {
                let status = if a.completed { "[DONE]".green().bold() } else { "[IN PROGRESS]".yellow() };
                let short_id = a.id.strip_prefix("minecraft:").unwrap_or(&a.id);
                lines.push(format!("{:<30} {}", short_id, status));
            }
            show_modal_message(&format!("ADVANCEMENTS: {}", p.name), &lines, false)?;
        }
    }
    Ok(())
}

async fn data_storages_menu(world_path: &Path) -> Result<()> {
    let files = list_world_data_storages(world_path);
    if files.is_empty() {
        show_modal_message("NO DATA STORAGES", &["No .dat files found in world data/ directory.".to_string()], false)?;
        return Ok(());
    }

    let width = get_content_width(80);
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Persistent game data storages in data/:\r\n Select any storage file to inspect its NBT tree:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("DATA STORAGES", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
            let kb = f.size_bytes as f64 / 1024.0;
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} ({:>5.1} KB) - {}", f.filename, kb, f.description.dimmed()),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < files.len() => {
                let f = &files[idx];
                let path = world_path.join("data").join(&f.filename);
                nbt_explorer_menu(&path).await?;
            }
            _ => return Ok(()),
        }
    }
}

pub async fn nbt_explorer_menu(file_path: &Path) -> Result<()> {
    let _nav = NavGuard::enter(format!("NBT: {}", file_path.file_name().unwrap_or_default().to_string_lossy()));
    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let mut nbt_file = match NbtFile::read(file_path) {
        Ok(f) => f,
        Err(e) => {
            show_modal_message("NBT READ ERROR", &[format!("Failed to parse NBT file: {}", e)], true)?;
            return Ok(());
        }
    };

    let mut current_path: Vec<String> = Vec::new();
    let mut selected = 0;
    let mut flash_msg: Option<String> = None;

    loop {
        let width = get_content_width(80);
        let path_str = if current_path.is_empty() {
            "ROOT".to_string()
        } else {
            current_path.join(" / ")
        };

        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n File: {} | Compressed: {}\r\n Node: {}\r\n Select an entry to expand compound or edit scalar value:\r\n",
            box_top(width).cyan().bold(),
            box_title(&format!("NBT EXPLORER: {}", file_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            file_name.white().bold(),
            if nbt_file.is_compressed { "GZIP".green() } else { "No".dimmed() },
            path_str.yellow().bold(),
        );

        if let Some(msg) = flash_msg.take() {
            header.push_str(&format!(" {}\r\n", msg));
        }

        header.push_str(&box_divider(width).dimmed().to_string());

        // Locate current compound
        let mut target_ref = &nbt_file.root;
        let mut valid_path = true;
        for segment in &current_path {
            if let Some(next) = target_ref.get(segment) {
                target_ref = next;
            } else {
                valid_path = false;
                break;
            }
        }

        if !valid_path {
            current_path.clear();
            continue;
        }

        match target_ref {
            NbtTag::Compound(map) => {
                let keys: Vec<String> = map.keys().cloned().collect();
                let mut entries = Vec::new();

                for (i, k) in keys.iter().enumerate() {
                    let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
                    let val = &map[k];
                    let brief = val.format_value_brief();
                    let type_label = format!("[{}]", val.type_name()).cyan();
                    entries.push(MenuEntry::new(
                        hotkey,
                        format!("{:<24} {:<12} {}", k, type_label, brief.dimmed()),
                    ));
                }

                if !current_path.is_empty() {
                    entries.push(MenuEntry::new("u", ".. Up One Level").with_aliases(&["up"]));
                }
                entries.push(MenuEntry::new("0", "Exit NBT Editor").with_aliases(&["b", "q"]));

                match run_menu(&header, &entries, &mut selected)? {
                    Some(idx) if idx < keys.len() => {
                        let chosen_key = &keys[idx];
                        let child = &map[chosen_key];
                        match child {
                            NbtTag::Compound(_) => {
                                current_path.push(chosen_key.clone());
                            }
                            NbtTag::String(s) => {
                                if let Some(new_val) = run_input_prompt("EDIT STRING TAG", &format!("{}:", chosen_key), Some(s))? {
                                    backup_nbt_file(file_path)?;
                                    mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::String(new_val.clone()));
                                    nbt_file.write(file_path)?;
                                    flash_msg = Some(format!("[OK] Set '{}' = '{}' (Backup created)", chosen_key, new_val).green().bold().to_string());
                                }
                            }
                            NbtTag::Int(i) => {
                                if let Some(new_val) = run_input_prompt("EDIT INT TAG", &format!("{}:", chosen_key), Some(&i.to_string()))? {
                                    if let Ok(num) = new_val.trim().parse::<i32>() {
                                        backup_nbt_file(file_path)?;
                                        mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::Int(num));
                                        nbt_file.write(file_path)?;
                                        flash_msg = Some(format!("[OK] Set '{}' = {} (Backup created)", chosen_key, num).green().bold().to_string());
                                    }
                                }
                            }
                            NbtTag::Byte(b) => {
                                let new_b = if *b == 0 { 1 } else { 0 };
                                backup_nbt_file(file_path)?;
                                mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::Byte(new_b));
                                nbt_file.write(file_path)?;
                                flash_msg = Some(format!("[OK] Toggled '{}' = {}b", chosen_key, new_b).green().bold().to_string());
                            }
                            NbtTag::Long(l) => {
                                if let Some(new_val) = run_input_prompt("EDIT LONG TAG", &format!("{}:", chosen_key), Some(&l.to_string()))? {
                                    if let Ok(num) = new_val.trim().parse::<i64>() {
                                        backup_nbt_file(file_path)?;
                                        mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::Long(num));
                                        nbt_file.write(file_path)?;
                                        flash_msg = Some(format!("[OK] Set '{}' = {}L", chosen_key, num).green().bold().to_string());
                                    }
                                }
                            }
                            NbtTag::Float(f) => {
                                if let Some(new_val) = run_input_prompt("EDIT FLOAT TAG", &format!("{}:", chosen_key), Some(&f.to_string()))? {
                                    if let Ok(num) = new_val.trim().parse::<f32>() {
                                        backup_nbt_file(file_path)?;
                                        mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::Float(num));
                                        nbt_file.write(file_path)?;
                                        flash_msg = Some(format!("[OK] Set '{}' = {}f", chosen_key, num).green().bold().to_string());
                                    }
                                }
                            }
                            NbtTag::Double(d) => {
                                if let Some(new_val) = run_input_prompt("EDIT DOUBLE TAG", &format!("{}:", chosen_key), Some(&d.to_string()))? {
                                    if let Ok(num) = new_val.trim().parse::<f64>() {
                                        backup_nbt_file(file_path)?;
                                        mutate_nested_tag(&mut nbt_file.root, &current_path, chosen_key, NbtTag::Double(num));
                                        nbt_file.write(file_path)?;
                                        flash_msg = Some(format!("[OK] Set '{}' = {}d", chosen_key, num).green().bold().to_string());
                                    }
                                }
                            }
                            _ => {
                                show_modal_message("TAG VALUE", &[format!("{}: {}", chosen_key, child.format_value_brief())], false)?;
                            }
                        }
                    }
                    Some(idx) if idx == keys.len() && !current_path.is_empty() => {
                        current_path.pop();
                    }
                    _ => return Ok(()),
                }
            }
            _ => {
                current_path.pop();
            }
        }
    }
}

fn backup_nbt_file(path: &Path) -> Result<()> {
    let bak_path = path.with_extension("dat.bak");
    if !bak_path.exists() {
        let _ = fs::copy(path, &bak_path);
    }
    Ok(())
}

fn mutate_nested_tag(root: &mut NbtTag, path: &[String], target_key: &str, new_tag: NbtTag) {
    let mut current = root;
    for segment in path {
        if let NbtTag::Compound(ref mut map) = current {
            if let Some(next) = map.get_mut(segment) {
                current = next;
            } else {
                return;
            }
        } else {
            return;
        }
    }

    if let NbtTag::Compound(ref mut map) = current {
        map.insert(target_key.to_string(), new_tag);
    }
}

fn compress_folder_to_zip(src_dir: &Path, dest_zip: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let file = File::create(dest_zip).map_err(CraftError::Io)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let prefix = src_dir.parent().unwrap_or(src_dir);

    for entry in walkdir(src_dir).map_err(CraftError::Io)? {
        let path = entry.as_path();
        let rel_path = path.strip_prefix(prefix).map_err(|e| CraftError::Other(format!("{}", e)))?;
        let name = rel_path.to_string_lossy().replace('\\', "/");

        if path.is_file() {
            zip.start_file(name, options).map_err(|e| CraftError::Other(format!("{}", e)))?;
            let mut f = File::open(path).map_err(CraftError::Io)?;
            std::io::copy(&mut f, &mut zip).map_err(CraftError::Io)?;
        } else if !name.is_empty() {
            zip.add_directory(format!("{}/", name), options).map_err(|e| CraftError::Other(format!("{}", e)))?;
        }
    }

    zip.finish().map_err(|e| CraftError::Other(format!("{}", e)))?;
    Ok(())
}

fn walkdir(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                result.push(path.clone());
                result.extend(walkdir(&path)?);
            } else {
                result.push(path);
            }
        }
    }
    Ok(result)
}

fn prompt_set_as_default_world(server_path: &Path, world_name: &str) -> Result<()> {
    let entries = vec![
        MenuEntry::new("1", "Yes, set as active default Overworld"),
        MenuEntry::new("2", "No, keep current default"),
    ];
    let mut sel = 0;
    if let Some(0) = run_menu(
        &format!(" Would you like to set '{}' as the active default Overworld in server.properties?", world_name),
        &entries,
        &mut sel,
    )? {
        set_default_world(server_path, world_name)?;
    }
    Ok(())
}

async fn curated_maps_menu(server_path: &Path) -> Result<()> {
    let mut selected = 0;

    loop {
        let maps = get_curated_maps();
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a popular community map to install or search by keyword:\r\n{}",
            box_top(width).cyan().bold(),
            box_title("CURATED COMMUNITY MAPS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, m) in maps.iter().enumerate() {
            let hotkey = (i + 1).to_string();
            let desc = craft_core::truncate_ellipsis(m.description, 40);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} [{}] - {}", m.name, m.category, desc),
            ));
        }
        entries.push(MenuEntry::new("s", "Search Maps").with_aliases(&["search"]));
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < maps.len() => {
                let chosen = &maps[idx];
                let _ = print_in_place_status(
                    "DOWNLOADING MAP",
                    &[
                        format!("Downloading '{}'...", chosen.name),
                        format!("Source URL: {}", chosen.download_url),
                    ],
                );
                match install_world_from_url(server_path, chosen.download_url, Some(chosen.default_folder)).await {
                    Ok((dest, installed_name)) => {
                        show_modal_message(
                            "MAP INSTALLED",
                            &[
                                format!("[OK] Successfully installed map '{}'!", chosen.name).green().bold().to_string(),
                                format!("Path: {}", dest.display()),
                            ],
                            false,
                        )?;
                        prompt_set_as_default_world(server_path, &installed_name)?;
                        return Ok(());
                    }
                    Err(e) => {
                        show_modal_message("INSTALLATION FAILED", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(idx) if idx == maps.len() => {
                if let Some(query) = run_input_prompt(
                    "SEARCH MAPS",
                    "Enter map search keyword (e.g. skyblock, parkour, dropper, adventure):",
                    None,
                )? {
                    let results = search_curated_maps(query.trim());
                    if results.is_empty() {
                        show_modal_message("NO MAPS FOUND", &[format!("No maps found matching '{}'.", query)], false)?;
                    } else {
                        let mut s_entries = Vec::new();
                        for (i, m) in results.iter().enumerate() {
                            s_entries.push(MenuEntry::new((i + 1).to_string(), format!("{:<20} [{}]", m.name, m.category)));
                        }
                        s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));
                        let mut s_sel = 0;
                        if let Some(s_idx) = run_menu(&format!(" Results for '{}':", query), &s_entries, &mut s_sel)? {
                            if s_idx < results.len() {
                                let chosen = &results[s_idx];
                                match install_world_from_url(server_path, chosen.download_url, Some(chosen.default_folder)).await {
                                    Ok((_dest, installed_name)) => {
                                        show_modal_message("MAP INSTALLED", &[format!("[OK] Installed '{}'", chosen.name)], false)?;
                                        prompt_set_as_default_world(server_path, &installed_name)?;
                                        return Ok(());
                                    }
                                    Err(e) => {
                                        show_modal_message("ERROR", &[format!("{}", e)], true)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn manage_non_minecraft_saves_menu(server: &ServerConfig) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Manage Saves");
    let mut selected = 0;

    loop {
        let saves = craft_plugins::list_saves_for_server(&server.path, &server.game)?;
        let width = get_content_width(80);
        let game_def = server.game_definition();

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server:   {:<18} | Game: {}\r\n Format:   {}\r\n Select a save item to view details or create backups:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("SAVE MANAGER: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            game_def.name.magenta().bold(),
            game_def.save_directory.as_deref().unwrap_or("saves/").cyan(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, s) in saves.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else {
                ((b'a' + (i - 9) as u8) as char).to_string()
            };
            let mb = (s.size_bytes as f64) / (1024.0 * 1024.0);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<24} {:>7.2} MB  [{}]", s.name, mb, s.format_desc),
            ));
        }

        if saves.is_empty() {
            entries.push(MenuEntry::new("!", "No saves detected yet (run server to generate initial world)"));
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < saves.len() => {
                let chosen = &saves[idx];
                let mb = (chosen.size_bytes as f64) / (1024.0 * 1024.0);
                show_modal_message(
                    "SAVE DETAILS",
                    &[
                        format!("Name:        {}", chosen.name),
                        format!("Game:        {}", chosen.game),
                        format!("Format:      {}", chosen.format_desc),
                        format!("Size:        {:.2} MB ({} bytes)", mb, chosen.size_bytes),
                        format!("Path:        {}", chosen.path.display()),
                    ],
                    false,
                )?;
            }
            _ => return Ok(()),
        }
    }
}
