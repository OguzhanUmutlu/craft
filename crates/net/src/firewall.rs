use std::process::Command;
use craft_core::{CraftError, Result};

pub fn allow_ip_port(ip: &str, port: u16, is_udp: bool) -> Result<()> {
    let proto = if is_udp { "udp" } else { "tcp" };

    #[cfg(target_os = "windows")]
    {
        let rule_name = format!("Craft_Allow_{}_{}", ip.replace(['.', ':'], "_"), port);
        let status = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", rule_name),
                "dir=in", "action=allow",
                &format!("protocol={}", proto),
                &format!("localport={}", port),
                &format!("remoteip={}", ip),
            ])
            .status()
            .map_err(|e| CraftError::Other(format!("Failed to execute netsh: {}", e)))?;

        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Other("netsh command failed. Administrator privileges required.".to_string()))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Try ufw first
        let ufw_check = Command::new("which").arg("ufw").output();
        if let Ok(out) = ufw_check {
            if out.status.success() {
                let status = Command::new("ufw")
                    .args(["allow", "from", ip, "to", "any", "port", &port.to_string(), "proto", proto])
                    .status()
                    .map_err(|e| CraftError::Other(format!("ufw execution error: {}", e)))?;
                if status.success() {
                    return Ok(());
                }
            }
        }

        // Fallback to iptables
        let status = Command::new("iptables")
            .args(["-A", "INPUT", "-p", proto, "-s", ip, "--dport", &port.to_string(), "-j", "ACCEPT"])
            .status()
            .map_err(|e| CraftError::Other(format!("iptables execution error: {}", e)))?;

        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Other("iptables command failed. Sudo privileges required.".to_string()))
        }
    }
}
