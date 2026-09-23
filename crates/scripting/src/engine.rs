use crate::config::CustomServerConfig;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use mlua::{Lua, LuaSerdeExt};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

pub struct LuaEngine {
    lua: Lua,
}

impl LuaEngine {
    pub fn new() -> Result<Self> {
        Self::new_full(None, None, None, None)
    }

    pub fn new_with_paths(paths: &CraftPaths) -> Result<Self> {
        Self::new_full(Some(paths), None, None, None)
    }

    pub fn new_with_server_context(
        server_dir: &Path,
        config: &CustomServerConfig,
        pid: Option<u32>,
    ) -> Result<Self> {
        Self::new_full(None, Some(server_dir), Some(config), pid)
    }

    pub fn new_full(
        paths: Option<&CraftPaths>,
        server_dir: Option<&Path>,
        config: Option<&CustomServerConfig>,
        pid: Option<u32>,
    ) -> Result<Self> {
        let lua = Lua::new();
        let engine = Self { lua };
        engine.init_craft_globals(paths, server_dir, config, pid)?;
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

    pub fn eval_with_timeout<T: mlua::FromLuaMulti>(
        &self,
        source: &str,
        timeout_secs: u64,
    ) -> Result<T> {
        let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs.max(1));
        let _ = self.lua.set_hook(mlua::HookTriggers::default().every_nth_instruction(5000), move |_, _| {
            if Instant::now() >= deadline {
                Err(mlua::Error::RuntimeError(format!(
                    "Lua evaluation timed out after {}s",
                    timeout_secs
                )))
            } else {
                Ok(mlua::VmState::Continue)
            }
        });
        let res = self.eval(source);
        self.lua.remove_hook();
        res
    }

    pub fn exec(&self, source: &str) -> Result<()> {
        self.lua
            .load(source)
            .exec()
            .map_err(|e| CraftError::Other(format!("Lua execution error: {}", e)))
    }

    fn init_craft_globals(
        &self,
        paths: Option<&CraftPaths>,
        server_dir: Option<&Path>,
        config: Option<&CustomServerConfig>,
        pid: Option<u32>,
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
        let log_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let log_info = self
            .lua
            .create_function(|_, msg: String| {
                println!("[craft] [INFO] {}", msg);
                info!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        log_table
            .set("info", log_info.clone())
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let log_warn = self
            .lua
            .create_function(|_, msg: String| {
                eprintln!("[craft] [WARN] {}", msg);
                warn!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        log_table
            .set("warn", log_warn.clone())
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let log_error = self
            .lua
            .create_function(|_, msg: String| {
                eprintln!("[craft] [ERROR] {}", msg);
                error!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        log_table
            .set("error", log_error.clone())
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let log_debug = self
            .lua
            .create_function(|_, msg: String| {
                debug!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        log_table
            .set("debug", log_debug)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Metatable to allow craft.log("msg") directly as well as craft.log.info("msg")
        let log_meta = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;
        let log_call = self
            .lua
            .create_function(|_, (_, msg): (mlua::Value, String)| {
                println!("[craft] [INFO] {}", msg);
                info!("{}", msg);
                Ok(())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        log_meta
            .set("__call", log_call)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        let _ = log_table.set_metatable(Some(log_meta));

        craft_table
            .set("log", log_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("info", log_info)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("warn", log_warn)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("error", log_error)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Platform table
        let platform_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;
        let plat_os = self
            .lua
            .create_function(|_, ()| Ok(std::env::consts::OS.to_string()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("os", plat_os)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let plat_arch = self
            .lua
            .create_function(|_, ()| Ok(std::env::consts::ARCH.to_string()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("arch", plat_arch)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let plat_is_win = self
            .lua
            .create_function(|_, ()| Ok(cfg!(target_os = "windows")))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("is_windows", plat_is_win)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let plat_is_lin = self
            .lua
            .create_function(|_, ()| Ok(cfg!(target_os = "linux")))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("is_linux", plat_is_lin)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let plat_is_mac = self
            .lua
            .create_function(|_, ()| Ok(cfg!(target_os = "macos")))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("is_macos", plat_is_mac)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let plat_env = self
            .lua
            .create_function(|_, key: String| Ok(std::env::var(&key).ok()))
            .map_err(|e| CraftError::Other(e.to_string()))?;
        platform_table
            .set("env", plat_env)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("platform", platform_table)
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
            .set("read", fs_read.clone())
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("read_string", fs_read)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_write = self
            .lua
            .create_function(|_, (path, content): (String, String)| {
                fs::write(&path, content)
                    .map_err(|e| mlua::Error::RuntimeError(format!("Write failed: {}", e)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("write", fs_write.clone())
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("write_string", fs_write)
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

        let fs_copy = self
            .lua
            .create_function(|_, (src, dst): (String, String)| {
                fs::copy(&src, &dst)
                    .map_err(|e| mlua::Error::RuntimeError(format!("copy failed: {}", e)))?;
                Ok(true)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("copy", fs_copy)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_remove = self
            .lua
            .create_function(|_, path: String| {
                let p = Path::new(&path);
                if p.is_dir() {
                    fs::remove_dir_all(p)
                        .map_err(|e| mlua::Error::RuntimeError(format!("remove dir failed: {}", e)))?;
                } else if p.exists() {
                    fs::remove_file(p)
                        .map_err(|e| mlua::Error::RuntimeError(format!("remove file failed: {}", e)))?;
                }
                Ok(true)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("remove", fs_remove)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let fs_size = self
            .lua
            .create_function(|_, path: String| {
                let meta = fs::metadata(&path)
                    .map_err(|e| mlua::Error::RuntimeError(format!("metadata failed: {}", e)))?;
                Ok(meta.len())
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        fs_table
            .set("size", fs_size)
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

        // JSON decode / encode
        let json_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;
        let json_decode = self
            .lua
            .create_function(|lua, s: String| {
                let val: serde_json::Value = serde_json::from_str(&s)
                    .map_err(|e| mlua::Error::RuntimeError(format!("JSON decode error: {}", e)))?;
                lua.to_value(&val)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        json_table
            .set("decode", json_decode)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let json_encode = self
            .lua
            .create_function(|lua, val: mlua::Value| {
                let serde_val: serde_json::Value = lua
                    .from_value(val)
                    .map_err(|e| mlua::Error::RuntimeError(format!("JSON encode error: {}", e)))?;
                serde_json::to_string(&serde_val)
                    .map_err(|e| mlua::Error::RuntimeError(format!("JSON encode error: {}", e)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        json_table
            .set("encode", json_encode)
            .map_err(|e| CraftError::Other(e.to_string()))?;
        craft_table
            .set("json", json_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let effective_paths = paths.cloned().or_else(|| CraftPaths::new().ok());

        // HTTP table
        let http_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let http_get = self
            .lua
            .create_function(|lua, (url, headers_val): (String, mlua::Value)| {
                let headers: Option<HashMap<String, String>> = if headers_val.is_table() {
                    lua.from_value(headers_val).ok()
                } else {
                    None
                };

                let fetch = async {
                    let mut builder = reqwest::Client::builder()
                        .user_agent("craft/1.0")
                        .build()
                        .map_err(|e| format!("{}", e))?
                        .get(&url);
                    if let Some(hdrs) = headers {
                        for (k, v) in hdrs {
                            builder = builder.header(&k, &v);
                        }
                    }
                    let res = builder.send().await.map_err(|e| format!("{}", e))?;
                    let status = res.status().as_u16();
                    let body = res.text().await.map_err(|e| format!("{}", e))?;
                    Ok::<_, String>((status, body))
                };

                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| handle.block_on(fetch))
                } else {
                    match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt.block_on(fetch),
                        Err(e) => Err(e.to_string()),
                    }
                };

                let res_tbl = lua.create_table()?;
                match result {
                    Ok((status, body)) => {
                        res_tbl.set("status", status)?;
                        res_tbl.set("ok", (200..300).contains(&status))?;
                        res_tbl.set("body", body)?;
                    }
                    Err(e) => {
                        res_tbl.set("status", 0)?;
                        res_tbl.set("ok", false)?;
                        res_tbl.set("error", e)?;
                        res_tbl.set("body", "")?;
                    }
                }
                Ok(res_tbl)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        http_table
            .set("get", http_get)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let http_post = self
            .lua
            .create_function(|lua, (url, body, headers_val): (String, String, mlua::Value)| {
                let headers: Option<HashMap<String, String>> = if headers_val.is_table() {
                    lua.from_value(headers_val).ok()
                } else {
                    None
                };

                let fetch = async {
                    let mut builder = reqwest::Client::builder()
                        .user_agent("craft/1.0")
                        .build()
                        .map_err(|e| format!("{}", e))?
                        .post(&url)
                        .body(body);
                    if let Some(hdrs) = headers {
                        for (k, v) in hdrs {
                            builder = builder.header(&k, &v);
                        }
                    }
                    let res = builder.send().await.map_err(|e| format!("{}", e))?;
                    let status = res.status().as_u16();
                    let res_body = res.text().await.map_err(|e| format!("{}", e))?;
                    Ok::<_, String>((status, res_body))
                };

                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| handle.block_on(fetch))
                } else {
                    match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt.block_on(fetch),
                        Err(e) => Err(e.to_string()),
                    }
                };

                let res_tbl = lua.create_table()?;
                match result {
                    Ok((status, res_body)) => {
                        res_tbl.set("status", status)?;
                        res_tbl.set("ok", (200..300).contains(&status))?;
                        res_tbl.set("body", res_body)?;
                    }
                    Err(e) => {
                        res_tbl.set("status", 0)?;
                        res_tbl.set("ok", false)?;
                        res_tbl.set("error", e)?;
                        res_tbl.set("body", "")?;
                    }
                }
                Ok(res_tbl)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        http_table
            .set("post", http_post)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("http", http_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // craft.servers table
        let servers_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_list = effective_paths.clone();
        let servers_list = self
            .lua
            .create_function(move |lua, ()| {
                let res_tbl = lua.create_table()?;
                if let Some(ref p) = ep_list {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        for (idx, s) in reg.servers.iter().enumerate() {
                            let s_tbl = lua.create_table()?;
                            s_tbl.set("name", s.name.clone())?;
                            s_tbl.set("path", s.path.to_string_lossy().to_string())?;
                            s_tbl.set("software", s.software.clone())?;
                            s_tbl.set("version", s.version.clone())?;
                            s_tbl.set("port", s.port)?;
                            s_tbl.set("auto", s.auto)?;
                            let is_running = craft_core::is_server_locked(&s.path);
                            s_tbl.set("running", is_running)?;
                            if is_running {
                                if let Some(pid) = craft_core::read_pid_file(&s.path.join("server.pid")) {
                                    s_tbl.set("pid", pid)?;
                                }
                            }
                            res_tbl.set(idx + 1, s_tbl)?;
                        }
                    }
                }
                Ok(res_tbl)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        servers_table
            .set("list", servers_list)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_get = effective_paths.clone();
        let servers_get = self
            .lua
            .create_function(move |lua, name: String| {
                if let Some(ref p) = ep_get {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        if let Some(s) = reg.find_by_name(&name) {
                            let s_tbl = lua.create_table()?;
                            s_tbl.set("name", s.name.clone())?;
                            s_tbl.set("path", s.path.to_string_lossy().to_string())?;
                            s_tbl.set("software", s.software.clone())?;
                            s_tbl.set("version", s.version.clone())?;
                            s_tbl.set("port", s.port)?;
                            s_tbl.set("auto", s.auto)?;
                            let is_running = craft_core::is_server_locked(&s.path);
                            s_tbl.set("running", is_running)?;
                            if is_running {
                                if let Some(pid) = craft_core::read_pid_file(&s.path.join("server.pid")) {
                                    s_tbl.set("pid", pid)?;
                                }
                            }
                            return Ok(Some(s_tbl));
                        }
                    }
                }
                Ok(None)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        servers_table
            .set("get", servers_get)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_start = effective_paths.clone();
        let servers_start = self
            .lua
            .create_function(move |_lua, name: String| {
                if let Some(ref p) = ep_start {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        if let Some(s) = reg.find_by_name(&name) {
                            if craft_core::is_server_locked(&s.path) {
                                return Err(mlua::Error::RuntimeError(format!(
                                    "Server '{}' is already running",
                                    name
                                )));
                            }
                            let script = if cfg!(windows) {
                                s.path.join("start.bat")
                            } else {
                                s.path.join("start.sh")
                            };
                            if !script.exists() {
                                return Err(mlua::Error::RuntimeError(format!(
                                    "Start script not found at '{}'",
                                    script.display()
                                )));
                            }
                            let mut cmd = if cfg!(windows) {
                                let mut c = Command::new("cmd.exe");
                                c.arg("/c").arg(&script);
                                c
                            } else {
                                let mut c = Command::new("sh");
                                c.arg(&script);
                                c
                            };
                            cmd.current_dir(&s.path);
                            match cmd.spawn() {
                                Ok(child) => return Ok(child.id()),
                                Err(e) => {
                                    return Err(mlua::Error::RuntimeError(format!(
                                        "Failed to spawn server process: {}",
                                        e
                                    )))
                                }
                            }
                        }
                    }
                }
                Err(mlua::Error::RuntimeError(format!("Server '{}' not found", name)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        servers_table
            .set("start", servers_start)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_stop = effective_paths.clone();
        let servers_stop = self
            .lua
            .create_function(move |_lua, name: String| {
                if let Some(ref p) = ep_stop {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        if let Some(s) = reg.find_by_name(&name) {
                            if let Some(pid) = craft_core::read_pid_file(&s.path.join("server.pid")) {
                                #[cfg(unix)]
                                {
                                    let _ = Command::new("kill")
                                        .args(["-15", &pid.to_string()])
                                        .output();
                                }
                                #[cfg(windows)]
                                {
                                    let _ = Command::new("taskkill")
                                        .args(["/PID", &pid.to_string()])
                                        .output();
                                }
                                return Ok(true);
                            }
                            return Ok(false);
                        }
                    }
                }
                Err(mlua::Error::RuntimeError(format!("Server '{}' not found", name)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        servers_table
            .set("stop", servers_stop)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_cmd = effective_paths.clone();
        let servers_cmd = self
            .lua
            .create_function(move |_lua, (name, cmd_text): (String, String)| {
                if let Some(ref p) = ep_cmd {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        if let Some(s) = reg.find_by_name(&name) {
                            let cmd_file = s.path.join("cmd.in");
                            if let Ok(mut f) = fs::OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(&cmd_file)
                            {
                                let _ = writeln!(f, "{}", cmd_text);
                                return Ok(true);
                            }
                            return Ok(false);
                        }
                    }
                }
                Err(mlua::Error::RuntimeError(format!("Server '{}' not found", name)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        servers_table
            .set("command", servers_cmd)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("servers", servers_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // craft.backup table
        let backup_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_bak = effective_paths.clone();
        let backup_list = self
            .lua
            .create_function(move |lua, name: String| {
                let res_tbl = lua.create_table()?;
                if let Some(ref p) = ep_bak {
                    let bdir = p.backups_dir.join(&name);
                    if bdir.is_dir() {
                        if let Ok(entries) = fs::read_dir(&bdir) {
                            let mut idx = 1;
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_file() {
                                    let fname = entry.file_name().to_string_lossy().to_string();
                                    if fname.ends_with(".tar.gz")
                                        || fname.ends_with(".tar.zst")
                                        || fname.ends_with(".zip")
                                    {
                                        let meta = entry.metadata().ok();
                                        let b_tbl = lua.create_table()?;
                                        b_tbl.set("filename", fname)?;
                                        b_tbl.set("path", path.to_string_lossy().to_string())?;
                                        b_tbl.set(
                                            "size_bytes",
                                            meta.as_ref().map(|m| m.len()).unwrap_or(0),
                                        )?;
                                        let modified = meta
                                            .and_then(|m| m.modified().ok())
                                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        b_tbl.set("modified", modified)?;
                                        res_tbl.set(idx, b_tbl)?;
                                        idx += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(res_tbl)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        backup_table
            .set("list", backup_list)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_bak_create = effective_paths.clone();
        let backup_create = self
            .lua
            .create_function(move |_lua, (name, _fmt): (String, mlua::Value)| {
                if let Some(ref p) = ep_bak_create {
                    if let Ok(reg) = ServersRegistry::load(p) {
                        if let Some(s) = reg.find_by_name(&name) {
                            let bdir = p.backups_dir.join(&name);
                            let _ = fs::create_dir_all(&bdir);
                            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                            let out_file = bdir.join(format!("{}-{}.tar.zst", name, ts));

                            let tar_file = fs::File::create(&out_file).map_err(|e| {
                                mlua::Error::RuntimeError(format!("Failed to create backup: {}", e))
                            })?;
                            let mut enc = zstd::stream::Encoder::new(tar_file, 3).map_err(|e| {
                                mlua::Error::RuntimeError(format!("Zstd encoder error: {}", e))
                            })?;
                            {
                                let mut tar = tar::Builder::new(&mut enc);
                                let _ = tar.append_dir_all(".", &s.path);
                                let _ = tar.finish();
                            }
                            let _ = enc.finish();

                            return Ok(out_file.to_string_lossy().to_string());
                        }
                    }
                }
                Err(mlua::Error::RuntimeError(format!("Server '{}' not found", name)))
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        backup_table
            .set("create", backup_create)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("backup", backup_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // craft.net table
        let net_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let net_ping = self
            .lua
            .create_function(|lua, (host, port, edition_val): (String, u16, mlua::Value)| {
                let edition: Option<String> = if let mlua::Value::String(s) = edition_val {
                    s.to_str().ok().map(|st| st.to_string())
                } else {
                    None
                };

                let kind = match edition.as_deref() {
                    Some("bedrock") => Some(craft_core::QueryProtocolKind::MinecraftBedrockRakNet),
                    Some("a2s") => Some(craft_core::QueryProtocolKind::ValveA2S),
                    _ => Some(craft_core::QueryProtocolKind::MinecraftJavaSlp),
                };

                let ping_fut = async { craft_net::ping_server_auto(&host, port, kind).await };

                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| handle.block_on(ping_fut))
                } else {
                    match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt.block_on(ping_fut),
                        Err(e) => Err(craft_core::CraftError::Other(e.to_string())),
                    }
                };

                let res_tbl = lua.create_table()?;
                match result {
                    Ok(craft_net::UniversalPingStatus::MinecraftJava(slp)) => {
                        res_tbl.set("online", true)?;
                        res_tbl.set("latency_ms", slp.latency_ms)?;
                        res_tbl.set("players_online", slp.online_players)?;
                        res_tbl.set("players_max", slp.max_players)?;
                        res_tbl.set("version", slp.version_name)?;
                        res_tbl.set("motd", slp.motd)?;
                    }
                    Ok(craft_net::UniversalPingStatus::MinecraftBedrock(rak)) => {
                        res_tbl.set("online", true)?;
                        res_tbl.set("latency_ms", rak.latency_ms)?;
                        res_tbl.set("players_online", rak.online_players)?;
                        res_tbl.set("players_max", rak.max_players)?;
                        res_tbl.set("version", rak.version)?;
                        res_tbl.set("motd", rak.server_name)?;
                    }
                    Ok(craft_net::UniversalPingStatus::ValveA2S(a2s)) => {
                        res_tbl.set("online", true)?;
                        res_tbl.set("latency_ms", a2s.latency_ms as u64)?;
                        res_tbl.set("players_online", a2s.online_players as u64)?;
                        res_tbl.set("players_max", a2s.max_players as u64)?;
                        res_tbl.set("version", a2s.game_name)?;
                        res_tbl.set("motd", a2s.server_name)?;
                    }
                    Ok(craft_net::UniversalPingStatus::PortProbe { latency_ms, .. }) => {
                        res_tbl.set("online", true)?;
                        res_tbl.set("latency_ms", latency_ms as u64)?;
                        res_tbl.set("players_online", 0)?;
                        res_tbl.set("players_max", 0)?;
                        res_tbl.set("version", "raw_port")?;
                        res_tbl.set("motd", "")?;
                    }
                    Err(e) => {
                        res_tbl.set("online", false)?;
                        res_tbl.set("error", e.to_string())?;
                    }
                }
                Ok(res_tbl)
            })
            .map_err(|e| CraftError::Other(e.to_string()))?;
        net_table
            .set("ping", net_ping)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("net", net_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // craft.audit table
        let audit_table = self
            .lua
            .create_table()
            .map_err(|e| CraftError::Other(e.to_string()))?;

        let ep_audit = effective_paths.clone();
        let audit_log = self
            .lua
            .create_function(
                move |_lua, (action, actor, details_val): (String, String, mlua::Value)| {
                    if let Some(ref p) = ep_audit {
                        let details: Option<String> = if let mlua::Value::String(s) = details_val {
                            s.to_str().ok().map(|st| st.to_string())
                        } else {
                            None
                        };
                        let _ = craft_core::AuditLedger::append(
                            p,
                            "lua-script",
                            &actor,
                            &action,
                            None,
                            None,
                            "SUCCESS",
                            details,
                            craft_core::DEFAULT_AUDIT_SECRET,
                        );
                        return Ok(true);
                    }
                    Ok(false)
                },
            )
            .map_err(|e| CraftError::Other(e.to_string()))?;
        audit_table
            .set("log", audit_log)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        craft_table
            .set("audit", audit_table)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        // Contextual craft.server table
        if let Some(s_dir) = server_dir {
            let server_tbl = self
                .lua
                .create_table()
                .map_err(|e| CraftError::Other(e.to_string()))?;

            if let Some(cfg) = config {
                server_tbl
                    .set("name", cfg.server.name.as_str())
                    .map_err(|e| CraftError::Other(e.to_string()))?;
                server_tbl
                    .set("path", s_dir.to_string_lossy().to_string())
                    .map_err(|e| CraftError::Other(e.to_string()))?;
                server_tbl
                    .set("port", cfg.network.port)
                    .map_err(|e| CraftError::Other(e.to_string()))?;
                server_tbl
                    .set("game", "custom")
                    .map_err(|e| CraftError::Other(e.to_string()))?;
                server_tbl
                    .set("runtime_type", cfg.server.runtime_type.as_str())
                    .map_err(|e| CraftError::Other(e.to_string()))?;
            } else {
                let name = s_dir
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("server");
                server_tbl
                    .set("name", name)
                    .map_err(|e| CraftError::Other(e.to_string()))?;
                server_tbl
                    .set("path", s_dir.to_string_lossy().to_string())
                    .map_err(|e| CraftError::Other(e.to_string()))?;
            }

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

    pub fn run_file_with_timeout(
        &self,
        script_path: &Path,
        args: &[String],
        timeout_secs: u64,
    ) -> Result<()> {
        let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs.max(1));
        let _ = self.lua
            .set_hook(mlua::HookTriggers::default().every_nth_instruction(5000), move |_, _| {
                if Instant::now() >= deadline {
                    Err(mlua::Error::RuntimeError(format!(
                        "Script execution timed out after {}s",
                        timeout_secs
                    )))
                } else {
                    Ok(mlua::VmState::Continue)
                }
            });
        let res = self.run_file(script_path, args);
        self.lua.remove_hook();
        res
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
