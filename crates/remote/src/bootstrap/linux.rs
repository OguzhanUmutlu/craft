use colored::Colorize;
use craft_core::Result;
use crate::session::RemoteSession;

pub fn bootstrap_linux(session: &RemoteSession) -> Result<()> {
    println!("{}", "=== Bootstrapping Linux Remote Host ===".cyan().bold());

    // 1. Detect OS distro
    let os_info = session.exec_checked("cat /etc/os-release 2>/dev/null || echo 'NAME=Linux'")?;
    println!("Remote OS: {}", os_info.lines().find(|l| l.starts_with("PRETTY_NAME=")).unwrap_or("Linux"));

    // 2. Check Java 21+
    println!("{}", "Checking remote Java installation...".cyan());
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((0, stdout, stderr)) => {
            let out = format!("{}\n{}", stdout, stderr);
            println!("Detected remote Java:\n{}", out.lines().next().unwrap_or("").dimmed());
            !out.contains("\"21") && !out.contains("\"22") && !out.contains("\"23") && !out.contains("\"24") && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        println!("{}", "Installing OpenJDK 21 on remote Linux host...".yellow());

        // Try apt
        if session.exec("which apt-get").map(|(c, ..)| c == 0).unwrap_or(false) {
            println!("Using apt package manager...");
            let _ = session.exec("sudo apt-get update -y && sudo apt-get install -y openjdk-21-jre-headless");
        } else if session.exec("which dnf").map(|(c, ..)| c == 0).unwrap_or(false) {
            println!("Using dnf package manager...");
            let _ = session.exec("sudo dnf install -y java-21-openjdk-headless");
        } else if session.exec("which pacman").map(|(c, ..)| c == 0).unwrap_or(false) {
            println!("Using pacman package manager...");
            let _ = session.exec("sudo pacman -Sy --noconfirm jre21-openjdk-headless");
        } else {
            println!("{}", "Package manager not recognized. Please ensure Java 21+ is installed on the remote machine.".yellow());
        }
    } else {
        println!("{}", "[OK] Compatible Java 21+ already present on remote host.".green());
    }

    // 3. Create Craft directories
    session.exec_checked("mkdir -p ~/.craft/servers ~/.craft/download_cache ~/.craft/backups ~/.config/systemd/user ~/.local/bin")?;

    // 4. Configure systemd user service for 24/7 background operation
    println!("{}", "Configuring systemd service for Craft daemon...".cyan());
    let service_content = r#"[Unit]
Description=Craft Minecraft Server Management Daemon
After=network.target

[Service]
ExecStart=%h/.local/bin/craft service start --foreground
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
"#;

    let write_cmd = format!(
        "cat << 'EOF' > ~/.config/systemd/user/craft.service\n{}\nEOF",
        service_content
    );
    session.exec_checked(&write_cmd)?;

    // Enable linger so user service runs when not logged in
    let _ = session.exec("loginctl enable-linger $USER 2>/dev/null || true");
    let _ = session.exec("systemctl --user daemon-reload");
    let _ = session.exec("systemctl --user enable craft.service");

    // 5. Check firewall (UFW)
    if session.exec("which ufw").map(|(c, ..)| c == 0).unwrap_or(false) {
        if let Ok((code, out, _)) = session.exec("sudo ufw status | grep 'Status: active'") {
            if code == 0 && out.contains("active") {
                println!("{}", "Opening Minecraft ports (25565/tcp, 19132/udp) in UFW...".cyan());
                let _ = session.exec("sudo ufw allow 25565/tcp comment 'Minecraft Java'");
                let _ = session.exec("sudo ufw allow 19132/udp comment 'Minecraft Bedrock'");
            }
        }
    }

    println!("{}", "[OK] Linux host bootstrapped successfully!".green().bold());
    Ok(())
}
