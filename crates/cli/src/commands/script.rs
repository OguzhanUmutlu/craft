use crate::cli::ScriptCommands;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_scripting::{mlua, HookBus, HookContext, LifecycleEvent, LuaEngine};
use std::fs;
use std::path::{Path, PathBuf};

pub async fn handle_script(paths: &CraftPaths, command: &ScriptCommands) -> Result<()> {
    match command {
        ScriptCommands::Run {
            path,
            server,
            timeout,
            args,
        } => run_script_file(paths, path, server.as_deref(), *timeout, args).await,
        ScriptCommands::Eval {
            code,
            server,
            timeout,
        } => eval_script_expression(paths, code, server.as_deref(), *timeout).await,
        ScriptCommands::List { server } => list_hooks(paths, server.as_deref()),
        ScriptCommands::Test { event, server } => {
            test_hook_event(paths, event, server.as_deref()).await
        }
        ScriptCommands::New { hook_name, server } => {
            scaffold_hook(paths, hook_name, server.as_deref())
        }
    }
}

async fn run_script_file(
    paths: &CraftPaths,
    script_path: &Path,
    server_name: Option<&str>,
    timeout: u64,
    args: &[String],
) -> Result<()> {
    if !script_path.exists() {
        return Err(CraftError::Other(format!(
            "Script file not found at '{}'",
            script_path.display()
        )));
    }

    let (server_dir, custom_cfg, pid) = resolve_server_context(paths, server_name)?;

    let engine = LuaEngine::new_full(
        Some(paths),
        server_dir.as_deref(),
        custom_cfg.as_ref(),
        pid,
    )?;

    println!(
        "{} Running '{}' (timeout: {}s)...",
        "[CRAFT]".cyan().bold(),
        script_path.display(),
        timeout
    );

    let path_buf = script_path.to_path_buf();
    let args_vec = args.to_vec();
    let res = tokio::task::spawn_blocking(move || {
        engine.run_file_with_timeout(&path_buf, &args_vec, timeout)
    })
    .await
    .map_err(|e| CraftError::Other(format!("Execution task panicked: {}", e)))?;

    match res {
        Ok(()) => {
            println!(
                "{} Script execution finished successfully.",
                "[OK]".green().bold()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("{}: {}", "[ERROR]".red().bold(), e);
            Err(e)
        }
    }
}

async fn eval_script_expression(
    paths: &CraftPaths,
    code: &str,
    server_name: Option<&str>,
    timeout: u64,
) -> Result<()> {
    let (server_dir, custom_cfg, pid) = resolve_server_context(paths, server_name)?;

    let engine = LuaEngine::new_full(
        Some(paths),
        server_dir.as_deref(),
        custom_cfg.as_ref(),
        pid,
    )?;

    let code_str = code.to_string();
    let res = tokio::task::spawn_blocking(move || {
        let val = engine.eval_with_timeout::<mlua::Value>(&code_str, timeout)?;
        print_lua_value(&val);
        Ok::<(), CraftError>(())
    })
    .await
    .map_err(|e| CraftError::Other(format!("Evaluation task panicked: {}", e)))?;

    match res {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("{}: {}", "[ERROR]".red().bold(), e);
            Err(e)
        }
    }
}

fn print_lua_value(val: &mlua::Value) {
    match val {
        mlua::Value::Nil => println!("nil"),
        mlua::Value::Boolean(b) => println!("{}", b),
        mlua::Value::Integer(i) => println!("{}", i),
        mlua::Value::Number(n) => println!("{}", n),
        mlua::Value::String(s) => {
            if let Ok(st) = s.to_str() {
                println!("{}", st);
            }
        }
        mlua::Value::Table(tbl) => {
            println!("Table ({} entries):", tbl.len().unwrap_or(0));
            for pair in tbl.clone().pairs::<mlua::Value, mlua::Value>().flatten() {
                let (k, v) = pair;
                let k_str = format_lua_simple(&k);
                let v_str = format_lua_simple(&v);
                println!("  {} => {}", k_str.cyan(), v_str);
            }
        }
        other => println!("{:?}", other),
    }
}

fn format_lua_simple(val: &mlua::Value) -> String {
    match val {
        mlua::Value::Nil => "nil".to_string(),
        mlua::Value::Boolean(b) => b.to_string(),
        mlua::Value::Integer(i) => i.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::String(s) => s.to_str().map(|b| b.to_string()).unwrap_or_default(),
        mlua::Value::Table(t) => format!("[table:{}]", t.len().unwrap_or(0)),
        _ => format!("{:?}", val),
    }
}

fn list_hooks(paths: &CraftPaths, server_filter: Option<&str>) -> Result<()> {
    let hooks = HookBus::discover_hooks(paths, server_filter);

    if hooks.is_empty() {
        println!("{}: No lifecycle hooks found.", "[INFO]".cyan().bold());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("Scope").fg(Color::Cyan),
        Cell::new("Script Name").fg(Color::Cyan),
        Cell::new("Event Trigger").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
        Cell::new("Location").fg(Color::Cyan),
    ]);

    for h in hooks {
        let status_cell = if h.active {
            Cell::new("[ACTIVE]").fg(Color::Green)
        } else {
            Cell::new("[ABSENT]").fg(Color::DarkGrey)
        };

        table.add_row(vec![
            Cell::new(&h.scope),
            Cell::new(&h.name),
            Cell::new(&h.event),
            status_cell,
            Cell::new(h.path.to_string_lossy()),
        ]);
    }

    println!("{}", table);
    Ok(())
}

async fn test_hook_event(
    paths: &CraftPaths,
    event_name: &str,
    server_name: Option<&str>,
) -> Result<()> {
    let event = LifecycleEvent::from_name(event_name).ok_or_else(|| {
        CraftError::Other(format!(
            "Unknown event '{}'. Supported events: on_server_start, on_server_stop, on_server_crash, on_backup_start, on_backup_complete, on_circuit_trip, on_storage_low, on_anomaly_detected",
            event_name
        ))
    })?;

    let mut ctx = HookContext::new(event);

    if let Some(sname) = server_name {
        if let Ok(reg) = ServersRegistry::load(paths) {
            if let Some(s) = reg.find_by_name(sname) {
                ctx.server_name = Some(s.name.clone());
                ctx.server_path = Some(s.path.to_string_lossy().to_string());
                ctx.port = s.port;
                ctx.software = Some(s.software.clone());
                ctx.version = Some(s.version.clone());
            }
        }
    }

    ctx.exit_code = Some(1);
    ctx.crashes = Some(1);
    ctx.details = Some("Manual CLI test dispatch".to_string());

    println!(
        "{} Simulating event '{}' for scope '{}'...",
        "[TEST]".cyan().bold(),
        event.as_str(),
        server_name.unwrap_or("global")
    );

    let p = paths.clone();
    let results = tokio::task::spawn_blocking(move || HookBus::dispatch(&p, event, &ctx, 10))
        .await
        .map_err(|e| CraftError::Other(format!("Test dispatch panicked: {}", e)))?;

    if results.is_empty() {
        println!(
            "{} No active hook scripts responded to event '{}'.",
            "[WARN]".yellow().bold(),
            event.as_str()
        );
        println!(
            "Use 'craft script new {}' to create a starter hook script.",
            event.as_str()
        );
        return Ok(());
    }

    for r in results {
        if r.success {
            println!(
                "{} {} in {}ms",
                "[OK]".green().bold(),
                r.hook_name,
                r.duration_ms
            );
        } else {
            println!(
                "{} {} in {}ms: {}",
                "[FAIL]".red().bold(),
                r.hook_name,
                r.duration_ms,
                r.error.as_deref().unwrap_or("unknown error")
            );
        }
    }

    Ok(())
}

fn scaffold_hook(paths: &CraftPaths, hook_name: &str, server_name: Option<&str>) -> Result<()> {
    let (target_dir, file_name) = if let Some(sname) = server_name {
        let reg = ServersRegistry::load(paths)?;
        let s = reg
            .find_by_name(sname)
            .ok_or_else(|| CraftError::Other(format!("Server '{}' not found", sname)))?;
        (s.path.clone(), "hooks.lua".to_string())
    } else {
        let hdir = HookBus::ensure_hooks_dir(paths)?;
        let fname = if hook_name.ends_with(".lua") {
            hook_name.to_string()
        } else if let Some(ev) = LifecycleEvent::from_name(hook_name) {
            format!("{}.lua", ev.as_str())
        } else {
            format!("{}.lua", hook_name)
        };
        (hdir, fname)
    };

    let target_file = target_dir.join(&file_name);
    if target_file.exists() {
        return Err(CraftError::Other(format!(
            "Hook file already exists at '{}'",
            target_file.display()
        )));
    }

    let template = format!(
        r#"-- Craft Lifecycle Hook Script: {name}
-- Global context table: 'ctx'
-- Global APIs: craft.servers, craft.backup, craft.net, craft.http, craft.audit, craft.fs, craft.exec, craft.json

function on_server_start(ctx)
    craft.log("[HOOK] Server " .. (ctx.server_name or "server") .. " started (PID: " .. tostring(ctx.pid or "none") .. ")")
end

function on_server_stop(ctx)
    craft.log("[HOOK] Server " .. (ctx.server_name or "server") .. " stopped")
end

function on_server_crash(ctx)
    craft.warn("[HOOK] Server " .. (ctx.server_name or "server") .. " crashed with code " .. tostring(ctx.exit_code or "none"))
end

function on_backup_complete(ctx)
    craft.log("[HOOK] Backup finished: " .. (ctx.backup_file or "unknown") .. " (" .. tostring(ctx.backup_bytes or 0) .. " bytes)")
end

function on_storage_low(ctx)
    craft.warn("[HOOK] Low disk alert: " .. (ctx.details or "storage exhaustion"))
end
"#,
        name = file_name
    );

    fs::write(&target_file, template)?;
    println!(
        "{} Scaffolded hook template at '{}'.",
        "[OK]".green().bold(),
        target_file.display()
    );
    Ok(())
}

fn resolve_server_context(
    paths: &CraftPaths,
    server_name: Option<&str>,
) -> Result<(
    Option<PathBuf>,
    Option<craft_scripting::CustomServerConfig>,
    Option<u32>,
)> {
    if let Some(sname) = server_name {
        let reg = ServersRegistry::load(paths)?;
        if let Some(s) = reg.find_by_name(sname) {
            let custom_cfg = craft_scripting::CustomServerConfig::load_from_dir(&s.path)
                .ok()
                .flatten();
            let pid = craft_core::read_pid_file(&s.path.join("server.pid"));
            return Ok((Some(s.path.clone()), custom_cfg, pid));
        }
    }
    Ok((None, None, None))
}
