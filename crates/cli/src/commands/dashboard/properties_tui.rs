use super::screen::{
    box_divider, box_title, box_top, get_content_width, run_input_prompt, run_menu,
    show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};
use colored::Colorize;
use craft_core::{PropertyCategory, Result, ServerProperties};
use std::path::Path;

pub async fn server_properties_editor(server_path: &Path, server_name: &str) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Properties");

    let props_path = server_path.join("server.properties");
    if !props_path.exists() {
        // Create initial default properties if missing
        let default_props = ServerProperties::parse(
            "# Minecraft Server Properties\nserver-port=25565\nonline-mode=true\ngamemode=survival\ndifficulty=normal\nlevel-name=world\nmotd=A Craft Minecraft Server\n",
        );
        default_props.save(&props_path)?;
    }

    let mut selected_cat = 0;
    let mut flash_msg: Option<String> = None;

    loop {
        let mut props = ServerProperties::load(&props_path)?;
        let width = get_content_width(80);

        let port_str = props.get("server-port").unwrap_or("25565");
        let online_str = props.get("online-mode").unwrap_or("true");
        let gm_str = props.get("gamemode").unwrap_or("survival");
        let diff_str = props.get("difficulty").unwrap_or("normal");
        let world_str = props.get("level-name").unwrap_or("world");

        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n Server:   {:<18} | Port: {:<8} | Online Mode: {}\r\n World:    {:<18} | Mode: {:<8} | Difficulty:  {}\r\n",
            box_top(width).cyan().bold(),
            box_title(&format!("SERVER CONFIG EDITOR: {}", server_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server_name.white().bold(),
            port_str.yellow().bold(),
            if online_str == "true" { "Enabled".green().bold() } else { "Disabled".red().bold() },
            world_str.cyan(),
            gm_str.white(),
            diff_str.yellow(),
        );

        if let Some(msg) = flash_msg.take() {
            header.push_str(&format!(" {}\r\n", msg));
        }

        header.push_str(&box_divider(width).dimmed().to_string());

        let categories = PropertyCategory::all();
        let mut entries = Vec::new();

        for (i, cat) in categories.iter().enumerate() {
            let hotkey = (i + 1).to_string();
            let count = props.list_by_category(*cat).len();
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<26} ({} settings)", cat.name(), count),
            ));
        }

        entries.push(MenuEntry::new("s", "Search All Properties").with_aliases(&["search", "f"]));
        entries.push(MenuEntry::new("a", "Add / Edit Custom Key").with_aliases(&["add", "new"]));
        entries.push(MenuEntry::new("r", "View Raw File Content").with_aliases(&["raw", "view"]));
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected_cat)? {
            Some(idx) if idx < categories.len() => {
                let chosen_cat = categories[idx];
                category_properties_menu(&props_path, server_name, chosen_cat).await?;
            }
            Some(idx) if idx == categories.len() => {
                // Search All Properties
                if let Some(query) = run_input_prompt(
                    "SEARCH PROPERTIES",
                    "Enter search keyword (key, value, or description):",
                    None,
                )? {
                    if !query.trim().is_empty() {
                        search_properties_menu(&props_path, server_name, query.trim()).await?;
                    }
                }
            }
            Some(idx) if idx == categories.len() + 1 => {
                // Add / Edit Custom Key
                if let Some(key) = run_input_prompt(
                    "CUSTOM PROPERTY KEY",
                    "Enter property key name (e.g. view-distance, max-players):",
                    None,
                )? {
                    let key = key.trim().to_string();
                    if !key.is_empty() {
                        let current = props.get(&key).unwrap_or("");
                        if let Some(val) = run_input_prompt(
                            "PROPERTY VALUE",
                            &format!("Enter value for '{}':", key),
                            Some(current),
                        )? {
                            props.set(&key, val.trim());
                            props.save(&props_path)?;
                            flash_msg = Some(
                                format!("[OK] Set '{}' = '{}'", key, val.trim())
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Some(idx) if idx == categories.len() + 2 => {
                // View Raw File Content
                if let Ok(raw) = std::fs::read_to_string(&props_path) {
                    let raw_lines: Vec<String> =
                        raw.lines().take(30).map(|s| s.to_string()).collect();
                    show_modal_message("RAW SERVER.PROPERTIES", &raw_lines, false)?;
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn category_properties_menu(
    props_path: &Path,
    server_name: &str,
    category: PropertyCategory,
) -> Result<()> {
    let _nav = NavGuard::enter(category.name());
    let mut selected = 0;
    let mut flash_msg: Option<String> = None;

    loop {
        let mut props = ServerProperties::load(props_path)?;
        let items = props.list_by_category(category);

        let width = get_content_width(80);
        let mut header = format!(
            "{}\r\n{}\r\n{}\r\n Category: {} | Total Settings: {}\r\n Select any setting to toggle boolean or edit value:\r\n",
            box_top(width).cyan().bold(),
            box_title(&format!("{} - {}", category.name(), server_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            category.name().white().bold(),
            items.len(),
        );

        if let Some(msg) = flash_msg.take() {
            header.push_str(&format!(" {}\r\n", msg));
        }

        header.push_str(&box_divider(width).dimmed().to_string());

        if items.is_empty() {
            let entries = vec![
                MenuEntry::new("1", "Add New Property In This Category"),
                MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
            ];
            let mut empty_sel = 0;
            if let Some(0) = run_menu(&header, &entries, &mut empty_sel)? {
                if let Some(key) = run_input_prompt("NEW PROPERTY KEY", "Enter key:", None)? {
                    if let Some(val) = run_input_prompt("PROPERTY VALUE", "Enter value:", None)? {
                        props.set(key.trim(), val.trim());
                        props.save(props_path)?;
                        flash_msg = Some(
                            format!("[OK] Added '{}'", key.trim())
                                .green()
                                .bold()
                                .to_string(),
                        );
                    }
                }
                continue;
            }
            return Ok(());
        }

        let mut entries = Vec::new();
        for (i, (k, v)) in items.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else if i < 35 {
                ((b'a' + (i - 9) as u8) as char).to_string()
            } else {
                format!("{}", i + 1)
            };

            let val_display = if *v == "true" {
                "[ON]".green().bold().to_string()
            } else if *v == "false" {
                "[OFF]".red().bold().to_string()
            } else {
                format!("[{}]", v).yellow().to_string()
            };

            let desc = ServerProperties::property_description(k);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<28} {:<10} - {}", k, val_display, desc.dimmed()),
            ));
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < items.len() => {
                let (key, val) = items[idx];
                let key_str = key.to_string();
                let val_str = val.to_string();

                if val_str == "true" || val_str == "false" {
                    // Boolean instant toggle
                    let new_val = if val_str == "true" { "false" } else { "true" };
                    props.set(&key_str, new_val);
                    props.save(props_path)?;
                    flash_msg = Some(
                        format!("[OK] Toggled '{}' -> {}", key_str, new_val)
                            .green()
                            .bold()
                            .to_string(),
                    );
                } else if key_str == "gamemode" {
                    let modes = vec![
                        MenuEntry::new("1", "survival"),
                        MenuEntry::new("2", "creative"),
                        MenuEntry::new("3", "adventure"),
                        MenuEntry::new("4", "spectator"),
                        MenuEntry::new("0", "Cancel"),
                    ];
                    let mut gm_sel = 0;
                    if let Some(m_idx) = run_menu("Select Default Game Mode:", &modes, &mut gm_sel)?
                    {
                        if m_idx < 4 {
                            let chosen = match m_idx {
                                0 => "survival",
                                1 => "creative",
                                2 => "adventure",
                                _ => "spectator",
                            };
                            props.set("gamemode", chosen);
                            props.save(props_path)?;
                            flash_msg = Some(
                                format!("[OK] Set gamemode = {}", chosen)
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                } else if key_str == "difficulty" {
                    let diffs = vec![
                        MenuEntry::new("1", "peaceful"),
                        MenuEntry::new("2", "easy"),
                        MenuEntry::new("3", "normal"),
                        MenuEntry::new("4", "hard"),
                        MenuEntry::new("0", "Cancel"),
                    ];
                    let mut d_sel = 0;
                    if let Some(d_idx) = run_menu("Select Server Difficulty:", &diffs, &mut d_sel)?
                    {
                        if d_idx < 4 {
                            let chosen = match d_idx {
                                0 => "peaceful",
                                1 => "easy",
                                2 => "normal",
                                _ => "hard",
                            };
                            props.set("difficulty", chosen);
                            props.save(props_path)?;
                            flash_msg = Some(
                                format!("[OK] Set difficulty = {}", chosen)
                                    .green()
                                    .bold()
                                    .to_string(),
                            );
                        }
                    }
                } else {
                    // Text / Number input
                    let desc = ServerProperties::property_description(&key_str);
                    if let Some(new_val) = run_input_prompt(
                        &format!("EDIT {}", key_str.to_uppercase()),
                        desc,
                        Some(&val_str),
                    )? {
                        props.set(&key_str, new_val.trim());
                        props.save(props_path)?;
                        flash_msg = Some(
                            format!("[OK] Updated '{}' = '{}'", key_str, new_val.trim())
                                .green()
                                .bold()
                                .to_string(),
                        );
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

async fn search_properties_menu(props_path: &Path, server_name: &str, query: &str) -> Result<()> {
    let _nav = NavGuard::enter(format!("Search: {}", query));
    let mut selected = 0;

    loop {
        let mut props = ServerProperties::load(props_path)?;
        let q = query.to_lowercase();

        let matches: Vec<(&str, &str)> = props
            .list_entries()
            .into_iter()
            .filter(|(k, v)| {
                k.to_lowercase().contains(&q)
                    || v.to_lowercase().contains(&q)
                    || ServerProperties::property_description(k)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect();

        if matches.is_empty() {
            show_modal_message(
                "NO MATCHES FOUND",
                &[format!("No properties found matching query '{}'.", query)],
                false,
            )?;
            return Ok(());
        }

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Matches for '{}' in {}:\r\n Select any setting to toggle or edit:\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("SEARCH: {}", query), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            query.cyan(),
            server_name,
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (i, (k, v)) in matches.iter().enumerate() {
            let hotkey = if i < 9 {
                (i + 1).to_string()
            } else {
                ((b'a' + (i - 9) as u8) as char).to_string()
            };
            let val_display = if *v == "true" {
                "[ON]".green().bold().to_string()
            } else if *v == "false" {
                "[OFF]".red().bold().to_string()
            } else {
                format!("[{}]", v).yellow().to_string()
            };
            let desc = ServerProperties::property_description(k);
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<25} {:<10} - {}", k, val_display, desc.dimmed()),
            ));
        }
        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < matches.len() => {
                let (key, val) = matches[idx];
                let key_str = key.to_string();
                let val_str = val.to_string();

                if val_str == "true" || val_str == "false" {
                    let new_val = if val_str == "true" { "false" } else { "true" };
                    props.set(&key_str, new_val);
                    props.save(props_path)?;
                } else {
                    let desc = ServerProperties::property_description(&key_str);
                    if let Some(new_val) = run_input_prompt(
                        &format!("EDIT {}", key_str.to_uppercase()),
                        desc,
                        Some(&val_str),
                    )? {
                        props.set(&key_str, new_val.trim());
                        props.save(props_path)?;
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}
