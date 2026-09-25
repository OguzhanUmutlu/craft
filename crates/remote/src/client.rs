use crate::session::RemoteSession;
use crate::sftp_ops::SftpOps;
use craft_core::{CraftError, RemoteHostConfig, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
        if let Ok((code, out, _)) = self.session.exec(
            "craft --version || ~/.local/bin/craft --version || /usr/local/bin/craft --version",
        ) {
            code == 0 && out.to_lowercase().contains("craft")
        } else {
            false
        }
    }

    /// Extracts version string from craft --version stdout
    pub fn parse_version_str(out: &str) -> Option<String> {
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return None;
        }
        let token = trimmed.split_whitespace().last()?;
        let v = token.trim_start_matches('v').trim();
        if !v.is_empty() {
            Some(v.to_string())
        } else {
            None
        }
    }

    /// Retrieves the installed Craft version from the remote host (e.g., "1.0.0")
    pub fn get_craft_version(&self) -> Option<String> {
        if let Ok((code, out, _)) = self.session.exec("~/.local/bin/craft --version 2>/dev/null || /usr/local/bin/craft --version 2>/dev/null || craft --version 2>/dev/null") {
            if code == 0 {
                return Self::parse_version_str(&out);
            }
        }
        None
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
                let (code, stdout, _) =
                    self.session.exec("cat ~/.craft/servers.toml 2>/dev/null")?;
                if code != 0 || stdout.trim().is_empty() {
                    return Ok(Vec::new());
                }
                stdout
            }
        };

        let parsed: toml::Value = toml::from_str(&content).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote servers.toml: {}", e))
        })?;

        let mut servers = Vec::new();
        if let Some(servers_arr) = parsed.get("servers").and_then(|s| s.as_array()) {
            for entry in servers_arr {
                let name = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let server_type = entry
                    .get("server_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("custom")
                    .to_string();
                let version = entry
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let port = entry
                    .get("port")
                    .and_then(|v| v.as_integer())
                    .unwrap_or(25565) as u16;
                let path = entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

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
    pub fn check_server_running(
        &self,
        server_name: &str,
        server_path: &str,
    ) -> (bool, Option<u32>) {
        let cmd = format!(
            "if [ -f \"{}/.server.pid\" ]; then cat \"{}/.server.pid\"; fi",
            server_path, server_path
        );

        if let Ok((code, out, _)) = self.session.exec(&cmd) {
            if code == 0 && !out.trim().is_empty() {
                if let Ok(pid) = out.trim().parse::<u32>() {
                    // Check if PID is alive: kill -0 <pid>
                    if let Ok((kcode, _, _)) =
                        self.session.exec(&format!("kill -0 {} 2>/dev/null", pid))
                    {
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
        let cmd = format!("{} start {}", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to start remote server: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Stops a remote server
    pub fn stop_server(&self, server_name: &str) -> Result<()> {
        let cmd = format!("{} stop {}", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to stop remote server: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Restarts a remote server
    pub fn restart_server(&self, server_name: &str) -> Result<()> {
        let cmd = format!("{} restart {}", self.craft_bin(), server_name);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to restart remote server: {}",
                err.trim()
            )));
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
                    if parts.len() >= 9
                        && parts
                            .last()
                            .map(|f| f.ends_with(".tar.gz"))
                            .unwrap_or(false)
                    {
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
    pub fn restore_backup(
        &self,
        server_name: &str,
        server_path: &str,
        backup_filename: &str,
    ) -> Result<()> {
        let (running, pid) = self.check_server_running(server_name, server_path);
        if running {
            let pid_str = pid.map(|p| format!(" (PID: {})", p)).unwrap_or_default();
            return Err(CraftError::Other(format!(
                "Cannot restore backup: Remote server '{}' is currently running{}. The server MUST be stopped before restoring a backup to prevent world corruption.",
                server_name, pid_str
            )));
        }

        let cmd = format!(
            "{} backup restore {} {}",
            self.craft_bin(),
            server_name,
            backup_filename
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to restore remote backup: {}",
                err.trim()
            )));
        }

        Ok(())
    }

    /// Downloads a remote backup archive via SFTP into a target local path
    pub fn download_backup(
        &self,
        server_name: &str,
        backup_filename: &str,
        local_target_dir: &Path,
    ) -> Result<PathBuf> {
        let remote_path_rel = Path::new(".craft")
            .join("backups")
            .join(server_name)
            .join(backup_filename);
        let local_dest_path = local_target_dir.join(backup_filename);

        if let Some(parent) = local_dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // SFTP download
        self.sftp()
            .download_file(&remote_path_rel, &local_dest_path)
            .map_err(|e| CraftError::Other(format!("SFTP download failed: {}", e)))?;

        Ok(local_dest_path)
    }

    /// Triggers an immediate backup creation on the remote server
    pub fn create_backup(&self, server_name: &str, world_only: bool) -> Result<()> {
        let suffix = if world_only { " --world-only" } else { "" };
        let cmd = format!(
            "{} backup create {}{}",
            self.craft_bin(),
            server_name,
            suffix
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to create remote backup: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Creates a new server on the remote host using craft new
    pub fn create_server(
        &self,
        name: &str,
        software: &str,
        version: &str,
        port: u16,
    ) -> Result<()> {
        let cmd = format!(
            "{} new \"{}\" \"{}\" \"{}\" --yes --agree-eula --no-start",
            self.craft_bin(),
            name,
            software,
            version
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to create remote server: {}",
                err.trim()
            )));
        }

        // Configure port in server.properties if customized
        let server_dir = format!("~/.craft/servers/{}", name);
        let props_file = format!("{}/server.properties", server_dir);
        let port_script = format!(
            "if [ -f \"{p}\" ]; then \
                if grep -q '^server-port=' \"{p}\"; then \
                    sed -i 's/^server-port=.*/server-port={port}/' \"{p}\"; \
                else \
                    echo \"server-port={port}\" >> \"{p}\"; \
                fi; \
            else \
                echo \"server-port={port}\" > \"{p}\"; \
            fi; \
            if [ -f \"{p}\" ] && grep -q '^server-portv6=' \"{p}\"; then \
                sed -i 's/^server-portv6=.*/server-portv6={port_v6}/' \"{p}\"; \
            fi",
            p = props_file,
            port = port,
            port_v6 = port.saturating_add(1),
        );
        let _ = self.session.exec(&port_script);

        // Also update port in ~/.craft/servers.toml if it exists
        if let Ok((0, stdout, _)) = self.session.exec("cat ~/.craft/servers.toml 2>/dev/null") {
            if !stdout.trim().is_empty() {
                if let Ok(mut parsed) = toml::from_str::<toml::Value>(&stdout) {
                    let mut modified = false;
                    if let Some(servers) = parsed.get_mut("servers").and_then(|s| s.as_array_mut())
                    {
                        for s in servers {
                            if s.get("name").and_then(|v| v.as_str()) == Some(name) {
                                if let Some(tbl) = s.as_table_mut() {
                                    tbl.insert(
                                        "port".to_string(),
                                        toml::Value::Integer(port as i64),
                                    );
                                    modified = true;
                                }
                            }
                        }
                    }
                    if modified {
                        if let Ok(new_toml) = toml::to_string(&parsed) {
                            let write_cmd =
                                format!("cat << 'EOF' > ~/.craft/servers.toml\n{}\nEOF", new_toml);
                            let _ = self.session.exec(&write_cmd);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Checks the status of the remote service daemon
    pub fn daemon_status(&self) -> Result<bool> {
        let cmd = format!(
            "{} service status 2>/dev/null || {} daemon status 2>/dev/null",
            self.craft_bin(),
            self.craft_bin()
        );
        if let Ok((code, stdout, _)) = self.session.exec(&cmd) {
            let s = stdout.to_lowercase();
            Ok(
                code == 0
                    && (s.contains("running") || s.contains("online") || s.contains("active")),
            )
        } else {
            Ok(false)
        }
    }

    /// Starts the remote service daemon
    pub fn daemon_start(&self) -> Result<()> {
        let cmd = format!("systemctl --user start craft.service 2>/dev/null || {} service start 2>/dev/null || {} daemon start", self.craft_bin(), self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to start remote daemon: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Ensures the remote service daemon is running, starting it if currently stopped
    pub fn ensure_daemon_started(&self) -> Result<()> {
        if !self.daemon_status().unwrap_or(false) {
            let _ = self.daemon_start();
        }
        Ok(())
    }

    /// Stops the remote service daemon
    pub fn daemon_stop(&self) -> Result<()> {
        let cmd = format!("systemctl --user stop craft.service 2>/dev/null || {} service stop 2>/dev/null || {} daemon stop", self.craft_bin(), self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to stop remote daemon: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Restarts the remote service daemon
    pub fn daemon_restart(&self) -> Result<()> {
        let cmd = format!("systemctl --user restart craft.service 2>/dev/null || {} service restart 2>/dev/null || {} daemon restart", self.craft_bin(), self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to restart remote daemon: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Completely uninstalls Craft CLI, daemon, and background services from the remote host
    pub fn uninstall_craft(&self) -> Result<()> {
        // Stop & remove systemd user unit
        let _ = self
            .session
            .exec("systemctl --user stop craft.service 2>/dev/null || true");
        let _ = self
            .session
            .exec("systemctl --user disable craft.service 2>/dev/null || true");
        let _ = self
            .session
            .exec("rm -f ~/.config/systemd/user/craft.service 2>/dev/null || true");
        let _ = self
            .session
            .exec("systemctl --user daemon-reload 2>/dev/null || true");

        // Stop daemon directly
        let _ = self.session.exec(&format!(
            "{} service stop 2>/dev/null || true",
            self.craft_bin()
        ));
        let _ = self
            .session
            .exec("pkill -f 'craft service' 2>/dev/null || true");
        let _ = self
            .session
            .exec("pkill -f 'craft daemon' 2>/dev/null || true");

        // macOS launchctl cleanup if present
        let _ = self.session.exec(
            "launchctl unload -w ~/Library/LaunchAgents/com.craft.daemon.plist 2>/dev/null || true",
        );
        let _ = self
            .session
            .exec("rm -f ~/Library/LaunchAgents/com.craft.daemon.plist 2>/dev/null || true");

        // Windows scheduled task cleanup if present
        let _ = self
            .session
            .exec("schtasks /Delete /TN CraftDaemon /F 2>nul || true");

        // Remove installed binaries
        let _ = self
            .session
            .exec("rm -f ~/.local/bin/craft ~/craft 2>/dev/null || true");
        let _ = self
            .session
            .exec("sudo rm -f /usr/local/bin/craft 2>/dev/null || true");

        Ok(())
    }

    /// Cleans the downloaded asset cache on the remote host
    pub fn clean_cache(&self) -> Result<()> {
        let cmd = format!("{} cache clean", self.craft_bin());
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to clean remote cache: {}",
                err.trim()
            )));
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
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to trash remote backup: {}",
                err.trim()
            )));
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
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to restore remote trash: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Permanently deletes an archive from remote trash
    pub fn delete_trash_item(&self, filename: &str) -> Result<()> {
        let cmd = format!("rm -f ~/.craft/trash/\"{}\"", filename);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to delete remote trash item: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Empties all items in the remote trash directory
    pub fn empty_trash(&self) -> Result<()> {
        let cmd = "rm -rf ~/.craft/trash/*";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(CraftError::Other(format!(
                "Failed to empty remote trash: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    /// Dispatches a structured log search to the remote host executing `craft log search --json ...`
    pub fn search_remote_logs(&self, query: &craft_core::LogQuery) -> Result<craft_core::LogSearchResult> {
        let mut cmd = format!("craft log search \"{}\" --json", query.query_pattern.replace('"', "\\\""));
        if let Some(ref srv) = query.server_name {
            cmd.push_str(&format!(" --server \"{}\"", srv));
        }
        if let Some(lvl) = query.level {
            cmd.push_str(&format!(" --level \"{}\"", lvl));
        }
        if query.is_regex {
            cmd.push_str(" --regex");
        }
        if query.limit > 0 {
            cmd.push_str(&format!(" --limit {}", query.limit));
        }

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote log search failed: {}", err.trim())));
        }

        let result: craft_core::LogSearchResult = serde_json::from_str(&stdout)
            .map_err(|e| CraftError::Other(format!("Failed to parse remote log search results: {}", e)))?;
        Ok(result)
    }

    /// Dispatches a workload forecast query to the remote host executing `craft forecast show <server> --horizon <hours> --json`
    pub fn get_remote_forecast(&self, server: &str, horizon_hours: u32) -> Result<craft_core::WorkloadForecast> {
        let cmd = format!("craft forecast show \"{}\" --horizon {} --json", server.replace('"', "\\\""), horizon_hours);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote workload forecast failed: {}", err.trim())));
        }

        let forecast: craft_core::WorkloadForecast = serde_json::from_str(&stdout)
            .map_err(|e| CraftError::Other(format!("Failed to parse remote forecast results: {}", e)))?;
        Ok(forecast)
    }

    /// Dispatches a cost optimization report query to the remote host executing `craft forecast cost --json`
    pub fn get_remote_cost_report(&self, server: Option<&str>) -> Result<craft_core::CostOptimizationReport> {
        let mut cmd = "craft forecast cost --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server \"{}\"", srv.replace('"', "\\\"")));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote cost report failed: {}", err.trim())));
        }

        let report: craft_core::CostOptimizationReport = serde_json::from_str(&stdout)
            .map_err(|e| CraftError::Other(format!("Failed to parse remote cost report: {}", e)))?;
        Ok(report)
    }

    /// Syncs a modpack delta patch to the remote host executing `craft modpack patch <base_pack> <delta_patch> --output <output_path>`
    pub fn sync_modpack_delta(
        &self,
        base_pack_path: &str,
        delta_patch_path: &str,
        output_path: &str,
    ) -> Result<String> {
        let cmd = format!(
            "craft modpack patch \"{}\" \"{}\" --output \"{}\"",
            base_pack_path.replace('"', "\\\""),
            delta_patch_path.replace('"', "\\\""),
            output_path.replace('"', "\\\"")
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote modpack delta patch failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn apply_remote_sdn_mesh(
        &self,
        wg_conf_content: &str,
    ) -> Result<String> {
        let cmd = format!(
            "mkdir -p ~/.craft/sdn/wireguard && cat << 'EOF' > ~/.craft/sdn/wireguard/wg0.conf\n{}\nEOF\ncraft sdn up 2>/dev/null || true",
            wg_conf_content
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SDN mesh configuration failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_raft_status(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft raft status --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote raft status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn propose_remote_raft_command(&self, action: &str, data: &str) -> Result<String> {
        let cmd = format!("craft raft propose --action {} --data {} --json", action, data);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote raft proposal failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_server_quota(&self, server: &str) -> Result<String> {
        let cmd = format!("craft quota get {} --json", server);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote quota query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn set_remote_server_quota(
        &self,
        server: &str,
        cpu: Option<u32>,
        memory_mb: Option<u64>,
        priority: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!("craft quota set {} --json", server);
        if let Some(c) = cpu {
            cmd.push_str(&format!(" --cpu {}", c));
        }
        if let Some(m) = memory_mb {
            cmd.push_str(&format!(" --memory {}", m));
        }
        if let Some(p) = priority {
            cmd.push_str(&format!(" --priority {}", p));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote quota update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn list_remote_quotas(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft quota list --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote quota listing failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_tracing_status(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft trace status --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote tracing status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn query_remote_traces(
        &self,
        service: Option<&str>,
        min_duration_ms: Option<u64>,
        limit: Option<usize>,
    ) -> Result<String> {
        let mut cmd = "craft trace list --json".to_string();
        if let Some(s) = service {
            cmd.push_str(&format!(" --service {}", s));
        }
        if let Some(m) = min_duration_ms {
            cmd.push_str(&format!(" --min-duration-ms {}", m));
        }
        if let Some(l) = limit {
            cmd.push_str(&format!(" --limit {}", l));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote trace query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_trace_details(&self, trace_id: &str) -> Result<String> {
        let cmd = format!("craft trace get {} --json", trace_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote trace details query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn export_remote_traces(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft trace export --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote trace export failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn set_remote_tracing_config(
        &self,
        enabled: Option<bool>,
        sampler: Option<&str>,
        sample_ratio: Option<f64>,
        otlp_endpoint: Option<&str>,
        service_name: Option<&str>,
    ) -> Result<String> {
        let mut cmd = "craft trace config --json".to_string();
        if let Some(en) = enabled {
            cmd.push_str(&format!(" --enabled {}", en));
        }
        if let Some(s) = sampler {
            cmd.push_str(&format!(" --sampler {}", s));
        }
        if let Some(r) = sample_ratio {
            cmd.push_str(&format!(" --sample-ratio {}", r));
        }
        if let Some(ep) = otlp_endpoint {
            cmd.push_str(&format!(" --otlp-endpoint {}", ep));
        }
        if let Some(sn) = service_name {
            cmd.push_str(&format!(" --service-name {}", sn));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote tracing config update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_anvil_status(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft anvil status --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote anvil status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn inspect_remote_region(&self, server: &str, file: &str) -> Result<String> {
        let cmd = format!("craft anvil inspect {} {} --json", server, file);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote anvil region inspection failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn prefetch_remote_chunks(
        &self,
        server: &str,
        world: Option<&str>,
        x: i32,
        z: i32,
        radius: u32,
    ) -> Result<String> {
        let mut cmd = format!("craft anvil prefetch {} --x {} --z {} --radius {} --json", server, x, z, radius);
        if let Some(w) = world {
            cmd.push_str(&format!(" --world {}", w));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote anvil chunk prefetch failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn benchmark_remote_anvil(&self, chunks: usize) -> Result<String> {
        let cmd = format!("craft anvil bench --chunks {} --json", chunks);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote anvil benchmark failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn set_remote_anvil_config(
        &self,
        enabled: Option<bool>,
        engine: Option<&str>,
        cache_mb: Option<usize>,
        radius: Option<u32>,
    ) -> Result<String> {
        let mut cmd = "craft anvil config --json".to_string();
        if let Some(en) = enabled {
            cmd.push_str(&format!(" --enabled {}", en));
        }
        if let Some(eng) = engine {
            cmd.push_str(&format!(" --engine {}", eng));
        }
        if let Some(mb) = cache_mb {
            cmd.push_str(&format!(" --cache-mb {}", mb));
        }
        if let Some(r) = radius {
            cmd.push_str(&format!(" --prefetch-radius {}", r));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote anvil config update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_numa_status(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft numa status --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote NUMA status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn pin_remote_server_cores(
        &self,
        server: &str,
        cpus: &[usize],
        numa_node: Option<u32>,
        policy: Option<&str>,
    ) -> Result<String> {
        let cpus_str = craft_core::format_cpu_range_string(cpus);
        let mut cmd = format!("craft numa pin {} --cpus {} --json", server, cpus_str);
        if let Some(node) = numa_node {
            cmd.push_str(&format!(" --node {}", node));
        }
        if let Some(pol) = policy {
            cmd.push_str(&format!(" --policy {}", pol));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote core pinning failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn set_remote_numa_policy(&self, server: &str, policy: &str) -> Result<String> {
        let cmd = format!("craft numa policy {} --policy {} --json", server, policy);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote NUMA policy update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn benchmark_remote_numa_memory(&self, node: u32, size_mb: usize) -> Result<String> {
        let cmd = format!("craft numa bench --node {} --size-mb {} --json", node, size_mb);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote NUMA memory benchmark failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    pub fn get_remote_dpdk_status(&self, bench_count: Option<usize>) -> Result<String> {
        let mut cmd = "craft dpdk status --json".to_string();
        if let Some(cnt) = bench_count {
            cmd.push_str(&format!(" --bench {}", cnt));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote DPDK status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries Multi-Raft consensus partition status on remote host
    pub fn get_remote_multiraft_status(&self, group_id: Option<u64>) -> Result<String> {
        let mut cmd = "craft raft status --json".to_string();
        if let Some(gid) = group_id {
            cmd.push_str(&format!(" --group {}", gid));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote Multi-Raft status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Reconfigures cluster membership in a Multi-Raft group on remote host
    pub fn reconfigure_remote_membership(
        &self,
        group_id: u64,
        change_type: &str,
        node_id: &str,
        address: &str,
        port: u16,
        voting: bool,
    ) -> Result<String> {
        let mut cmd = format!(
            "craft raft reconfigure --group {} --action {} --node-id {} --address {} --port {}",
            group_id, change_type, node_id, address, port
        );
        if voting {
            cmd.push_str(" --voting");
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote Raft reconfiguration failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Triggers streaming log compaction and snapshot creation on remote host
    pub fn trigger_remote_log_compaction(&self, group_id: u64, force: bool) -> Result<String> {
        let mut cmd = format!("craft raft compact --group {}", group_id);
        if force {
            cmd.push_str(" --force");
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote log compaction failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Routes an application key to a Multi-Raft group on remote host
    pub fn route_remote_partition_key(&self, key: &str) -> Result<String> {
        let cmd = format!("craft raft partition route {} --json", key);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote key routing failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Initiates a zero-downtime live migration on the remote host
    pub fn start_remote_live_migration(&self, plan: &craft_core::LiveMigrationPlan) -> Result<String> {
        let cmd = format!(
            "craft migrate live {} --target-node {} --target-host {} --target-port {} --freeze-max-ms {} --json",
            plan.server_name, plan.target_node, plan.target_host, plan.target_port, plan.freeze_timeout_ms
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote live migration failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries live migration status from the remote host
    pub fn get_remote_migration_status(&self, migration_id: Option<&str>) -> Result<String> {
        let cmd = if let Some(id) = migration_id {
            format!("craft migrate status --id {} --json", id)
        } else {
            "craft migrate list --json".to_string()
        };
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote migration status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Manages an Anycast BGP route on the remote host
    pub fn manage_remote_anycast_route(
        &self,
        action: &str,
        prefix: &str,
        asn: Option<u32>,
    ) -> Result<String> {
        let mut cmd = format!("craft anycast route {} --prefix {}", action, prefix);
        if let Some(a) = asn {
            cmd.push_str(&format!(" --asn {}", a));
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote Anycast route management failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Starts an eBPF profiling session on the remote host
    pub fn start_remote_ebpf_profiling(
        &self,
        server: &str,
        probe_type: &str,
        duration_secs: u64,
        sample_rate_hz: u32,
    ) -> Result<String> {
        let cmd = format!(
            "craft bpf trace {} --event {} --duration {} --rate {} --json",
            server, probe_type, duration_secs, sample_rate_hz
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote eBPF profiling start failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries the eBPF probe status and syscall telemetry on the remote host
    pub fn get_remote_ebpf_status(&self, server: &str) -> Result<String> {
        let cmd = format!("craft bpf status {} --json", server);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote eBPF status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches a folded or SVG flame graph from the remote host
    pub fn get_remote_ebpf_flamegraph(&self, server: &str, format: &str) -> Result<String> {
        let cmd = format!("craft bpf flamegraph {} --format {} --json", server, format);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote eBPF flamegraph query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Formats an exec command with an active W3C traceparent environment prefix if provided
    pub fn exec_with_trace_context(
        &self,
        cmd: &str,
        traceparent: Option<&str>,
    ) -> Result<(i32, String, String)> {
        let full_cmd = if let Some(tp) = traceparent {
            format!("CRAFT_TRACEPARENT=\"{}\" {}", tp, cmd)
        } else {
            cmd.to_string()
        };
        self.session.exec(&full_cmd)
    }

    /// Verifies a remote artifact cryptographic provenance against the remote policy
    pub fn verify_remote_artifact(
        &self,
        artifact_path: &str,
        attestation_path: Option<&str>,
        strict: bool,
    ) -> Result<String> {
        let mut cmd = format!("craft attest verify {}", artifact_path);
        if let Some(att) = attestation_path {
            cmd.push_str(&format!(" --attestation {}", att));
        }
        if strict {
            cmd.push_str(" --strict");
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote artifact verification failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches the remote host's cryptographic supply chain policy
    pub fn get_remote_supply_chain_policy(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft attest policy --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote supply chain policy query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Sets the remote host's supply chain enforcement mode
    pub fn set_remote_supply_chain_policy(
        &self,
        mode: &str,
        min_slsa: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!("craft attest policy --set-mode {}", mode);
        if let Some(lvl) = min_slsa {
            cmd.push_str(&format!(" --min-slsa {}", lvl));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote supply chain policy update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches the remote host's post-quantum cryptographic status and harvest defense score
    pub fn get_remote_pqc_status(&self) -> Result<String> {
        let (code, stdout, stderr) = self.session.exec("craft pqc status --json")?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PQC status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Updates the remote host's post-quantum policy enforcement mode and ciphersuite
    pub fn set_remote_pqc_policy(
        &self,
        mode: &str,
        suite: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!("craft pqc policy --set-mode {}", mode);
        if let Some(s) = suite {
            cmd.push_str(&format!(" --ciphersuite {}", s));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PQC policy update failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Runs a hardware/software post-quantum cryptographic benchmark on the remote host
    pub fn benchmark_remote_pqc(&self, iterations: usize) -> Result<String> {
        let cmd = format!("craft pqc bench --iterations {} --json", iterations);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PQC benchmark failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Transitions the remote host's post-quantum migration phase
    pub fn migrate_remote_pqc_node(&self, phase: &str) -> Result<String> {
        let cmd = format!("craft pqc migrate --phase {} --json", phase);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PQC node migration failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries the hardware security module and TPM 2.0 status from the remote host
    pub fn get_remote_hsm_status(&self) -> Result<String> {
        let cmd = "craft hsm status --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote HSM status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Generates or validates an enclave attestation quote on the remote host
    pub fn attest_remote_hsm(&self, pcr_mask: Option<u32>, nonce: Option<&str>) -> Result<String> {
        let mut cmd = "craft hsm attest".to_string();
        if let Some(mask) = pcr_mask {
            cmd.push_str(&format!(" --pcr-mask {}", mask));
        }
        if let Some(n) = nonce {
            cmd.push_str(&format!(" --nonce {}", n));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote HSM enclave attestation failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Verifies zero-knowledge cluster membership on the remote host
    pub fn verify_remote_zk_membership(&self, cluster_id: &str) -> Result<String> {
        let cmd = format!("craft hsm zk-member --action prove --cluster {} --json", cluster_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote ZK cluster membership failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries the memory compaction, THP, and page pool status from the remote host
    pub fn get_remote_compaction_status(&self) -> Result<String> {
        let cmd = "craft memory status --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote compaction status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Triggers an immediate memory defragmentation cycle on the remote host
    pub fn trigger_remote_compaction(&self) -> Result<String> {
        let cmd = "craft memory compact --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote memory compaction trigger failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Configures transparent hugepages and defrag policy on the remote host
    pub fn configure_remote_thp(&self, mode: &str, defrag: Option<&str>) -> Result<String> {
        let mut cmd = format!("craft memory thp --mode {}", mode);
        if let Some(def) = defrag {
            cmd.push_str(&format!(" --defrag {}", def));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote THP configuration failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches autonomous eBPF XDP firewall and flow status from the remote host
    pub fn get_remote_xdp_status(&self) -> Result<String> {
        let cmd = "craft xdp status --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote XDP status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Attaches eBPF XDP firewall to a network interface on the remote host
    pub fn attach_remote_xdp(&self, interface: &str, mode: Option<&str>) -> Result<String> {
        let mut cmd = format!("craft xdp attach {}", interface);
        if let Some(m) = mode {
            cmd.push_str(&format!(" --mode {}", m));
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote XDP attachment failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Detaches eBPF XDP firewall from network interface on the remote host
    pub fn detach_remote_xdp(&self) -> Result<String> {
        let cmd = "craft xdp detach --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote XDP detachment failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets XDP packet and drop metrics on the remote host
    pub fn reset_remote_xdp_metrics(&self) -> Result<String> {
        let cmd = "craft xdp reset-metrics --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote XDP metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches PMU and cache miss profiling status from the remote host
    pub fn get_remote_pmu_status(&self) -> Result<String> {
        let cmd = "craft pmu status --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PMU status failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Starts PMU sampling on the remote host
    pub fn start_remote_pmu_sampling(&self, pid: Option<u32>, rate_hz: Option<u32>) -> Result<String> {
        let mut cmd = "craft pmu sample".to_string();
        if let Some(p) = pid {
            cmd.push_str(&format!(" --pid {}", p));
        }
        if let Some(r) = rate_hz {
            cmd.push_str(&format!(" --rate {}", r));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PMU start sampling failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches top execution hotspot symbols from the remote host
    pub fn get_remote_pmu_hotspots(&self, limit: Option<usize>) -> Result<String> {
        let limit_val = limit.unwrap_or(10);
        let cmd = format!("craft pmu hotspots --limit {} --json", limit_val);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PMU hotspots query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets PMU counter metrics and clear sample buffers on the remote host
    pub fn reset_remote_pmu_metrics(&self) -> Result<String> {
        let cmd = "craft pmu reset-metrics --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote PMU metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches POSIX shared memory status summary from the remote host
    pub fn get_remote_shm_status(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft shm status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SHM status query failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Creates a new high-speed shared memory channel on the remote host
    pub fn create_remote_shm_channel(
        &self,
        server: &str,
        channel: &str,
        slot_size: Option<usize>,
        slots: Option<usize>,
    ) -> Result<String> {
        let mut cmd = format!("craft shm create --server {} --channel {}", server, channel);
        if let Some(sz) = slot_size {
            cmd.push_str(&format!(" --slot-size {}", sz));
        }
        if let Some(cnt) = slots {
            cmd.push_str(&format!(" --slots {}", cnt));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SHM channel creation failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Closes and unlinks a shared memory channel on the remote host
    pub fn close_remote_shm_channel(&self, server: &str, channel: &str) -> Result<String> {
        let cmd = format!("craft shm close --server {} --channel {} --json", server, channel);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SHM channel close failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Runs a zero-copy throughput and latency benchmark on the remote host
    pub fn run_remote_shm_bench(
        &self,
        message_count: Option<usize>,
        payload_size: Option<usize>,
    ) -> Result<String> {
        let mut cmd = "craft shm bench".to_string();
        if let Some(cnt) = message_count {
            cmd.push_str(&format!(" --messages {}", cnt));
        }
        if let Some(sz) = payload_size {
            cmd.push_str(&format!(" --size {}", sz));
        }
        cmd.push_str(" --json");

        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SHM benchmark failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets shared memory counters and metrics on the remote host
    pub fn reset_remote_shm_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft shm reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SHM metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries dynamic binary patch summary status from the remote host
    pub fn get_remote_patch_status(&self, server: Option<&str>) -> Result<craft_core::patch::PatchStatusSummary> {
        let mut cmd = "craft patch status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch status failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote patch status: {}", e)))
    }

    /// Applies a dynamic binary patch on the remote host
    pub fn apply_remote_patch(
        &self,
        server: &str,
        patch: &str,
        target: &str,
        bytes: &str,
    ) -> Result<craft_core::patch::PatchManifest> {
        let cmd = format!(
            "craft patch apply --server {} --patch {} --target {} --bytes {} --json",
            server, patch, target, bytes
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch apply failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote patch manifest: {}", e)))
    }

    /// Rolls back an active dynamic binary patch on the remote host
    pub fn rollback_remote_patch(&self, server: &str, patch: &str) -> Result<bool> {
        let cmd = format!(
            "craft patch rollback --server {} --patch {} --json",
            server, patch
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch rollback failed: {}", err.trim())));
        }
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| CraftError::Other(format!("Failed to parse remote rollback response: {}", e)))?;
        Ok(parsed.get("success").and_then(|v| v.as_bool()).unwrap_or(true))
    }

    /// Retrieves unified diff disassembly for a patch on the remote host
    pub fn get_remote_patch_diff(&self, server: &str, patch: &str) -> Result<String> {
        let cmd = format!(
            "craft patch diff --server {} --patch {} --json",
            server, patch
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch diff failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Executes synthetic patch application throughput benchmark on the remote host
    pub fn run_remote_patch_bench(&self, iterations: usize) -> Result<craft_core::patch::PatchBenchmarkMetrics> {
        let cmd = format!("craft patch bench --iterations {} --json", iterations);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch bench failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote patch bench metrics: {}", e)))
    }

    /// Resets patch runtime metrics and telemetry on the remote host
    pub fn reset_remote_patch_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft patch reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote patch metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries MicroVM sandboxes status on the remote host
    pub fn get_remote_vm_status(&self, vm_id: Option<&str>) -> Result<craft_core::vm::MicroVmStatusSummary> {
        let mut cmd = "craft vm status --json".to_string();
        if let Some(id) = vm_id {
            cmd.push_str(&format!(" --id {}", id));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM status query failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote VM status: {}", e)))
    }

    /// Provisions a new MicroVM sandbox on the remote host
    pub fn spawn_remote_vm(
        &self,
        name: &str,
        vcpus: u32,
        memory_mb: u64,
        vsock_cid: Option<u32>,
        devices: Vec<String>,
    ) -> Result<craft_core::vm::MicroVmDescriptor> {
        let mut cmd = format!("craft vm spawn -n {} --vcpus {} --memory {} --json", name, vcpus, memory_mb);
        if let Some(cid) = vsock_cid {
            cmd.push_str(&format!(" --cid {}", cid));
        }
        if !devices.is_empty() {
            cmd.push_str(&format!(" --devices {}", devices.join(",")));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM spawn failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote VM descriptor: {}", e)))
    }

    /// Stops a running MicroVM sandbox on the remote host
    pub fn stop_remote_vm(&self, vm_id: &str, force: bool) -> Result<bool> {
        let mut cmd = format!("craft vm stop -i {} --json", vm_id);
        if force {
            cmd.push_str(" --force");
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM stop failed: {}", err.trim())));
        }
        Ok(stdout.contains("\"stopped\": true") || stdout.contains("true"))
    }

    /// Inspects a MicroVM sandbox on the remote host
    pub fn inspect_remote_vm(&self, vm_id: &str) -> Result<craft_core::vm::MicroVmDescriptor> {
        let cmd = format!("craft vm inspect -i {} --json", vm_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM inspect failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote VM inspection: {}", e)))
    }

    /// Executes synthetic MicroVM cold start benchmark on the remote host
    pub fn run_remote_vm_bench(
        &self,
        concurrency: usize,
        iterations: usize,
    ) -> Result<craft_core::vm::MicroVmBenchmarkMetrics> {
        let cmd = format!("craft vm bench --concurrency {} --iterations {} --json", concurrency, iterations);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM bench failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote VM bench metrics: {}", e)))
    }

    /// Resets MicroVM cumulative metrics on the remote host
    pub fn reset_remote_vm_metrics(&self, vm_id: Option<&str>) -> Result<String> {
        let mut cmd = "craft vm reset-metrics --json".to_string();
        if let Some(id) = vm_id {
            cmd.push_str(&format!(" --id {}", id));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote VM metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries autonomous crash triage status summary from the remote host
    pub fn get_remote_crash_status(
        &self,
        server: Option<&str>,
    ) -> Result<craft_core::crash::CrashTriageStatusSummary> {
        let mut cmd = "craft crash status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash status failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote crash status: {}", e)))
    }

    /// Triages a remote crash dump or hs_err file on the remote host
    pub fn triage_remote_crash_file(
        &self,
        server: Option<&str>,
        file_path: &str,
    ) -> Result<craft_core::crash::CrashTriageReport> {
        let mut cmd = format!("craft crash triage -f {} --json", file_path);
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash triage failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote crash triage report: {}", e)))
    }

    /// Lists triaged crash reports from the remote host
    pub fn list_remote_crash_reports(
        &self,
        server: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<craft_core::crash::CrashTriageReport>> {
        let mut cmd = "craft crash list --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        if let Some(lim) = limit {
            cmd.push_str(&format!(" --limit {}", lim));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash list failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote crash reports: {}", e)))
    }

    /// Inspects a specific crash triage report on the remote host
    pub fn get_remote_crash_report(&self, report_id: &str) -> Result<craft_core::crash::CrashTriageReport> {
        let cmd = format!("craft crash inspect -i {} --json", report_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash inspect failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote crash report: {}", e)))
    }

    /// Runs synthetic crash triage and leak detection benchmark on the remote host
    pub fn run_remote_crash_bench(
        &self,
        iterations: usize,
    ) -> Result<craft_core::crash::CrashTriageBenchmarkMetrics> {
        let cmd = format!("craft crash bench --iterations {} --json", iterations);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash bench failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote crash bench metrics: {}", e)))
    }

    /// Resets crash triage and leak metrics on the remote host
    pub fn reset_remote_crash_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft crash reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote crash metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches RDMA status from the remote host
    pub fn get_remote_rdma_status(&self, server: Option<&str>) -> Result<craft_core::rdma::RdmaStatusSummary> {
        let mut cmd = "craft rdma status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA status query failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote RDMA status: {}", e)))
    }

    /// Registers a memory region on the remote host
    pub fn register_remote_rdma_mr(&self, server: Option<&str>, size: usize, read_only: bool) -> Result<craft_core::rdma::MemoryRegionDescriptor> {
        let mut cmd = format!("craft rdma mr-register --size {} --json", size);
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        if read_only {
            cmd.push_str(" --read-only");
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA MR register failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote RDMA MR descriptor: {}", e)))
    }

    /// Connects to a remote RDMA peer endpoint
    pub fn connect_remote_rdma_peer(&self, server: Option<&str>, peer: &str, qp: u32) -> Result<craft_core::rdma::RdmaPeerEndpoint> {
        let mut cmd = format!("craft rdma connect --peer {} --qp {} --json", peer, qp);
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA peer connect failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote RDMA peer endpoint: {}", e)))
    }

    /// Lists connected RDMA peer endpoints on the remote host
    pub fn list_remote_rdma_peers(&self, server: Option<&str>) -> Result<Vec<craft_core::rdma::RdmaPeerEndpoint>> {
        let mut cmd = "craft rdma peers --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA peers query failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote RDMA peers list: {}", e)))
    }

    /// Runs an RDMA zero-copy throughput benchmark on the remote host
    pub fn run_remote_rdma_bench(&self, iterations: usize, buffer_size: usize) -> Result<craft_core::rdma::RdmaBenchmarkMetrics> {
        let cmd = format!("craft rdma bench --iterations {} --buffer-size {} --json", iterations, buffer_size);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA bench failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote RDMA bench metrics: {}", e)))
    }

    /// Resets RDMA metrics on the remote host
    pub fn reset_remote_rdma_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft rdma reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote RDMA metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries SmartNIC offload status and device enumeration on the remote host
    pub fn get_remote_smartnic_status(&self, server: Option<&str>) -> Result<(craft_core::SmartNicStatusSummary, Vec<craft_core::SmartNicDeviceInfo>)> {
        let mut cmd = "craft smartnic status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC status query failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote SmartNIC status: {}", e)))
    }

    /// Installs a SmartNIC match-action offload rule on the remote host
    pub fn install_remote_smartnic_rule(&self, rule: &craft_core::SmartNicOffloadRule) -> Result<craft_core::SmartNicOffloadRule> {
        let mut cmd = format!("craft smartnic rule-add --rule-id {} --protocol {} --action {}",
            rule.rule_id, rule.protocol.as_str(), rule.action.to_string());
        if let Some(port) = rule.match_port {
            cmd.push_str(&format!(" --port {}", port));
        }
        if let Some(ref cidr) = rule.match_cidr {
            cmd.push_str(&format!(" --cidr {}", cidr));
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC rule installation failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse installed remote SmartNIC rule: {}", e)))
    }

    /// Removes a SmartNIC offload rule on the remote host
    pub fn remove_remote_smartnic_rule(&self, rule_id: &str) -> Result<bool> {
        let cmd = format!("craft smartnic rule-rm --rule-id {} --json", rule_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC rule removal failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote SmartNIC removal result: {}", e)))
    }

    /// Lists active SmartNIC offload rules on the remote host
    pub fn list_remote_smartnic_rules(&self, server: Option<&str>) -> Result<Vec<craft_core::SmartNicOffloadRule>> {
        let mut cmd = "craft smartnic rules --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC rules list failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote SmartNIC rules list: {}", e)))
    }

    /// Runs a line-rate SmartNIC packet switching benchmark on the remote host
    pub fn run_remote_smartnic_bench(&self, iterations: usize, packet_size: usize) -> Result<craft_core::SmartNicBenchmarkMetrics> {
        let cmd = format!("craft smartnic bench --iterations {} --packet-size {} --json", iterations, packet_size);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC bench failed: {}", err.trim())));
        }
        serde_json::from_str(&stdout).map_err(|e| CraftError::Other(format!("Failed to parse remote SmartNIC bench metrics: {}", e)))
    }

    /// Resets SmartNIC cumulative counters on the remote host
    pub fn reset_remote_smartnic_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft smartnic reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!("Remote SmartNIC metrics reset failed: {}", err.trim())));
        }
        Ok(stdout.trim().to_string())
    }

    /// Retrieves distributed memory fabric status from the remote host
    pub fn get_remote_memfabric_status(
        &self,
        server: Option<&str>,
    ) -> Result<(
        craft_core::memfabric::MemFabricStatusSummary,
        Vec<craft_core::memfabric::MemFabricNodeInfo>,
    )> {
        let mut cmd = "craft memfabric status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric status query failed: {}",
                err.trim()
            )));
        }
        serde_json::from_str(&stdout).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote MemFabric status: {}", e))
        })
    }

    /// Allocates a memory fabric page on the remote host
    pub fn allocate_remote_memfabric_page(
        &self,
        page_id: &str,
        size: usize,
        tier: &str,
        dimension: Option<&str>,
    ) -> Result<craft_core::memfabric::RemotePageDescriptor> {
        let mut cmd = format!(
            "craft memfabric page-alloc --page-id {} --size {} --tier {}",
            page_id, size, tier
        );
        if let Some(dim) = dimension {
            cmd.push_str(&format!(" --dimension {}", dim));
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric page allocation failed: {}",
                err.trim()
            )));
        }
        serde_json::from_str(&stdout).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote MemFabric page: {}", e))
        })
    }

    /// Evicts a dormant dimension on the remote host to the cluster NVRAM pool
    pub fn evict_remote_memfabric_dimension(
        &self,
        dimension: &str,
        target_node: Option<&str>,
    ) -> Result<(usize, u64, String)> {
        let mut cmd = format!("craft memfabric evict-dim --dimension {}", dimension);
        if let Some(target) = target_node {
            cmd.push_str(&format!(" --target-node {}", target));
        }
        cmd.push_str(" --json");
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric dimension eviction failed: {}",
                err.trim()
            )));
        }
        serde_json::from_str(&stdout).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote MemFabric evict result: {}", e))
        })
    }

    /// Lists active memory fabric pages on the remote host
    pub fn list_remote_memfabric_pages(
        &self,
        server: Option<&str>,
    ) -> Result<Vec<craft_core::memfabric::RemotePageDescriptor>> {
        let mut cmd = "craft memfabric pages --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric pages list failed: {}",
                err.trim()
            )));
        }
        serde_json::from_str(&stdout).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote MemFabric pages list: {}", e))
        })
    }

    /// Runs a memory fabric remote paging benchmark on the remote host
    pub fn run_remote_memfabric_bench(
        &self,
        iterations: usize,
        page_size: usize,
    ) -> Result<craft_core::memfabric::MemFabricBenchmarkMetrics> {
        let cmd = format!(
            "craft memfabric bench --iterations {} --page-size {} --json",
            iterations, page_size
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric bench failed: {}",
                err.trim()
            )));
        }
        serde_json::from_str(&stdout).map_err(|e| {
            CraftError::Other(format!("Failed to parse remote MemFabric bench metrics: {}", e))
        })
    }

    /// Resets memory fabric telemetry counters on the remote host
    pub fn reset_remote_memfabric_metrics(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft memfabric reset-metrics --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote MemFabric metrics reset failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Fetches NVMe-oF target and distributed flash pool status from the remote host
    pub fn get_remote_nvme_status(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft nvme status --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe status query failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Dynamically provisions an NVMe namespace in the remote flash block pool
    pub fn create_remote_nvme_namespace(
        &self,
        nsid: u32,
        size_mb: u64,
        block_size: u32,
        server: Option<&str>,
        dimension: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!(
            "craft nvme ns-create --nsid {} --size-mb {} --block-size {} --json",
            nsid, size_mb, block_size
        );
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        if let Some(dim) = dimension {
            cmd.push_str(&format!(" --dimension {}", dim));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe namespace creation failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Deletes an NVMe namespace on the remote host
    pub fn delete_remote_nvme_namespace(&self, nsid: u32) -> Result<String> {
        let cmd = format!("craft nvme ns-delete --nsid {} --json", nsid);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe namespace deletion failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Lists NVMe namespaces on the remote host
    pub fn list_remote_nvme_namespaces(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft nvme namespaces --json".to_string();
        if let Some(srv) = server {
            cmd.push_str(&format!(" --server {}", srv));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe namespaces list failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Lists NVMe-oF target subsystems on the remote host
    pub fn list_remote_nvme_subsystems(&self) -> Result<String> {
        let cmd = "craft nvme subsystems --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe subsystems list failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Benchmarks 4KB flash block fabric I/O on the remote host
    pub fn run_remote_nvme_bench(&self, block_size: usize, iterations: usize) -> Result<String> {
        let cmd = format!(
            "craft nvme bench --block-size {} --iterations {} --json",
            block_size, iterations
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe bench failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets NVMe storage fabric telemetry counters on the remote host
    pub fn reset_remote_nvme_metrics(&self) -> Result<String> {
        let cmd = "craft nvme reset-metrics --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote NVMe metrics reset failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries WireGuard PQXDH VPN mesh status on the remote host
    pub fn get_remote_vpn_status(&self, server: Option<&str>) -> Result<String> {
        let mut cmd = "craft vpn status --json".to_string();
        if let Some(s) = server {
            cmd.push_str(&format!(" --server {}", s));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN get status failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Creates a WireGuard PQXDH VPN tunnel on the remote host
    pub fn create_remote_vpn_tunnel(
        &self,
        tunnel_id: &str,
        address: &str,
        port: u16,
        crypto_mode: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!(
            "craft vpn tunnel-create --tunnel-id {} --address {} --port {} --json",
            tunnel_id, address, port
        );
        if let Some(mode) = crypto_mode {
            cmd.push_str(&format!(" --crypto-mode {}", mode));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN tunnel create failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Deletes a WireGuard PQXDH VPN tunnel on the remote host
    pub fn delete_remote_vpn_tunnel(&self, tunnel_id: &str) -> Result<String> {
        let cmd = format!("craft vpn tunnel-delete --tunnel-id {} --json", tunnel_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN tunnel delete failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Adds a peer to a WireGuard PQXDH VPN tunnel on the remote host
    pub fn add_remote_vpn_peer(
        &self,
        tunnel_id: &str,
        peer_id: &str,
        endpoint: &str,
        allowed_ips: &[String],
    ) -> Result<String> {
        let mut cmd = format!(
            "craft vpn peer-add --tunnel-id {} --peer-id {} --endpoint {} --json",
            tunnel_id, peer_id, endpoint
        );
        for ip in allowed_ips {
            cmd.push_str(&format!(" --allowed-ip {}", ip));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN peer add failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Removes a peer from a WireGuard PQXDH VPN tunnel on the remote host
    pub fn remove_remote_vpn_peer(&self, tunnel_id: &str, peer_id: &str) -> Result<String> {
        let cmd = format!(
            "craft vpn peer-rm --tunnel-id {} --peer-id {} --json",
            tunnel_id, peer_id
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN peer remove failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Executes zero-loss PQXDH key rotation on the remote host
    pub fn rotate_remote_vpn_key(&self, tunnel_id: &str, peer_id: Option<&str>) -> Result<String> {
        let mut cmd = format!("craft vpn rotate-key --tunnel-id {} --json", tunnel_id);
        if let Some(pid) = peer_id {
            cmd.push_str(&format!(" --peer-id {}", pid));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN key rotation failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Runs end-to-end VPN mesh benchmark on the remote host
    pub fn run_remote_vpn_bench(&self, iterations: usize, packet_size: usize) -> Result<String> {
        let cmd = format!(
            "craft vpn bench --iterations {} --packet-size {} --json",
            iterations, packet_size
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN bench failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets VPN mesh telemetry counters on the remote host
    pub fn reset_remote_vpn_metrics(&self) -> Result<String> {
        let cmd = "craft vpn reset-metrics --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote VPN metrics reset failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Queries BFT consensus cluster status and PACEMAKER view telemetry from the remote host
    pub fn get_remote_bft_status(&self) -> Result<String> {
        let cmd = "craft bft status --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT status query failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Submits a state-transition transaction to the remote BFT consensus mempool
    pub fn submit_remote_bft_transaction(
        &self,
        tx_type: &str,
        payload: &str,
        sender: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!(
            "craft bft tx-submit --type {} --payload \"{}\" --json",
            tx_type, payload
        );
        if let Some(s) = sender {
            cmd.push_str(&format!(" --sender {}", s));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT tx submit failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Lists active BFT consensus validators and public keys on the remote host
    pub fn list_remote_bft_validators(&self) -> Result<String> {
        let cmd = "craft bft validators --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT list validators failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Registers a new validator into the remote BFT consensus quorum
    pub fn add_remote_bft_validator(
        &self,
        validator_id: &str,
        voting_weight: u64,
        public_key: Option<&str>,
        role: Option<&str>,
    ) -> Result<String> {
        let mut cmd = format!(
            "craft bft validator-add --id {} --voting-weight {} --json",
            validator_id, voting_weight
        );
        if let Some(pk) = public_key {
            cmd.push_str(&format!(" --public-key {}", pk));
        }
        if let Some(r) = role {
            cmd.push_str(&format!(" --role {}", r));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT validator add failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Removes or slashes a validator from the remote BFT consensus quorum
    pub fn remove_remote_bft_validator(&self, validator_id: &str) -> Result<String> {
        let cmd = format!("craft bft validator-rm --id {} --json", validator_id);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT validator remove failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Triggers a view change / pacemaker timeout on the remote BFT consensus cluster
    pub fn trigger_remote_bft_view_change(&self, reason: Option<&str>) -> Result<String> {
        let mut cmd = "craft bft view-change --json".to_string();
        if let Some(r) = reason {
            cmd.push_str(&format!(" --reason \"{}\"", r));
        }
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT view-change failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Verifies a zero-knowledge recursive state transition proof on the remote host
    pub fn verify_remote_bft_zk_proof(&self, proof_path: &str) -> Result<String> {
        let cmd = format!("craft bft zk-verify --proof \"{}\" --json", proof_path);
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT ZK proof verify failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Runs end-to-end BFT consensus benchmark on the remote host
    pub fn run_remote_bft_bench(&self, transactions: usize, validators: usize) -> Result<String> {
        let cmd = format!(
            "craft bft bench --transactions {} --validators {} --json",
            transactions, validators
        );
        let (code, stdout, stderr) = self.session.exec(&cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT bench failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }

    /// Resets BFT consensus telemetry counters on the remote host
    pub fn reset_remote_bft_metrics(&self) -> Result<String> {
        let cmd = "craft bft reset-metrics --json";
        let (code, stdout, stderr) = self.session.exec(cmd)?;
        if code != 0 {
            let err = if !stderr.trim().is_empty() { stderr } else { stdout };
            return Err(CraftError::Other(format!(
                "Remote BFT metrics reset failed: {}",
                err.trim()
            )));
        }
        Ok(stdout.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_str() {
        assert_eq!(
            RemoteCraftClient::parse_version_str("craft 1.0.0\n"),
            Some("1.0.0".to_string())
        );
        assert_eq!(
            RemoteCraftClient::parse_version_str("craft 1.0.1"),
            Some("1.0.1".to_string())
        );
        assert_eq!(
            RemoteCraftClient::parse_version_str("1.0.1\n"),
            Some("1.0.1".to_string())
        );
        assert_eq!(
            RemoteCraftClient::parse_version_str("craft version 1.0.1\n"),
            Some("1.0.1".to_string())
        );
        assert_eq!(RemoteCraftClient::parse_version_str(""), None);
        assert_eq!(RemoteCraftClient::parse_version_str("   \n"), None);
    }
}
