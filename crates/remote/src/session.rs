use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use ssh2::Session;
use craft_core::{CraftError, RemoteAuthType, RemoteHostConfig, RemoteOsType, Result};

pub struct RemoteSession {
    pub session: Session,
    pub config: RemoteHostConfig,
    _tcp: TcpStream,
}

impl RemoteSession {
    pub fn connect(config: &RemoteHostConfig) -> Result<Self> {
        let addr = format!("{}:{}", config.host, config.port);
        let socket_addrs: Vec<_> = addr.to_socket_addrs()
            .map_err(|e| CraftError::Other(format!("Failed to resolve host {}: {}", addr, e)))?
            .collect();

        if socket_addrs.is_empty() {
            return Err(CraftError::Other(format!("No IP addresses found for {}", addr)));
        }

        let tcp = TcpStream::connect_timeout(&socket_addrs[0], Duration::from_secs(10))
            .map_err(|e| CraftError::Other(format!("Failed to connect to SSH {}: {}", addr, e)))?;

        let mut session = Session::new()
            .map_err(|e| CraftError::Other(format!("Failed to initialize SSH session: {}", e)))?;

        session.set_tcp_stream(tcp.try_clone().map_err(CraftError::Io)?);
        session.handshake()
            .map_err(|e| CraftError::Other(format!("SSH handshake failed: {}", e)))?;

        // Authenticate (OpenSSH-style resilient chain):
        if config.auth_type == RemoteAuthType::Password {
            let password = config.password.as_deref().ok_or_else(|| {
                CraftError::Other("Password authentication specified but no password configured".to_string())
            })?;
            session.userauth_password(&config.user, password)
                .map_err(|e| CraftError::Other(format!("SSH password authentication failed for {}: {}", config.user, e)))?;
        } else {
            // 1. Try SSH Agent first (matches standard OpenSSH behavior)
            if let Ok(mut agent) = session.agent() {
                if agent.connect().is_ok() {
                    if let Ok(identities) = agent.identities() {
                        for identity in identities {
                            if agent.userauth(&config.user, &identity).is_ok() {
                                break;
                            }
                        }
                    }
                }
            }

            // 2. If not yet authenticated, try explicit configured key_path (if present and file exists)
            if !session.authenticated() {
                if let Some(ref p) = config.key_path {
                    let expanded = expand_tilde(p);
                    if expanded.exists() {
                        let _ = session.userauth_pubkey_file(&config.user, None, &expanded, config.password.as_deref());
                    }
                }
            }

            // 3. If still not authenticated, try standard candidate private keys in ~/.ssh/
            if !session.authenticated() {
                if let Some(user_dirs) = directories::UserDirs::new() {
                    let ssh_dir = user_dirs.home_dir().join(".ssh");
                    let candidates = [
                        ssh_dir.join("id_ed25519"),
                        ssh_dir.join("id_rsa"),
                        ssh_dir.join("id_ecdsa"),
                    ];
                    for cand in &candidates {
                        if cand.is_file()
                            && session.userauth_pubkey_file(&config.user, None, cand, config.password.as_deref()).is_ok()
                        {
                            break;
                        }
                    }

                }
            }

            // 4. If password is also provided, try password fallback
            if !session.authenticated() {
                if let Some(ref pass) = config.password {
                    let _ = session.userauth_password(&config.user, pass);
                }
            }
        }

        if !session.authenticated() {
            let detail = match config.auth_type {
                RemoteAuthType::Password => format!("Password authentication failed for user '{}'.", config.user),
                _ => {
                    if let Some(ref kp) = config.key_path {
                        let exp = expand_tilde(kp);
                        if !exp.exists() {
                            format!(
                                "Configured key '{}' does not exist, and SSH agent / default keys in ~/.ssh/ could not authenticate as '{}'.",
                                kp.display(),
                                config.user
                            )
                        } else {
                            format!("Authentication rejected for user '{}' with key '{}' and SSH agent.", config.user, kp.display())
                        }
                    } else {
                        format!("Authentication failed for user '{}' (no valid key or SSH agent identity accepted).", config.user)
                    }
                }
            };
            return Err(CraftError::Other(detail));
        }


        Ok(Self {
            session,
            config: config.clone(),
            _tcp: tcp,
        })
    }

    /// Execute a command and capture exit code, stdout, and stderr
    pub fn exec(&self, command: &str) -> Result<(i32, String, String)> {
        let mut channel = self.session.channel_session()
            .map_err(|e| CraftError::Other(format!("Failed to open SSH channel: {}", e)))?;

        channel.exec(command)
            .map_err(|e| CraftError::Other(format!("Failed to exec remote command '{}': {}", command, e)))?;

        let mut stdout = String::new();
        let _ = channel.read_to_string(&mut stdout);

        let mut stderr = String::new();
        let _ = channel.stderr().read_to_string(&mut stderr);

        channel.wait_close()
            .map_err(|e| CraftError::Other(format!("Channel close error: {}", e)))?;

        let exit_code = channel.exit_status().unwrap_or(0);
        Ok((exit_code, stdout, stderr))
    }

    /// Execute command and return Ok if exit status == 0
    pub fn exec_checked(&self, command: &str) -> Result<String> {
        let (code, stdout, stderr) = self.exec(command)?;
        if code == 0 {
            Ok(stdout)
        } else {
            Err(CraftError::Other(format!(
                "Remote command failed with exit code {}: {}",
                code,
                if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() }
            )))
        }
    }

    /// Automatically probes the remote operating system
    pub fn probe_os(&self) -> Result<RemoteOsType> {
        if let Some(os) = self.config.os_type {
            return Ok(os);
        }

        // Try uname first (Linux & macOS)
        if let Ok((code, out, _)) = self.exec("uname -s") {
            if code == 0 {
                let s = out.trim();
                if s.eq_ignore_ascii_case("Darwin") {
                    return Ok(RemoteOsType::MacOS);
                } else if s.eq_ignore_ascii_case("Linux") {
                    return Ok(RemoteOsType::Linux);
                }
            }
        }

        // Try Windows PowerShell check
        if let Ok((code, out, _)) = self.exec("powershell -Command \"Write-Host Windows\"") {
            if code == 0 && out.contains("Windows") {
                return Ok(RemoteOsType::Windows);
            }
        }

        // Fallback default
        Ok(RemoteOsType::Linux)
    }

    pub fn sftp(&self) -> Result<ssh2::Sftp> {
        self.session.sftp()
            .map_err(|e| CraftError::Other(format!("Failed to open SFTP session: {}", e)))
    }
}

pub fn expand_tilde(path: &Path) -> PathBuf {

    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            return if s == "~" {
                home.to_path_buf()
            } else {
                home.join(&s[2..])
            };
        }
    }
    path.to_path_buf()
}
