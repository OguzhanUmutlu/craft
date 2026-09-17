use super::screen::{
    box_divider, box_title, box_top, get_content_width, run_input_prompt, run_menu,
    show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};
use colored::Colorize;
use craft_core::{PropertyCategory, Result, ServerProperties};
use modalx::modals::{FormField, FormModal};
use std::path::Path;

pub async fn server_properties_editor(server_path: &Path, server_name: &str) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Properties");

    if !server_path.exists() {
        show_modal_message(
            "SERVER NOT FOUND",
            &[format!(
                "Server directory '{}' was deleted or moved.",
                server_path.display()
            )],
            true,
        )?;
        return Ok(());
    }

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
        if !server_path.exists() {
            show_modal_message(
                "SERVER NOT FOUND",
                &[format!(
                    "Server directory '{}' was deleted or moved.",
                    server_path.display()
                )],
                true,
            )?;
            return Ok(());
        }
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

    if !props_path.exists() {
        show_modal_message(
            "SERVER NOT FOUND",
            &[format!(
                "Properties file for '{}' was deleted or moved.",
                server_name
            )],
            true,
        )?;
        return Ok(());
    }

    let mut props = ServerProperties::load(props_path)?;
    let items: Vec<(String, String)> = props
        .list_by_category(category)
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if items.is_empty() {
        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Category: {} | Total Settings: 0\r\n No default settings found in this category.\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("{} - {}", category.name(), server_name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            category.name().white().bold(),
            box_divider(width).dimmed(),
        );

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
                }
            }
        }
        return Ok(());
    }

    let mut form = FormModal::new(format!("{} - {}", category.name(), server_name))
        .with_confirm_on_cancel(true)
        .with_header_row(format!(
            "Category: {} | Total Settings: {}",
            category.name().white().bold(),
            items.len()
        ))
        .with_header_row(
            "Edit values, press Space to toggle checkboxes, Ctrl+S or Submit to save:",
        );

    for (k, v) in &items {
        let desc = ServerProperties::property_description(k);
        let label = if desc.is_empty() {
            k.to_string()
        } else {
            format!("{} ({})", k, desc)
        };

        if v == "true" || v == "false" || props.get_bool(k).is_some() {
            form = form.with_field(FormField::checkbox_with_default(
                k.clone(),
                label,
                v == "true",
            ));
        } else if props.get_i32(k).is_some() || v.parse::<i64>().is_ok() {
            form = form.with_field(FormField::integer(k.clone(), label).with_default(v.clone()));
        } else {
            form = form.with_field(FormField::string(k.clone(), label).with_default(v.clone()));
        }
    }

    if let Some(result) = form.run()? {
        if !props_path.exists() {
            show_modal_message(
                "SERVER NOT FOUND",
                &["Server was deleted before settings could be saved.".to_string()],
                true,
            )?;
            return Ok(());
        }
        for (k, _) in &items {
            if let Some(new_val) = result.get(k) {
                props.set(k, new_val);
            }
        }
        props.save(props_path)?;
        show_modal_message(
            "SETTINGS SAVED",
            &[format!(
                "Successfully saved {} settings for '{}'.",
                category.name(),
                server_name
            )],
            false,
        )?;
    }

    Ok(())
}

async fn search_properties_menu(props_path: &Path, server_name: &str, query: &str) -> Result<()> {
    let _nav = NavGuard::enter(format!("Search: {}", query));

    if !props_path.exists() {
        show_modal_message(
            "SERVER NOT FOUND",
            &[format!(
                "Properties file for '{}' was deleted or moved.",
                server_name
            )],
            true,
        )?;
        return Ok(());
    }

    let mut props = ServerProperties::load(props_path)?;
    let q = query.to_lowercase();

    let matches: Vec<(String, String)> = props
        .list_entries()
        .into_iter()
        .filter(|(k, v)| {
            k.to_lowercase().contains(&q)
                || v.to_lowercase().contains(&q)
                || ServerProperties::property_description(k)
                    .to_lowercase()
                    .contains(&q)
        })
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if matches.is_empty() {
        show_modal_message(
            "NO MATCHES FOUND",
            &[format!("No properties found matching query '{}'.", query)],
            false,
        )?;
        return Ok(());
    }

    let mut form = FormModal::new(format!("Search: '{}' - {}", query, server_name))
        .with_confirm_on_cancel(true)
        .with_header_row(format!(
            "Query: '{}' | Matching Settings: {}",
            query.cyan().bold(),
            matches.len()
        ))
        .with_header_row(
            "Edit values, press Space to toggle checkboxes, Ctrl+S or Submit to save:",
        );

    for (k, v) in &matches {
        let desc = ServerProperties::property_description(k);
        let label = if desc.is_empty() {
            k.to_string()
        } else {
            format!("{} ({})", k, desc)
        };

        if v == "true" || v == "false" || props.get_bool(k).is_some() {
            form = form.with_field(FormField::checkbox_with_default(
                k.clone(),
                label,
                v == "true",
            ));
        } else if props.get_i32(k).is_some() || v.parse::<i64>().is_ok() {
            form = form.with_field(FormField::integer(k.clone(), label).with_default(v.clone()));
        } else {
            form = form.with_field(FormField::string(k.clone(), label).with_default(v.clone()));
        }
    }

    if let Some(result) = form.run()? {
        if !props_path.exists() {
            show_modal_message(
                "SERVER NOT FOUND",
                &["Server was deleted before settings could be saved.".to_string()],
                true,
            )?;
            return Ok(());
        }
        for (k, _) in &matches {
            if let Some(new_val) = result.get(k) {
                props.set(k, new_val);
            }
        }
        props.save(props_path)?;
        show_modal_message(
            "SETTINGS SAVED",
            &[format!(
                "Successfully saved matched settings for '{}'.",
                server_name
            )],
            false,
        )?;
    }

    Ok(())
}
