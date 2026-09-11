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

        session.set_tcp_stream(tcp.try_clone().map_err(|e| CraftError::Io(e))?);
        session.handshake()
            .map_err(|e| CraftError::Other(format!("SSH handshake failed: {}", e)))?;

        // Authenticate
        match config.auth_type {
            RemoteAuthType::Key => {
                let key_path = if let Some(ref p) = config.key_path {
                    p.clone()
                } else {
                    find_default_private_key()
                        .ok_or_else(|| CraftError::Other("No default SSH private key found in ~/.ssh/".to_string()))?
                };

                let expanded_key = expand_tilde(&key_path);
                if !expanded_key.exists() {
                    return Err(CraftError::Other(format!("SSH private key not found at '{}'", expanded_key.display())));
                }

                session.userauth_pubkey_file(&config.user, None, &expanded_key, config.password.as_deref())
                    .map_err(|e| CraftError::Other(format!("SSH key authentication failed for {}: {}", config.user, e)))?;
            }
            RemoteAuthType::Agent => {
                let mut agent = session.agent()
                    .map_err(|e| CraftError::Other(format!("Failed to connect to SSH agent: {}", e)))?;
                agent.connect()
                    .map_err(|e| CraftError::Other(format!("Could not connect to SSH agent: {}", e)))?;
                agent.list_identities()
                    .map_err(|e| CraftError::Other(format!("Failed to list SSH agent identities: {}", e)))?;

                let identities = agent.identities()
                    .map_err(|e| CraftError::Other(format!("Failed to get identities: {}", e)))?;

                let mut authed = false;
                for identity in identities {
                    if agent.userauth(&config.user, &identity).is_ok() {
                        authed = true;
                        break;
                    }
                }

                if !authed {
                    return Err(CraftError::Other(format!("SSH agent authentication failed for user '{}'", config.user)));
                }
            }
            RemoteAuthType::Password => {
                let password = config.password.as_deref().ok_or_else(|| {
                    CraftError::Other("Password authentication specified but no password configured".to_string())
                })?;

                session.userauth_password(&config.user, password)
                    .map_err(|e| CraftError::Other(format!("SSH password authentication failed for {}: {}", config.user, e)))?;
            }
        }

        if !session.authenticated() {
            return Err(CraftError::Other("SSH authentication was rejected".to_string()));
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

fn find_default_private_key() -> Option<PathBuf> {
    let home = directories::UserDirs::new()?.home_dir().to_path_buf();
    let ssh_dir = home.join(".ssh");

    let candidates = [
        ssh_dir.join("id_ed25519"),
        ssh_dir.join("id_rsa"),
        ssh_dir.join("id_ecdsa"),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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
