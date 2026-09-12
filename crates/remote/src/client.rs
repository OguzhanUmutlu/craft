use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use craft_core::{CraftError, RemoteHostConfig, Result};
use crate::session::RemoteSession;
use crate::sftp_ops::SftpOps;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerInfo {
    pub name: String,
    pub server_type: String,
    pub version: String,
    pub port: u16,
    pub is_running: bool,
    pub pid: Option<u32>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBackupInfo {
    pub filename: String,
    pub remote_path: String,
    pub size_bytes: u64,
    pub created_at: String,
}

pub struct RemoteCraftClient {
    pub session: RemoteSession,
}

impl RemoteCraftClient {
    pub fn connect(config: &RemoteHostConfig) -> Result<Self> {
        let session = RemoteSession::connect(config)?;
        Ok(Self { session })
    }

    pub fn sftp(&self) -> SftpOps<'_> {
        SftpOps::new(&self.session)
    }


    /// Checks whether the `craft` binary is installed and executable on the remote host
    pub fn is_craft_installed(&self) -> bool {
        if let Ok((code, out, _)) = self.session.exec("craft --version || ~/.local/bin/craft --version || /usr/local/bin/craft --version") {
            code == 0 && out.to_lowercase().contains("craft")
        } else {
            false
        }
    }

    /// Resolves the craft binary invocation path on remote
    fn craft_bin(&self) -> &'static str {
        // Will check standard PATH, falling back to ~/.local/bin/craft
        "PATH=\"$HOME/.local/bin:/usr/local/bin:$PATH\" craft"
    }

    /// Lists servers registered in the remote host's ~/.craft/servers.toml
    pub fn list_servers(&self) -> Result<Vec<RemoteServerInfo>> {
        // Read ~/.craft/servers.toml via SFTP
        let remote_toml_path = Path::new(".craft/servers.toml");
        let content = match self.sftp().read_file_to_string(remote_toml_path) {
            Ok(c) => c,
            Err(_) => {
                // If relative path didn't find it, try running cat
                let (code, stdout, _) = self.session.exec("cat ~/.craft/servers.toml 2>/dev/null")?;
                if code != 0 || stdout.trim().is_empty() {
                    return Ok(Vec::new());
                }
                stdout
            }
        };

        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| CraftError::Other(format!("Failed to parse remote servers.toml: {}", e)))?;

        let mut servers = Vec::new();
        if let Some(servers_arr) = parsed.get("servers").and_then(|s| s.as_array()) {
            for entry in servers_arr {
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                let server_type = entry.get("server_type").and_then(|v| v.as_str()).unwrap_or("custom").to_string();
                let version = entry.get("version").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                let port = entry.get("port").and_then(|v| v.as_integer()).unwrap_or(25565) as u16;
                let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();

                // Check if server is running on remote
                let (is_running, pid) = self.check_server_running(&name, &path);

                servers.push(RemoteServerInfo {
                    name,
                    server_type,
                    version,
                    port,
                    is_running,
                    pid,
                    path,
                });
            }
        }

        Ok(servers)
    }

    /// Checks if a remote server is running by inspecting PID file or process
    pub fn check_server_running(&self, server_name: &str, server_path: &str) -> (bool, Option<u32>) {
        let cmd = format!(
            "if [ -f \"{}/.server.pid\" ]; then cat \"{}/.server.pid\"; fi",
            server_path, server_path
        );

        if let Ok((code, out, _)) = self.session.exec(&cmd) {
            if code == 0 && !out.trim().is_empty() {
                if let Ok(pid) = out.trim().parse::<u32>() {
                    // Check if PID is alive: kill -0 <pid>
                    if let Ok((kcode, _, _)) = self.session.exec(&format!("kill -0 {} 2>/dev/null", pid)) {
                        if kcode == 0 {
                            return (true, Some(pid));
                        }
                    }
                }
            }
        }

        // Fallback: check craft status
        let status_cmd = format!("{} status {} 2>/dev/null", self.craft_bin(), server_name);
        if let Ok((code, out, _)) = self.session.exec(&status_cmd) {
            if code == 0 && out.to_lowercase().contains("running") {
                return (true, None);
            }
        }

        (false, None)
    }

    /// Starts a remote server in daemon mode
    pub fn start_server(&self, server_name: &str) -> Result<()> {
        let cmd = format!("{} start {} --daemon", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to start remote server: {}", err.trim())));
        }
        Ok(())
    }

    /// Stops a remote server
    pub fn stop_server(&self, server_name: &str) -> Result<()> {
        let cmd = format!("{} stop {}", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to stop remote server: {}", err.trim())));
        }
        Ok(())
    }

    /// Restarts a remote server
    pub fn restart_server(&self, server_name: &str) -> Result<()> {
        let cmd = format!("{} restart {}", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to restart remote server: {}", err.trim())));
        }
        Ok(())
    }

    /// Lists backups available on the remote server
    pub fn list_backups(&self, server_name: &str) -> Result<Vec<RemoteBackupInfo>> {
        let remote_backup_dir = format!(".craft/backups/{}", server_name);
        let entries = match self.sftp().list_dir(Path::new(&remote_backup_dir)) {
            Ok(e) => e,
            Err(_) => {
                // Fallback using remote find / ls
                let cmd = format!("ls -lh ~/.craft/backups/{}/ 2>/dev/null", server_name);
                let (code, out, _) = self.session.exec(&cmd)?;
                if code != 0 {
                    return Ok(Vec::new());
                }
                let mut list = Vec::new();
                for line in out.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 9 && parts.last().map(|f| f.ends_with(".tar.gz")).unwrap_or(false) {
                        let fname = parts.last().unwrap().to_string();
                        list.push(RemoteBackupInfo {
                            remote_path: format!("~/.craft/backups/{}/{}", server_name, fname),
                            filename: fname,
                            size_bytes: 0,
                            created_at: parts[5..8].join(" "),
                        });
                    }
                }
                return Ok(list);
            }
        };

        let mut backups = Vec::new();
        for (fname, stat) in entries {
            if fname.ends_with(".tar.gz") {
                let size_bytes = stat.size.unwrap_or(0);
                let mtime = stat.mtime.unwrap_or(0) as i64;
                let created_at = chrono::DateTime::from_timestamp(mtime, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());

                backups.push(RemoteBackupInfo {
                    remote_path: format!("{}/{}", remote_backup_dir, fname),
                    filename: fname,
                    size_bytes,
                    created_at,
                });
            }
        }

        backups.sort_by(|a, b| b.filename.cmp(&a.filename));
        Ok(backups)
    }

    /// Restores a backup on remote host.
    /// STRICTLY validates that the remote server is stopped first!
    pub fn restore_backup(&self, server_name: &str, server_path: &str, backup_filename: &str) -> Result<()> {
        let (running, pid) = self.check_server_running(server_name, server_path);
        if running {
            let pid_str = pid.map(|p| format!(" (PID: {})", p)).unwrap_or_default();
            return Err(CraftError::Other(format!(
                "Cannot restore backup: Remote server '{}' is currently running{}. The server MUST be stopped before restoring a backup to prevent world corruption.",
                server_name, pid_str
            )));
        }

        let cmd = format!("{} backup restore {} {}", self.craft_bin(), server_name, backup_filename);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to restore remote backup: {}", err.trim())));
        }

        Ok(())
    }

    /// Downloads a remote backup archive via SFTP into a target local path
    pub fn download_backup(&self, server_name: &str, backup_filename: &str, local_target_dir: &Path) -> Result<PathBuf> {
        let remote_path_rel = Path::new(".craft").join("backups").join(server_name).join(backup_filename);
        let local_dest_path = local_target_dir.join(backup_filename);

        if let Some(parent) = local_dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // SFTP download
        self.sftp().download_file(&remote_path_rel, &local_dest_path)
            .map_err(|e| CraftError::Other(format!("SFTP download failed: {}", e)))?;

        Ok(local_dest_path)
    }

    /// Triggers an immediate backup creation on the remote server
    pub fn create_backup(&self, server_name: &str, world_only: bool) -> Result<()> {
        let suffix = if world_only { " --world-only" } else { "" };
        let cmd = format!("{} backup create {}{}", self.craft_bin(), server_name, suffix);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to create remote backup: {}", err.trim())));
        }
        Ok(())
    }

    /// Creates a new server on the remote host using craft create
    pub fn create_server(&self, name: &str, software: &str, version: &str, port: u16) -> Result<()> {
        let cmd = format!(
            "{} create --name \"{}\" --software \"{}\" --version \"{}\" --port {} --non-interactive",
            self.craft_bin(), name, software, version, port
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to create remote server: {}", err.trim())));
        }
        Ok(())
    }

    /// Checks the status of the remote service daemon
    pub fn daemon_status(&self) -> Result<bool> {
        let cmd = format!("{} daemon status 2>/dev/null", self.craft_bin());
        if let Ok((code, stdout, _)) = self.session.exec(&cmd) {
            Ok(code == 0 && stdout.to_lowercase().contains("online"))
        } else {
            Ok(false)
        }
    }

    /// Starts the remote service daemon
    pub fn daemon_start(&self) -> Result<()> {
        let cmd = format!("{} daemon start", self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to start remote daemon: {}", err.trim())));
        }
        Ok(())
    }

    /// Stops the remote service daemon
    pub fn daemon_stop(&self) -> Result<()> {
        let cmd = format!("{} daemon stop", self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to stop remote daemon: {}", err.trim())));
        }
        Ok(())
    }

    /// Restarts the remote service daemon
    pub fn daemon_restart(&self) -> Result<()> {
        let cmd = format!("{} daemon restart", self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to restart remote daemon: {}", err.trim())));
        }
        Ok(())
    }

    /// Cleans the downloaded asset cache on the remote host
    pub fn clean_cache(&self) -> Result<()> {
        let cmd = format!("{} cache clean", self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to clean remote cache: {}", err.trim())));
        }
        Ok(())
    }

    /// Moves a remote backup archive into ~/.craft/trash/
    pub fn trash_backup(&self, server_name: &str, backup_filename: &str) -> Result<()> {
        let cmd = format!(
            "mkdir -p ~/.craft/trash && mv ~/.craft/backups/{}/{} ~/.craft/trash/ 2>&1",
            server_name, backup_filename
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to trash remote backup: {}", err.trim())));
        }
        Ok(())
    }

    /// Lists trashed backup archives on the remote host
    pub fn list_trash(&self) -> Result<Vec<RemoteBackupInfo>> {
        let cmd = "ls -lh ~/.craft/trash/ 2>/dev/null";
        let (code, out, _) = self.session.exec(cmd)?;
        if code != 0 {
            return Ok(Vec::new());
        }

        let mut list = Vec::new();
        for line in out.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                let fname = parts[8..].join(" ");
                if fname.ends_with(".tar.gz") || fname.ends_with(".bak") {
                    list.push(RemoteBackupInfo {
                        filename: fname.clone(),
                        remote_path: format!("~/.craft/trash/{}", fname),
                        size_bytes: 0,
                        created_at: parts[5..8].join(" "),
                    });
                }
            }
        }
        Ok(list)
    }

    /// Restores an archive from remote trash into a server's backups folder
    pub fn restore_trash(&self, filename: &str, target_server: &str) -> Result<()> {
        let cmd = format!(
            "mkdir -p ~/.craft/backups/{} && mv ~/.craft/trash/\"{}\" ~/.craft/backups/{}/",
            target_server, filename, target_server
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to restore remote trash: {}", err.trim())));
        }
        Ok(())
    }

    /// Permanently deletes an archive from remote trash
    pub fn delete_trash_item(&self, filename: &str) -> Result<()> {
        let cmd = format!("rm -f ~/.craft/trash/\"{}\"", filename);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to delete remote trash item: {}", err.trim())));
        }
        Ok(())
    }

    /// Empties all items in the remote trash directory
    pub fn empty_trash(&self) -> Result<()> {
        let cmd = "rm -rf ~/.craft/trash/*";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Failed to empty remote trash: {}", err.trim())));
        }
        Ok(())
    }
}
