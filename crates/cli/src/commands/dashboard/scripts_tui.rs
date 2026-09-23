use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use craft_core::{CraftPaths, Result};
use craft_scripting::{mlua, HookBus, HookContext, LifecycleEvent, LuaEngine};

use super::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, show_modal_message, AltScreenGuard, MenuEntry, NavGuard,
};

pub async fn scripts_hooks_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Scripts & Hooks");
    let mut selected = 0;

    loop {
        let hooks = HookBus::discover_hooks(paths, None);
        let scripts_dir = paths.home.join("scripts");
        let script_count = if scripts_dir.is_dir() {
            fs::read_dir(&scripts_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .map(|ext| ext == "lua")
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Active Lifecycle Hooks: {} | Custom Scripts in store: {}\r\n Standard Library: craft.servers, craft.backup, craft.net, craft.http\r\n{}",
            box_top(width).cyan().bold(),
            box_title("LUA AUTOMATION & LIFECYCLE HOOKS", width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            hooks.len().to_string().green().bold(),
            script_count.to_string().cyan().bold(),
            box_divider(width).dimmed(),
        );

        let entries = vec![
            MenuEntry::new("1", "List Registered Lifecycle Hooks").with_aliases(&["l", "list"]),
            MenuEntry::new("2", "Simulate Lifecycle Hook Event").with_aliases(&["t", "test"]),
            MenuEntry::new("3", "Run Script File").with_aliases(&["r", "run"]),
            MenuEntry::new("4", "Evaluate Lua Expression").with_aliases(&["e", "eval"]),
            MenuEntry::new("5", "Scaffold New Script or Hook").with_aliases(&["n", "new"]),
            MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                view_registered_hooks_modal(&hooks)?;
            }
            Some(1) => {
                simulate_hook_event_tui(paths).await?;
            }
            Some(2) => {
                run_script_file_tui(paths).await?;
            }
            Some(3) => {
                eval_lua_expression_tui(paths).await?;
            }
            Some(4) => {
                scaffold_script_tui(paths).await?;
            }
            _ => break,
        }
    }

    Ok(())
}

fn view_registered_hooks_modal(hooks: &[craft_scripting::HookDefinition]) -> Result<()> {
    if hooks.is_empty() {
        show_modal_message(
            "REGISTERED HOOKS",
            &[
                "No lifecycle hooks are currently registered.".dimmed().to_string(),
                "Place global hooks in ~/.craft/hooks/*.lua or per-server in <server_dir>/hooks.lua".to_string(),
            ],
            false,
        )?;
        return Ok(());
    }

    let mut lines = Vec::new();
    lines.push(format!("{:<20} {:<15} {}", "TRIGGER EVENT", "SCOPE", "SCRIPT PATH").bold().to_string());
    lines.push("-".repeat(70).dimmed().to_string());

    for hook in hooks {
        lines.push(format!(
            "{:<20} {:<15} {}",
            format!("[{}]", hook.event).green(),
            hook.scope.cyan(),
            hook.path.display()
        ));
    }

    show_modal_message("REGISTERED LIFECYCLE HOOKS", &lines, false)?;
    Ok(())
}

async fn simulate_hook_event_tui(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut ev_sel = 0;

    let width = get_content_width(80);
    let ev_header = format!(
        "{}\r\n{}\r\n{}\r\n Select a server lifecycle event to trigger simulate dispatch:\r\n{}",
        box_top(width).cyan().bold(),
        box_title("SIMULATE LIFECYCLE EVENT", width, false).cyan().bold(),
        box_divider(width).cyan().bold(),
        box_divider(width).dimmed(),
    );

    let ev_entries = vec![
        MenuEntry::new("1", "ServerStart"),
        MenuEntry::new("2", "ServerStop"),
        MenuEntry::new("3", "ServerCrash"),
        MenuEntry::new("4", "CircuitTrip"),
        MenuEntry::new("5", "BackupStart"),
        MenuEntry::new("6", "BackupComplete"),
        MenuEntry::new("7", "StorageLow"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let event = match run_menu(&ev_header, &ev_entries, &mut ev_sel)? {
        Some(0) => LifecycleEvent::ServerStart,
        Some(1) => LifecycleEvent::ServerStop,
        Some(2) => LifecycleEvent::ServerCrash,
        Some(3) => LifecycleEvent::CircuitTrip,
        Some(4) => LifecycleEvent::BackupStart,
        Some(5) => LifecycleEvent::BackupComplete,
        Some(6) => LifecycleEvent::StorageLow,
        _ => return Ok(()),
    };

    let server_name = match run_input_prompt(
        "EVENT SERVER CONTEXT",
        "Enter server name context (or press Enter for global test-server):",
        Some("test-server"),
    )? {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "test-server".to_string(),
    };

    let server_dir = paths.servers_dir.join(&server_name);
    let mut ctx = HookContext::new(event.clone());
    ctx.server_name = Some(server_name.clone());
    ctx.server_path = Some(server_dir.display().to_string());

    print_in_place_status(
        "DISPATCHING HOOK",
        &[format!("Simulating [{}] event for server '{}'...", event.as_str(), server_name)],
    )?;

    let results = HookBus::dispatch(paths, event.clone(), &ctx, 30);

    if results.is_empty() {
        show_modal_message(
            "DISPATCH RESULT",
            &[
                format!("[OK] Dispatched event [{}].", event.as_str()).green().bold().to_string(),
                "No matching hook scripts were found for this event/server scope.".dimmed().to_string(),
            ],
            false,
        )?;
    } else {
        let mut lines = Vec::new();
        let total = results.len();
        let successful = results.iter().filter(|r| r.success).count();
        lines.push(format!("Executed: {} hook(s) | Success: {} | Failed: {}", total, successful, total - successful));
        lines.push("-".repeat(70).dimmed().to_string());

        for res in results {
            let status = if res.success {
                "[OK]".green().bold().to_string()
            } else {
                "[FAILED]".red().bold().to_string()
            };
            lines.push(format!("{} {:<25} ({:.2}ms)", status, res.hook_name, res.duration_ms));
            if let Some(err) = res.error {
                lines.push(format!("   Error: {}", err).red().to_string());
            }
        }
        show_modal_message("DISPATCH SUMMARY", &lines, false)?;
    }

    Ok(())
}

async fn run_script_file_tui(paths: &CraftPaths) -> Result<()> {
    let script_path_str = match run_input_prompt(
        "RUN SCRIPT",
        "Enter script path (.lua) to execute:",
        Some("scripts/maintenance.lua"),
    )? {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Ok(()),
    };

    let script_path = PathBuf::from(&script_path_str);
    let resolved_path = if script_path.is_absolute() {
        script_path
    } else {
        paths.home.join(&script_path)
    };

    if !resolved_path.exists() {
        show_modal_message(
            "FILE NOT FOUND",
            &[format!("[ERROR] Script file does not exist: {}", resolved_path.display())],
            true,
        )?;
        return Ok(());
    }

    print_in_place_status(
        "RUNNING SCRIPT",
        &[format!("Executing {}...", resolved_path.display())],
    )?;

    let engine = LuaEngine::new_with_paths(paths)?;
    match engine.run_file_with_timeout(&resolved_path, &[], 30) {
        Ok(()) => {
            show_modal_message(
                "SCRIPT COMPLETED",
                &[
                    format!("[OK] Script execution finished cleanly: {}", resolved_path.file_name().unwrap_or_default().to_string_lossy()).green().bold().to_string(),
                    format!("Path: {}", resolved_path.display()),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "SCRIPT ERROR",
                &[
                    format!("[ERROR] Execution failed: {}", e).red().bold().to_string(),
                    format!("Path: {}", resolved_path.display()),
                ],
                true,
            )?;
        }
    }

    Ok(())
}

async fn eval_lua_expression_tui(paths: &CraftPaths) -> Result<()> {
    let expr = match run_input_prompt(
        "EVALUATE LUA EXPRESSION",
        "Enter Lua expression or statement:",
        Some("return craft.platform.os() .. ' with ' .. craft.platform.arch()"),
    )? {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Ok(()),
    };

    print_in_place_status("EVALUATING", &[format!("Evaluating: {}", expr)])?;

    let engine = LuaEngine::new_with_paths(paths)?;
    match engine.eval_with_timeout::<mlua::Value>(&expr, 10) {
        Ok(val) => {
            let type_str = match &val {
                mlua::Value::Nil => "nil",
                mlua::Value::Boolean(_) => "boolean",
                mlua::Value::Integer(_) => "integer",
                mlua::Value::Number(_) => "number",
                mlua::Value::String(_) => "string",
                mlua::Value::Table(_) => "table",
                mlua::Value::Function(_) => "function",
                mlua::Value::UserData(_) | mlua::Value::LightUserData(_) => "userdata",
                mlua::Value::Thread(_) => "thread",
                mlua::Value::Error(_) => "error",
                _ => "other",
            };
            show_modal_message(
                "EVAL RESULT",
                &[
                    format!("[OK] Result: {:?}", val).green().bold().to_string(),
                    format!("Type:   {}", type_str),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "EVALUATION ERROR",
                &[format!("[ERROR] Lua evaluation error: {}", e).red().bold().to_string()],
                true,
            )?;
        }
    }

    Ok(())
}

async fn scaffold_script_tui(paths: &CraftPaths) -> Result<()> {
    let name = match run_input_prompt(
        "NEW SCRIPT SCAFFOLD",
        "Enter script name (e.g. backup_notify or health_check):",
        Some("backup_notify"),
    )? {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Ok(()),
    };

    let filename = if name.ends_with(".lua") {
        name
    } else {
        format!("{}.lua", name)
    };

    let target_dir = paths.home.join("scripts");
    if let Err(e) = fs::create_dir_all(&target_dir) {
        show_modal_message("ERROR", &[format!("Failed to create scripts dir: {}", e)], true)?;
        return Ok(());
    }

    let target_path = target_dir.join(&filename);
    if target_path.exists() {
        show_modal_message(
            "FILE EXISTS",
            &[format!("Script already exists at: {}", target_path.display())],
            true,
        )?;
        return Ok(());
    }

    let template = format!(
        r#"-- Craft Automation Script: {}
-- Created via Craft ModalX Studio

craft.log.info("Starting script: {}")

-- Query servers
local servers = craft.servers.list()
craft.log.info("Found " .. #servers .. " registered server(s)")

for _, s in ipairs(servers) do
    craft.log.info("Server: " .. s.name .. " (Running: " .. tostring(s.running) .. ")")
end

craft.log.info("Script finished successfully.")
"#,
        filename, filename
    );

    if let Err(e) = fs::write(&target_path, template) {
        show_modal_message("ERROR", &[format!("Failed to write script: {}", e)], true)?;
        return Ok(());
    }

    show_modal_message(
        "SCRIPT CREATED",
        &[
            format!("[OK] Created new automation script:").green().bold().to_string(),
            format!("Path: {}", target_path.display()),
            format!("You can run it with: craft script run {}", target_path.display()),
        ],
        false,
    )?;

    Ok(())
}
