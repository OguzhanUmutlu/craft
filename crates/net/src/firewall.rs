use craft_core::{CraftError, Result};
use std::process::Command;

#[cfg(unix)]
fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
            } else {
                None
            }
        })
        .map(|uid| uid == 0)
        .unwrap_or(false)
}

pub fn allow_ip_port(ip: &str, port: u16, is_udp: bool) -> Result<()> {
    let proto = if is_udp { "udp" } else { "tcp" };

    #[cfg(target_os = "windows")]
    {
        let rule_name = format!("Craft_Allow_{}_{}", ip.replace(['.', ':'], "_"), port);
        let status = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &format!("name={}", rule_name),
                "dir=in",
                "action=allow",
                &format!("protocol={}", proto),
                &format!("localport={}", port),
                &format!("remoteip={}", ip),
            ])
            .status()
            .map_err(|e| CraftError::Other(format!("Failed to execute netsh: {}", e)))?;

        if status.success() {
            return Ok(());
        }

        let ps_args = format!(
            "advfirewall firewall add rule name=\\\"{}\\\" dir=in action=allow protocol={} localport={} remoteip={}",
            rule_name, proto, port, ip
        );
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Start-Process netsh -ArgumentList '{}' -Verb RunAs -Wait",
                    ps_args
                ),
            ])
            .status();

        if let Ok(s) = status {
            if s.success() {
                return Ok(());
            }
        }

        Err(CraftError::Other(
            "netsh command failed. Administrator privileges required.".to_string(),
        ))
    }

    #[cfg(target_os = "macos")]
    {
        let root = is_root();
        let rule = format!("pass in proto {} from {} to any port {}\n", proto, ip, port);
        let rule_path = std::env::temp_dir().join("craft_pf.rule");
        std::fs::write(&rule_path, &rule)
            .map_err(|e| CraftError::Other(format!("Failed to write pf rule: {}", e)))?;

        let mut cmd = if root {
            Command::new("pfctl")
        } else {
            let mut c = Command::new("sudo");
            c.arg("pfctl");
            c
        };
        let status = cmd
            .args(["-a", "craft", "-f", &rule_path.to_string_lossy()])
            .status()
            .map_err(|e| CraftError::Other(format!("pfctl execution error: {}", e)))?;

        let _ = std::fs::remove_file(&rule_path);

        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Other(
                "pfctl command failed. Sudo privileges required on macOS.".to_string(),
            ))
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let root = is_root();

        // Try ufw first
        let ufw_check = Command::new("which").arg("ufw").output();
        if let Ok(out) = ufw_check {
            if out.status.success() {
                let mut cmd = if root {
                    Command::new("ufw")
                } else {
                    let mut c = Command::new("sudo");
                    c.arg("ufw");
                    c
                };
                let status = cmd
                    .args([
                        "allow",
                        "from",
                        ip,
                        "to",
                        "any",
                        "port",
                        &port.to_string(),
                        "proto",
                        proto,
                    ])
                    .status()
                    .map_err(|e| CraftError::Other(format!("ufw execution error: {}", e)))?;
                if status.success() {
                    return Ok(());
                } else {
                    return Err(CraftError::Other(
                        "ufw command failed. Sudo privileges or valid firewall configuration required.".to_string(),
                    ));
                }
            }
        }

        // Fallback to iptables
        let mut cmd = if root {
            Command::new("iptables")
        } else {
            let mut c = Command::new("sudo");
            c.arg("iptables");
            c
        };
        let status = cmd
            .args([
                "-A",
                "INPUT",
                "-p",
                proto,
                "-s",
                ip,
                "--dport",
                &port.to_string(),
                "-j",
                "ACCEPT",
            ])
            .status()
            .map_err(|e| CraftError::Other(format!("iptables execution error: {}", e)))?;

        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Other(
                "iptables command failed. Sudo privileges required.".to_string(),
            ))
        }
    }
}
