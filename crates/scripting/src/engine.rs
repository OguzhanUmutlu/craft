use crate::config::CustomServerConfig;
use craft_core::{CraftError, Result};
use mlua::{Lua, LuaSerdeExt};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub struct LuaEngine {
    lua: Lua,
}

impl LuaEngine {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        let engine = Self { lua };
        engine.init_craft_globals(None)?;
        Ok(engine)
    }

    pub fn new_with_server_context(
        server_dir: &Path,
        config: &CustomServerConfig,
        pid: Option<u32>,
    ) -> Result<Self> {
        let lua = Lua::new();
        let engine = Self { lua };
        engine.init_craft_globals(Some((server_dir, config, pid)))?;
        Ok(engine)
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn eval<T: mlua::FromLuaMulti>(&self, source: &str) -> Result<T> {
        self.lua
            .load(source)
            .eval()
            .map_err(|e| CraftError::Other(format!("Lua evaluation error: {}", e)))
    }

    pub fn exec(&self, source: &str) -> Result<()> {
        self.lua
            .load(source)
            .exec()
            .map_err(|e| CraftError::Other(format!("Lua execution error: {}", e)))
    }

    fn init_craft_globals(
        &self,
        ctx: Option<(&Path, &CustomServerConfig, Option<u32>)>,
    ) -> Result<()> {
        let globals = self.lua.globals();

        let craft_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // craft.version
        craft_table
            .set("version", env!("CARGO_PKG_VERSION"))
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Logging
        let log_fn = self
            .lua
            .create_function(|_, msg: String| {
                println!("[craft] [INFO] {}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("log", log_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let info_fn = self
            .lua
            .create_function(|_, msg: String| {
                info!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("info", info_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let warn_fn = self
            .lua
            .create_function(|_, msg: String| {
                warn!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("warn", warn_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let error_fn = self
            .lua
            .create_function(|_, msg: String| {
                error!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("error", error_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Sleep
        let sleep_fn = self
            .lua
            .create_function(|_, ms: u64| {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("sleep", sleep_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Time
        let time_fn = self
            .lua
            .create_function(|_, ()| {
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                Ok(secs)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("time", time_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let time_ms_fn = self
            .lua
            .create_function(|_, ()| {
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                Ok(ms)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("time_ms", time_ms_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Environment variables
        let env_fn = self
            .lua
            .create_function(|_, (key, val): (String, Option<String>)| {
                if let Some(v) = val {
                    std::env::set_var(&key, &v);
                    Ok(Some(v))
                } else {
                    Ok(std::env::var(&key).ok())
                }
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("env", env_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // File system
        let fs_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_read = self
            .lua
            .create_function(|_, path: String| {
                fs::read_to_string(&path)
                    .map_err(|e| mlua::Error::RuntimeError(format!("Read failed: {}", e)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("read", fs_read)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_write = self
            .lua
            .create_function(|_, (path, content): (String, String)| {
                fs::write(&path, content)
                    .map_err(|e| mlua::Error::RuntimeError(format!("Write failed: {}", e)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("write", fs_write)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_append = self
            .lua
            .create_function(|_, (path, content): (String, String)| {
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| mlua::Error::RuntimeError(format!("Append open failed: {}", e)))?;
                file.write_all(content.as_bytes()).map_err(|e| {
                    mlua::Error::RuntimeError(format!("Append write failed: {}", e))
                })?;
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("append", fs_append)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_exists = self
            .lua
            .create_function(|_, path: String| Ok(Path::new(&path).exists()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("exists", fs_exists)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_is_file = self
            .lua
            .create_function(|_, path: String| Ok(Path::new(&path).is_file()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("is_file", fs_is_file)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_is_dir = self
            .lua
            .create_function(|_, path: String| Ok(Path::new(&path).is_dir()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("is_dir", fs_is_dir)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_mkdir = self
            .lua
            .create_function(|_, path: String| {
                fs::create_dir_all(&path)
                    .map_err(|e| mlua::Error::RuntimeError(format!("mkdir failed: {}", e)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("mkdir", fs_mkdir)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_list = self
            .lua
            .create_function(|_, path: String| {
                let entries = fs::read_dir(&path)
                    .map_err(|e| mlua::Error::RuntimeError(format!("list failed: {}", e)))?;
                let mut list = Vec::new();
                for entry in entries.flatten() {
                    list.push(entry.file_name().to_string_lossy().to_string());
                }
                Ok(list)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("list", fs_list)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("fs", fs_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Exec command helper
        let exec_fn = self
            .lua
            .create_function(|lua, (cmd, args): (String, Option<Vec<String>>)| {
                let mut c = Command::new(&cmd);
                if let Some(a) = args {
                    c.args(a);
                }
                match c.output() {
                    Ok(out) => {
                        let res_tbl = lua.create_table()?;
                        res_tbl.set("code", out.status.code().unwrap_or(-1))?;
                        res_tbl.set("success", out.status.success())?;
                        res_tbl.set("stdout", String::from_utf8_lossy(&out.stdout).to_string())?;
                        res_tbl.set("stderr", String::from_utf8_lossy(&out.stderr).to_string())?;
                        Ok(res_tbl)
                    }
                    Err(e) => {
                        let res_tbl = lua.create_table()?;
                        res_tbl.set("code", -1)?;
                        res_tbl.set("success", false)?;
                        res_tbl.set("stdout", "")?;
                        res_tbl.set("stderr", e.to_string())?;
                        Ok(res_tbl)
                    }
                }
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("exec", exec_fn)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Contextual craft.server table
        if let Some((server_dir, config, pid)) = ctx {
            let server_tbl = self
                .lua
                .create_table()
                .map_err(|e| CraftError::Other(e.to_string()))?;
            server_tbl
                .set("name", config.server.name.as_str())
                .map_err(|e| CraftError::Other(e.to_string()))?;
            server_tbl
                .set("path", server_dir.to_string_lossy().to_string())
                .map_err(|e| CraftError::Other(e.to_string()))?;
            server_tbl
                .set("port", config.network.port)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            server_tbl
                .set("game", "custom")
                .map_err(|e| CraftError::Other(e.to_string()))?;
            server_tbl
                .set("runtime_type", config.server.runtime_type.as_str())
                .map_err(|e| CraftError::Other(e.to_string()))?;
            if let Some(p) = pid {
                server_tbl
                    .set("pid", p)
                    .map_err(|e| CraftError::Other(e.to_string()))?;
            }
            craft_table
                .set("server", server_tbl)
                .map_err(|e| CraftError::Other(e.to_string()))?;
        }

        globals
            .set("craft", craft_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(())
    }

    pub fn run_file(&self, script_path: &Path, args: &[String]) -> Result<()> {
        if !script_path.exists() {
            return Err(CraftError::Other(format!(
                "Lua script not found at '{}'",
                script_path.display()
            )));
        }

        let content = fs::read_to_string(script_path)
            .map_err(|e| CraftError::Other(format!("Failed to read Lua script: {}", e)))?;

        // Populate global 'arg' table standard in Lua CLI
        let arg_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;
        arg_table
            .set(0, script_path.to_string_lossy().to_string())
            .map_err(|e| CraftError::Other(e.to_string()))?;
        for (i, arg) in args.iter().enumerate() {
            arg_table
                .set(i + 1, arg.clone())
                .map_err(|e| CraftError::Other(e.to_string()))?;
        }
        self.lua
            .globals()
            .set("arg", arg_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        self.lua
            .load(&content)
            .set_name(script_path.to_string_lossy())
            .exec()
            .map_err(|e| CraftError::Other(format!("Lua execution error: {}", e)))?;

        Ok(())
    }

    fn load_hooks_file(&self, server_dir: &Path, config: &CustomServerConfig) -> Result<bool> {
        let hooks_name = config.lifecycle.hooks.as_deref().unwrap_or("hooks.lua");
        let hooks_path = server_dir.join(hooks_name);
        if !hooks_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(&hooks_path)
            .map_err(|e| CraftError::Other(format!("Failed to read hooks file: {}", e)))?;

        self.lua
            .load(&content)
            .set_name(hooks_path.to_string_lossy())
            .exec()
            .map_err(|e| CraftError::Other(format!("Error loading hooks.lua: {}", e)))?;

        Ok(true)
    }

    pub fn run_pre_start(server_dir: &Path, config: &CustomServerConfig) -> Result<()> {
        let engine = Self::new_with_server_context(server_dir, config, None)?;
        if !engine.load_hooks_file(server_dir, config)? {
            return Ok(());
        }

        let globals = engine.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("on_pre_start") {
            let server_val = engine
                .lua
                .to_value(&config.server)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            func.call::<()>(server_val)
                .map_err(|e| CraftError::Other(format!("Hook on_pre_start failed: {}", e)))?;
        }
        Ok(())
    }

    pub fn run_post_start(server_dir: &Path, config: &CustomServerConfig, pid: u32) -> Result<()> {
        let engine = Self::new_with_server_context(server_dir, config, Some(pid))?;
        if !engine.load_hooks_file(server_dir, config)? {
            return Ok(());
        }

        let globals = engine.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("on_post_start") {
            let server_val = engine
                .lua
                .to_value(&config.server)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            func.call::<()>((server_val, pid))
                .map_err(|e| CraftError::Other(format!("Hook on_post_start failed: {}", e)))?;
        }
        Ok(())
    }

    pub fn run_pre_stop(server_dir: &Path, config: &CustomServerConfig, pid: u32) -> Result<()> {
        let engine = Self::new_with_server_context(server_dir, config, Some(pid))?;
        if !engine.load_hooks_file(server_dir, config)? {
            return Ok(());
        }

        let globals = engine.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("on_pre_stop") {
            let server_val = engine
                .lua
                .to_value(&config.server)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            func.call::<()>((server_val, pid))
                .map_err(|e| CraftError::Other(format!("Hook on_pre_stop failed: {}", e)))?;
        }
        Ok(())
    }

    pub fn run_post_stop(
        server_dir: &Path,
        config: &CustomServerConfig,
        exit_code: i32,
    ) -> Result<()> {
        let engine = Self::new_with_server_context(server_dir, config, None)?;
        if !engine.load_hooks_file(server_dir, config)? {
            return Ok(());
        }

        let globals = engine.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("on_post_stop") {
            let server_val = engine
                .lua
                .to_value(&config.server)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            func.call::<()>((server_val, exit_code))
                .map_err(|e| CraftError::Other(format!("Hook on_post_stop failed: {}", e)))?;
        }
        Ok(())
    }

    pub fn run_health_check(server_dir: &Path, config: &CustomServerConfig) -> Result<bool> {
        let engine = Self::new_with_server_context(server_dir, config, None)?;
        if !engine.load_hooks_file(server_dir, config)? {
            return Ok(true);
        }

        let globals = engine.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("health_check") {
            let server_val = engine
                .lua
                .to_value(&config.server)
                .map_err(|e| CraftError::Other(e.to_string()))?;
            let res = func
                .call::<bool>(server_val)
                .map_err(|e| CraftError::Other(format!("Hook health_check failed: {}", e)))?;
            Ok(res)
        } else {
            Ok(true)
        }
    }
}
