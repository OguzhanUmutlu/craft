use crate::session::RemoteSession;
use craft_core::Result;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn find_local_craft_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if exe.is_file() && is_elf_executable(&exe) {
            return Some(exe);
        }
    }

    let candidates = [
        PathBuf::from("/D/Projects/craft/bin/craft"),
        PathBuf::from("/D/Projects/craft/target/release/craft"),
        PathBuf::from("/usr/local/bin/craft"),
    ];
    for p in &candidates {
        if p.is_file() && is_elf_executable(p) {
            return Some(p.clone());
        }
    }
    None
}

fn is_elf_executable(path: &Path) -> bool {
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut magic = [0u8; 4];
        if f.read_exact(&mut magic).is_ok() {
            return &magic == b"\x7fELF";
        }
    }
    false
}

fn upload_craft_binary(session: &RemoteSession, local_bin_path: &Path) -> Result<()> {
    let sftp = session.sftp()?;
    let mut local_file = std::fs::File::open(local_bin_path).map_err(craft_core::CraftError::Io)?;

    let remote_home = session
        .exec("echo $HOME")
        .map(|(_, out, _)| out.trim().to_string())
        .unwrap_or_default();

    let remote_dest = if !remote_home.is_empty() {
        PathBuf::from(format!("{}/.local/bin/craft", remote_home))
    } else {
        PathBuf::from(".local/bin/craft")
    };

    let mut remote_file = sftp
        .create(&remote_dest)
        .or_else(|_| sftp.create(Path::new(".local/bin/craft")))
        .map_err(|e| {
            craft_core::CraftError::Other(format!(
                "Failed to create remote file ~/.local/bin/craft: {}",
                e
            ))
        })?;

    let mut buf = [0u8; 64 * 1024];
    loop {
        let count = local_file
            .read(&mut buf)
            .map_err(craft_core::CraftError::Io)?;
        if count == 0 {
            break;
        }
        remote_file
            .write_all(&buf[..count])
            .map_err(|e| craft_core::CraftError::Other(format!("SFTP write error: {}", e)))?;
    }

    session.exec_checked("chmod 755 ~/.local/bin/craft")?;
    Ok(())
}

pub fn bootstrap_linux(session: &RemoteSession, progress: &mut dyn FnMut(&str)) -> Result<()> {
    progress("Bootstrapping Linux Remote Host...");

    // 1. Detect OS distro
    let os_info = session.exec_checked("cat /etc/os-release 2>/dev/null || echo 'NAME=Linux'")?;
    let os_name = os_info
        .lines()
        .find(|l| l.starts_with("PRETTY_NAME="))
        .unwrap_or("Linux");
    progress(&format!("Remote OS: {}", os_name));

    // 2. Check Java 21+
    progress("Checking remote Java installation...");
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((0, stdout, stderr)) => {
            let out = format!("{}\n{}", stdout, stderr);
            let first_line = out.lines().next().unwrap_or("");
            if !first_line.is_empty() {
                progress(&format!("Detected Java: {}", first_line));
            }
            !out.contains("\"21")
                && !out.contains("\"22")
                && !out.contains("\"23")
                && !out.contains("\"24")
                && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        progress("Installing OpenJDK 21 on remote Linux host...");

        // Try apt
        if session
            .exec("which apt-get")
            .map(|(c, ..)| c == 0)
            .unwrap_or(false)
        {
            progress("Installing Java via apt package manager...");
            let _ = session
                .exec("sudo apt-get update -y && sudo apt-get install -y openjdk-21-jre-headless");
        } else if session
            .exec("which dnf")
            .map(|(c, ..)| c == 0)
            .unwrap_or(false)
        {
            progress("Installing Java via dnf package manager...");
            let _ = session.exec("sudo dnf install -y java-21-openjdk-headless");
        } else if session
            .exec("which pacman")
            .map(|(c, ..)| c == 0)
            .unwrap_or(false)
        {
            progress("Installing Java via pacman package manager...");
            let _ = session.exec("sudo pacman -Sy --noconfirm jre21-openjdk-headless");
        } else {
            progress("Warning: Package manager not recognized. Ensure Java 21+ is installed.");
        }
    } else {
        progress("[OK] Compatible Java 21+ already present on remote host.");
    }

    // 3. Create Craft directories
    progress("Creating Craft remote directories...");
    session.exec_checked("mkdir -p ~/.craft/servers ~/.craft/download_cache ~/.craft/backups ~/.config/systemd/user ~/.local/bin")?;

    // 4. Install Craft binary to ~/.local/bin/craft
    progress("Installing Craft CLI binary on remote host...");
    let mut installed = false;

    if let Some(local_bin) = find_local_craft_binary() {
        progress(&format!(
            "Uploading Craft executable from '{}'...",
            local_bin.display()
        ));
        match upload_craft_binary(session, &local_bin) {
            Ok(()) => {
                installed = true;
                progress("[OK] Craft binary deployed to ~/.local/bin/craft.");
            }
            Err(e) => {
                progress(&format!(
                    "Direct upload note: {}. Trying alternative installation...",
                    e
                ));
            }
        }
    }

    if !installed {
        // Alternative: Cargo or remote curl
        if session
            .exec("which cargo")
            .map(|(c, ..)| c == 0)
            .unwrap_or(false)
        {
            progress("Compiling Craft on remote host via cargo...");
            let _ = session.exec("cargo install --git https://github.com/OguzhanUmutlu/craft.git craft --root ~/.local");
        } else {
            progress("Downloading Craft binary release via curl...");
            let _ = session.exec("curl -sSL -f https://github.com/OguzhanUmutlu/craft/releases/latest/download/craft-linux-x86_64 -o ~/.local/bin/craft && chmod 755 ~/.local/bin/craft 2>/dev/null || true");
        }
    }

    // Ensure permissions and PATH symlinks
    let _ = session.exec("chmod 755 ~/.local/bin/craft 2>/dev/null || true");
    let _ = session.exec("sudo cp -f ~/.local/bin/craft /usr/local/bin/craft 2>/dev/null || true");
    let _ = session.exec("sudo chmod 755 /usr/local/bin/craft 2>/dev/null || true");
    let _ = session.exec("grep -q '.local/bin' ~/.bashrc || echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.bashrc 2>/dev/null || true");
    let _ = session.exec("grep -q '.local/bin' ~/.profile || echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.profile 2>/dev/null || true");

    // Verify remote binary
    let verify = session
        .exec("~/.local/bin/craft --version || /usr/local/bin/craft --version || craft --version");
    if let Ok((0, out, _)) = verify {
        progress(&format!("[OK] Remote Craft verified: {}", out.trim()));
    } else {
        progress("Warning: Could not verify remote 'craft --version'.");
    }

    // 4. Configure systemd user service for 24/7 background operation
    progress("Configuring systemd service for Craft daemon...");
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
    let _ = session.exec("systemctl --user enable --now craft.service");
    let _ = session.exec("systemctl --user restart craft.service 2>/dev/null || true");
    let _ = session.exec("~/.local/bin/craft service start 2>/dev/null || ~/.local/bin/craft daemon start 2>/dev/null || true");

    // 5. Check firewall (UFW)
    if session
        .exec("which ufw")
        .map(|(c, ..)| c == 0)
        .unwrap_or(false)
    {
        if let Ok((code, out, _)) = session.exec("sudo ufw status | grep 'Status: active'") {
            if code == 0 && out.contains("active") {
                progress("Opening Minecraft ports (25565/tcp, 19132/udp) in UFW...");
                let _ = session.exec("sudo ufw allow 25565/tcp comment 'Minecraft Java'");
                let _ = session.exec("sudo ufw allow 19132/udp comment 'Minecraft Bedrock'");
            }
        }
    }

    progress("[OK] Linux host bootstrapped successfully!");
    Ok(())
}
