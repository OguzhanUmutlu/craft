use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use crate::ring_buffer::RingBuffer;

struct ActiveServer {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    log_broadcaster: broadcast::Sender<String>,
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
            false
        }
    }

    pub async fn get_running_paths(&self) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let servers = self.servers.lock().await;
        for (path, active) in servers.iter() {
            let mut child = active.child.lock().await;
            if let Ok(None) = child.try_wait() {
                result.push(path.clone());
            }
        }
        result
    }

    pub async fn start_server(&self, server_path: &Path) -> Result<()> {
        let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.to_path_buf());

        if self.is_running(&canonical).await {
            return Err(CraftError::Other(format!(
                "Server at '{}' is already running",
                server_path.display()
            )));
        }

        let script = if cfg!(windows) {
            canonical.join("start.cmd")
        } else {
            canonical.join("start.sh")
        };

        if !script.exists() {
            return Err(CraftError::Other(format!(
                "Start script not found at '{}'",
                script.display()
            )));
        }

        // Check EULA
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

        let mut child = cmd.spawn()
            .map_err(|e| CraftError::Process(format!("Failed to spawn server process: {}", e)))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(50000)));
        let (log_broadcaster, _) = broadcast::channel(1000);

        // Spawn stdout reader
        if let Some(out) = stdout {
            let rb = ring_buffer.clone();
            let bc = log_broadcaster.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let text = format!("{}\n", line);
                    {
                        let mut b = rb.lock().await;
                        b.push(text.clone());
                    }
                    let _ = bc.send(text);
                }
            });
        }

        // Spawn stderr reader
        if let Some(err) = stderr {
            let rb = ring_buffer.clone();
            let bc = log_broadcaster.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let text = format!("{}\n", line);
                    {
                        let mut b = rb.lock().await;
                        b.push(text.clone());
                    }
                    let _ = bc.send(text);
                }
            });
        }

        let active = ActiveServer {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            ring_buffer,
            log_broadcaster,
        };

        let mut servers = self.servers.lock().await;
        servers.insert(canonical, active);

        Ok(())
    }

    pub async fn stop_server(&self, server_path: &Path, force: bool) -> Result<()> {
        let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.to_path_buf());

        let (child, stdin) = {
            let servers = self.servers.lock().await;
            if let Some(active) = servers.get(&canonical) {
                (active.child.clone(), active.stdin.clone())
            } else {
                return Err(CraftError::Other(format!(
                    "Server '{}' is not running",
                    server_path.display()
                )));
            }
        };

        if !force {
            // Attempt graceful stop command via stdin
            if let Some(ref mut input) = *stdin.lock().await {
                let _ = input.write_all(b"stop\n").await;
                let _ = input.flush().await;
            }

            // Wait up to 10 seconds for graceful exit
            for _ in 0..20 {
                sleep(Duration::from_millis(500)).await;
                let mut c = child.lock().await;
                if let Ok(Some(_)) = c.try_wait() {
                    let mut servers = self.servers.lock().await;
                    servers.remove(&canonical);
                    return Ok(());
                }
            }
        }

        // Force kill if graceful stop timed out or force was requested
        warn!("Force killing server '{}'", server_path.display());
        let mut c = child.lock().await;
        let _ = c.kill().await;

        let mut servers = self.servers.lock().await;
        servers.remove(&canonical);

        Ok(())
    }

    pub async fn send_input(&self, server_path: &Path, input: &str) -> Result<()> {
        let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.to_path_buf());
        let servers = self.servers.lock().await;

        if let Some(active) = servers.get(&canonical) {
            let mut stdin_guard = active.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin.write_all(input.as_bytes()).await
                    .map_err(|e| CraftError::Ipc(format!("Failed to write to stdin: {}", e)))?;
                stdin.flush().await
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
        let canonical = server_path.canonicalize().unwrap_or_else(|_| server_path.to_path_buf());
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
