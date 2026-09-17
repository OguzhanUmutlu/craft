use crate::ring_buffer::RingBuffer;
use craft_core::{
    auto_heal_server_file, auto_heal_server_jar, CraftError, CraftPaths, Result, ServerLockGuard,
    ServersRegistry,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

struct ActiveServer {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    log_broadcaster: broadcast::Sender<String>,
    _lock: Arc<ServerLockGuard>,
}

#[derive(Clone)]
pub struct Supervisor {
    paths: CraftPaths,
    servers: Arc<Mutex<HashMap<PathBuf, ActiveServer>>>,
}

impl Supervisor {
    pub fn new(paths: CraftPaths) -> Self {
        Self {
            paths,
            servers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn auto_start_servers(&self) {
        if let Ok(registry) = ServersRegistry::load(&self.paths) {
            for server in registry.servers {
                if server.auto && server.path.exists() {
                    info!("Auto-running server: {}", server.path.display());
                    let sup = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = sup.start_server(&server.path).await {
                            error!("Failed to auto-start {}: {}", server.path.display(), e);
                        }
                    });
                }
            }
        }
    }

    pub async fn is_running(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut servers = self.servers.lock().await;

        let child_arc = servers.get(&canonical).map(|active| active.child.clone());

        if let Some(child) = child_arc {
            let mut c = child.lock().await;
            match c.try_wait() {
                Ok(None) => true,
                _ => {
                    servers.remove(&canonical);
                    false
                }
            }
        } else {
            craft_core::is_server_locked(&canonical)
        }
    }

    pub async fn get_running_paths(&self) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let mut servers = self.servers.lock().await;
        let mut to_remove = Vec::new();

        for (path, active) in servers.iter() {
            let mut child = active.child.lock().await;
            match child.try_wait() {
                Ok(None) => {
                    result.push(path.clone());
                }
                _ => {
                    to_remove.push(path.clone());
                }
            }
        }

        for path in to_remove {
            servers.remove(&path);
        }

        // Also check registry for servers running externally (e.g. foreground)
        if let Ok(registry) = ServersRegistry::load(&self.paths) {
            for server in registry.servers {
                let canonical = server
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| server.path.clone());
                if !result.contains(&canonical) && craft_core::is_server_locked(&canonical) {
                    result.push(canonical);
                }
            }
        }

        result
    }

    pub async fn start_server(&self, server_path: &Path) -> Result<()> {
        let canonical = server_path
            .canonicalize()
            .unwrap_or_else(|_| server_path.to_path_buf());

        if self.is_running(&canonical).await {
            let pid_info = craft_core::get_server_running_pid(&canonical)
                .map(|p| format!(" (PID: {})", p))
                .unwrap_or_default();
            return Err(CraftError::Other(format!(
                "Server at '{}' is already running{}. Only one process can run the server at a time.",
                server_path.display(),
                pid_info
            )));
        }

        let lock_guard = ServerLockGuard::acquire(&canonical)?;

        let script = if cfg!(windows) {
            canonical.join("start.cmd")
        } else {
            canonical.join("start.sh")
        };

        // Self-healing: ensure server jar / binary is in place
        let server_entry = ServersRegistry::load(&self.paths)
            .ok()
            .and_then(|r| r.find_by_path(&canonical).cloned());

        let expected_file = server_entry
            .as_ref()
            .and_then(|s| craft_providers::find_software(&s.software))
            .map(|sw| sw.default_server_file())
            .unwrap_or("server.jar");

        if let Some(source) = auto_heal_server_file(&canonical, expected_file) {
            info!(
                "Self-healing: Restored {} from '{}' in '{}'",
                expected_file,
                source,
                canonical.display()
            );
        }
        if expected_file != "server.jar" && !canonical.join(expected_file).exists() {
            if let Some(source) = auto_heal_server_jar(&canonical) {
                info!(
                    "Self-healing: Restored server.jar from '{}' in '{}'",
                    source,
                    canonical.display()
                );
            }
        }

        if !script.exists() {
            return Err(CraftError::Other(format!(
                "Start script not found at '{}'",
                script.display()
            )));
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&script) {
                let mut perms = meta.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&script, perms);
                }
            }
        }

        // Delegate pre-start checks (EULA, binary permissions, config validation) to software provider
        if let Some(sw) = server_entry
            .as_ref()
            .and_then(|s| craft_providers::find_software(&s.software))
        {
            sw.pre_start_check(&canonical)?;
        } else {
            // Fallback for custom or unrecognised Minecraft servers
            let eula_path = canonical.join("eula.txt");
            if eula_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&eula_path) {
                    if content.contains("eula=false") {
                        return Err(CraftError::Other(
                            "EULA has not been accepted for this server yet. Run 'craft run --here' to view and agree to the EULA.".to_string()
                        ));
                    }
                }
            }
        }

        // Custom server pre-start lifecycle hook
        let custom_config = craft_scripting::CustomServerConfig::load_from_dir(&canonical)
            .ok()
            .flatten();
        if let Some(ref cfg) = custom_config {
            if let Err(e) = craft_scripting::LuaEngine::run_pre_start(&canonical, cfg) {
                warn!("Custom server on_pre_start hook error: {}", e);
            }
        }

        info!("Starting server process in '{}'", canonical.display());

        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd.exe");
            c.arg("/c").arg(&script);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg(&script);
            c
        };

        cmd.current_dir(&canonical)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(not(target_os = "windows"))]
        {
            cmd.process_group(0);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| CraftError::Process(format!("Failed to spawn server process: {}", e)))?;

        if let Some(pid) = child.id() {
            let _ = lock_guard.record_pid(pid);
            if let Some(ref cfg) = custom_config {
                if let Err(e) = craft_scripting::LuaEngine::run_post_start(&canonical, cfg, pid) {
                    warn!("Custom server on_post_start hook error: {}", e);
                }
            }
        }

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(50000)));
        let (log_broadcaster, _) = broadcast::channel(1000);

        let logs_dir = canonical.join("logs");
        let _ = std::fs::create_dir_all(&logs_dir);
        let console_log_path = logs_dir.join("console.log");

        // Spawn stdout reader
        if let Some(out) = stdout {
            let rb = ring_buffer.clone();
            let bc = log_broadcaster.clone();
            let log_file = console_log_path.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim_end_matches('\r');
                    if trimmed.is_empty() {
                        continue;
                    }
                    let text = format!("{}\n", trimmed);
                    {
                        let mut b = rb.lock().await;
                        b.push(text.clone());
                    }
                    let _ = bc.send(text);
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_file)
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "{}", trimmed);
                    }
                }
            });
        }

        // Spawn stderr reader
        if let Some(err) = stderr {
            let rb = ring_buffer.clone();
            let bc = log_broadcaster.clone();
            let log_file = console_log_path.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim_end_matches('\r');
                    if trimmed.is_empty() {
                        continue;
                    }
                    let text = format!("{}\n", trimmed);
                    {
                        let mut b = rb.lock().await;
                        b.push(text.clone());
                    }
                    let _ = bc.send(text);
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_file)
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "{}", trimmed);
                    }
                }
            });
        }

        let active = ActiveServer {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            ring_buffer,
            log_broadcaster,
            _lock: Arc::new(lock_guard),
        };

        let mut servers = self.servers.lock().await;
        servers.insert(canonical, active);

        Ok(())
    }

    pub async fn stop_server(&self, server_path: &Path, force: bool) -> Result<()> {
        let canonical = server_path
            .canonicalize()
            .unwrap_or_else(|_| server_path.to_path_buf());

        let custom_config = craft_scripting::CustomServerConfig::load_from_dir(&canonical)
            .ok()
            .flatten();

        let (child, stdin) = {
            let servers = self.servers.lock().await;
            if let Some(active) = servers.get(&canonical) {
                (Some(active.child.clone()), Some(active.stdin.clone()))
            } else {
                (None, None)
            }
        };

        if let (Some(child), Some(stdin)) = (child, stdin) {
            let child_pid = {
                let c = child.lock().await;
                c.id()
            };

            if let (Some(pid), Some(ref cfg)) = (child_pid, &custom_config) {
                if let Err(e) = craft_scripting::LuaEngine::run_pre_stop(&canonical, cfg, pid) {
                    warn!("Custom server on_pre_stop hook error: {}", e);
                }
            }

            if !force {
                let stop_method = custom_config
                    .as_ref()
                    .map(|c| c.lifecycle.stop_method.as_str())
                    .unwrap_or("stdin");
                let default_stop_cmd = "stop\n".to_string();
                let stop_cmd = custom_config
                    .as_ref()
                    .and_then(|c| c.lifecycle.stop_command.as_deref())
                    .unwrap_or(&default_stop_cmd);
                let timeout_secs = custom_config
                    .as_ref()
                    .map(|c| c.lifecycle.stop_timeout_seconds)
                    .unwrap_or(10);

                match stop_method {
                    "sigint" => {
                        if let Some(pid) = child_pid {
                            #[cfg(not(target_os = "windows"))]
                            {
                                let _ = std::process::Command::new("kill")
                                    .args(["-INT", &format!("-{}", pid)])
                                    .status();
                            }
                            #[cfg(target_os = "windows")]
                            {
                                let _ = std::process::Command::new("taskkill")
                                    .args(["/PID", &pid.to_string()])
                                    .status();
                            }
                        }
                    }
                    "sigterm" => {
                        if let Some(pid) = child_pid {
                            #[cfg(not(target_os = "windows"))]
                            {
                                let _ = std::process::Command::new("kill")
                                    .args(["-TERM", &format!("-{}", pid)])
                                    .status();
                            }
                            #[cfg(target_os = "windows")]
                            {
                                let _ = std::process::Command::new("taskkill")
                                    .args(["/PID", &pid.to_string()])
                                    .status();
                            }
                        }
                    }
                    _ => {
                        // Stdin command
                        if let Some(ref mut input) = *stdin.lock().await {
                            let cmd_to_send = if stop_cmd.ends_with('\n') {
                                stop_cmd.to_string()
                            } else {
                                format!("{}\n", stop_cmd)
                            };
                            let _ = input.write_all(cmd_to_send.as_bytes()).await;
                            let _ = input.flush().await;
                        }
                    }
                }

                // Wait up to timeout_secs for graceful exit
                let intervals = (timeout_secs * 2).max(1);
                for _ in 0..intervals {
                    sleep(Duration::from_millis(500)).await;
                    let mut c = child.lock().await;
                    if let Ok(Some(status)) = c.try_wait() {
                        let code = status.code().unwrap_or(0);
                        let mut servers = self.servers.lock().await;
                        servers.remove(&canonical);
                        if let Some(ref cfg) = custom_config {
                            let _ =
                                craft_scripting::LuaEngine::run_post_stop(&canonical, cfg, code);
                        }
                        return Ok(());
                    }
                }
            }

            // Force kill if graceful stop timed out or force was requested
            warn!("Force killing server '{}'", server_path.display());
            let mut c = child.lock().await;
            if let Some(pid) = c.id() {
                #[cfg(not(target_os = "windows"))]
                {
                    // Kill the entire process group (-pid)
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &format!("-{}", pid)])
                        .status();
                }
                #[cfg(target_os = "windows")]
                {
                    // Force kill the full process tree (/F /T)
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .status();
                }
            }
            let _ = c.kill().await;

            let mut servers = self.servers.lock().await;
            servers.remove(&canonical);

            if let Some(ref cfg) = custom_config {
                let _ = craft_scripting::LuaEngine::run_post_stop(&canonical, cfg, -9);
            }

            Ok(())
        } else if let Some(pid) = craft_core::get_server_running_pid(&canonical) {
            info!(
                "Stopping externally running server at '{}' (PID: {})",
                canonical.display(),
                pid
            );
            craft_core::kill_process(pid, force)?;
            // Wait up to 5 seconds for process to exit
            for _ in 0..10 {
                sleep(Duration::from_millis(500)).await;
                if !craft_core::is_process_running(pid) {
                    return Ok(());
                }
            }
            if !force {
                let _ = craft_core::kill_process(pid, true);
            }
            Ok(())
        } else {
            Err(CraftError::Other(format!(
                "Server '{}' is not running",
                server_path.display()
            )))
        }
    }

    pub async fn send_input(&self, server_path: &Path, input: &str) -> Result<()> {
        let canonical = server_path
            .canonicalize()
            .unwrap_or_else(|_| server_path.to_path_buf());
        let servers = self.servers.lock().await;

        if let Some(active) = servers.get(&canonical) {
            let mut stdin_guard = active.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin
                    .write_all(input.as_bytes())
                    .await
                    .map_err(|e| CraftError::Ipc(format!("Failed to write to stdin: {}", e)))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| CraftError::Ipc(format!("Failed to flush stdin: {}", e)))?;
                return Ok(());
            }
        }

        Err(CraftError::Other(format!(
            "Server '{}' stdin not available or not running",
            server_path.display()
        )))
    }

    pub async fn get_console_stream(
        &self,
        server_path: &Path,
    ) -> Result<(String, broadcast::Receiver<String>)> {
        let canonical = server_path
            .canonicalize()
            .unwrap_or_else(|_| server_path.to_path_buf());
        let servers = self.servers.lock().await;

        if let Some(active) = servers.get(&canonical) {
            let backlog = {
                let rb = active.ring_buffer.lock().await;
                rb.get_backlog()
            };
            let rx = active.log_broadcaster.subscribe();
            Ok((backlog, rx))
        } else {
            Err(CraftError::Other(format!(
                "Server '{}' is not running",
                server_path.display()
            )))
        }
    }
}
