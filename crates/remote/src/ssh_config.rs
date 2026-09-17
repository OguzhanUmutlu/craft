use std::fs;
use std::path::{Path, PathBuf};
use craft_core::{RemoteAuthType, RemoteHostConfig};

#[derive(Debug, Clone, Default)]
pub struct SshConfigHost {
    pub pattern: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<PathBuf>,
}

/// Parses an OpenSSH config file (typically ~/.ssh/config)
pub fn parse_ssh_config<P: AsRef<Path>>(path: P) -> Vec<SshConfigHost> {
    let content = match fs::read_to_string(path.as_ref()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_ssh_config_content(&content)
}

/// Parses OpenSSH config format from a string
pub fn parse_ssh_config_content(content: &str) -> Vec<SshConfigHost> {
    let mut hosts = Vec::new();
    let mut current: Option<SshConfigHost> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split on first whitespace or '='
        let mut parts = line.splitn(2, |c: char| c.is_whitespace() || c == '=');
        let key = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = parts.next().unwrap_or("").trim().trim_matches('"').trim();

        if key == "host" {
            // New Host block
            if let Some(h) = current.take() {
                if !h.pattern.is_empty() && h.pattern != "*" {
                    hosts.push(h);
                }
            }

            // Can have multiple space-separated aliases: Host web1 web2
            let first_alias = val.split_whitespace().next().unwrap_or("").to_string();
            current = Some(SshConfigHost {
                pattern: first_alias,
                ..Default::default()
            });
        } else if let Some(ref mut h) = current {
            match key.as_str() {
                "hostname" if !val.is_empty() => {
                    h.host_name = Some(val.to_string());
                }
                "user" if !val.is_empty() => {
                    h.user = Some(val.to_string());
                }
                "port" => {
                    if let Ok(p) = val.parse::<u16>() {
                        h.port = Some(p);
                    }
                }
                "identityfile" if !val.is_empty() => {
                    h.identity_file = Some(PathBuf::from(val));
                }
                _ => {}
            }

        }
    }

    if let Some(h) = current {
        if !h.pattern.is_empty() && h.pattern != "*" {
            hosts.push(h);
        }
    }

    hosts
}

/// Locates the default ~/.ssh/config if it exists
pub fn get_default_ssh_config_path() -> Option<PathBuf> {
    let dirs = directories::UserDirs::new()?;
    let p = dirs.home_dir().join(".ssh").join("config");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// Discovers candidate RemoteHostConfigs from default ~/.ssh/config
pub fn discover_ssh_hosts() -> Vec<RemoteHostConfig> {
    let config_path = match get_default_ssh_config_path() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let default_user = whoami_user();

    parse_ssh_config(&config_path)
        .into_iter()
        .map(|h| {
            let host_addr = h.host_name.unwrap_or_else(|| h.pattern.clone());
            let user = h.user.unwrap_or_else(|| default_user.clone());
            let port = h.port.unwrap_or(22);

            let key_path = if let Some(ref p) = h.identity_file {
                let expanded = crate::session::expand_tilde(p);
                if expanded.exists() {
                    Some(p.clone())
                } else {
                    None
                }
            } else {
                None
            };

            RemoteHostConfig {
                alias: h.pattern,
                host: host_addr,
                port,
                user,
                auth_type: RemoteAuthType::Key,
                key_path,
                password: None,
                remote_dir: None,
                os_type: None,
            }

        })
        .collect()
}

/// Resolves a remote host by alias using OpenSSH `ssh -G <alias>` or ~/.ssh/config parser
pub fn resolve_ssh_host(alias: &str) -> Option<RemoteHostConfig> {
    // 1. Try `ssh -G <alias>` first (most accurate OpenSSH resolution including Includes and wildcards)
    if let Ok(output) = std::process::Command::new("ssh")
        .args(["-G", alias])
        .output()
    {
        if output.status.success() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                let mut host = None;
                let mut user = None;
                let mut port = 22;
                let mut identity_file = None;

                for line in text.lines() {
                    let mut parts = line.split_whitespace();
                    let key = parts.next().unwrap_or("").to_lowercase();
                    let val = parts.next().unwrap_or("");
                    match key.as_str() {
                        "hostname" if !val.is_empty() => host = Some(val.to_string()),
                        "user" if !val.is_empty() => user = Some(val.to_string()),
                        "port" => {
                            if let Ok(p) = val.parse::<u16>() {
                                port = p;
                            }
                        }
                        "identityfile" if !val.is_empty() && identity_file.is_none() => {
                            let p = PathBuf::from(val);
                            let expanded = crate::session::expand_tilde(&p);
                            if expanded.is_file() {
                                identity_file = Some(p);
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(host_addr) = host {
                    return Some(RemoteHostConfig {
                        alias: alias.to_string(),
                        host: host_addr,
                        port,
                        user: user.unwrap_or_else(whoami_user),
                        auth_type: RemoteAuthType::Key,
                        key_path: identity_file,
                        password: None,
                        remote_dir: None,
                        os_type: None,
                    });
                }
            }
        }
    }

    // 2. Fallback to discover_ssh_hosts() from ~/.ssh/config
    discover_ssh_hosts()
        .into_iter()
        .find(|h| h.alias.eq_ignore_ascii_case(alias))
}

fn whoami_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_config() {
        let sample = r#"
# Global configuration
Host *
    ServerAliveInterval 60

Host vps-us
    HostName 198.51.100.24
    User ubuntu
    Port 2222
    IdentityFile ~/.ssh/id_vps

Host myserver.net
    User craftadmin
"#;

        let hosts = parse_ssh_config_content(sample);
        assert_eq!(hosts.len(), 2);

        assert_eq!(hosts[0].pattern, "vps-us");
        assert_eq!(hosts[0].host_name.as_deref(), Some("198.51.100.24"));
        assert_eq!(hosts[0].user.as_deref(), Some("ubuntu"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[0].identity_file, Some(PathBuf::from("~/.ssh/id_vps")));

        assert_eq!(hosts[1].pattern, "myserver.net");
        assert_eq!(hosts[1].host_name, None);
        assert_eq!(hosts[1].user.as_deref(), Some("craftadmin"));
        assert_eq!(hosts[1].port, None);
    }

    #[test]
    fn test_resolve_ssh_host_localhost() {
        let res = resolve_ssh_host("localhost");
        assert!(res.is_some());
        let conf = res.unwrap();
        assert_eq!(conf.port, 22);
    }
}
