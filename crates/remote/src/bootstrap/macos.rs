use colored::Colorize;
use craft_core::Result;
use crate::session::RemoteSession;

pub fn bootstrap_macos(session: &RemoteSession) -> Result<()> {
    println!("{}", "=== Bootstrapping macOS Remote Host ===".cyan().bold());

    let sw_vers = session.exec_checked("sw_vers")?;
    println!("Remote macOS info:\n{}", sw_vers.dimmed());

    // 1. Check Java 21+
    println!("{}", "Checking remote Java installation...".cyan());
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((0, stdout, stderr)) => {
            let out = format!("{}\n{}", stdout, stderr);
            !out.contains("\"21") && !out.contains("\"22") && !out.contains("\"23") && !out.contains("\"24") && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        println!("{}", "Installing OpenJDK 21 on remote macOS host...".yellow());
        if session.exec("which brew").map(|(c, ..)| c == 0).unwrap_or(false) {
            let _ = session.exec("brew install openjdk@21");
            let _ = session.exec("sudo ln -sfn /opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk /Library/Java/JavaVirtualMachines/openjdk-21.jdk 2>/dev/null || true");
        } else {
            println!("{}", "Homebrew not found. Please install Java 21+ on the remote macOS machine.".yellow());
        }
    } else {
        println!("{}", "[OK] Compatible Java 21+ already present on remote host.".green());
    }

    // 2. Create Craft directories
    session.exec_checked("mkdir -p ~/Library/craft/servers ~/Library/craft/download_cache ~/Library/craft/backups ~/bin ~/Library/LaunchAgents")?;

    // 3. Configure LaunchAgent plist for background daemon
    println!("{}", "Configuring LaunchAgent for Craft daemon...".cyan());
    let plist_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.craft.service</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/craft</string>
    <string>service</string>
    <string>start</string>
    <string>--foreground</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
"#;

    let write_cmd = format!(
        "cat << 'EOF' > ~/Library/LaunchAgents/com.craft.service.plist\n{}\nEOF",
        plist_content
    );
    session.exec_checked(&write_cmd)?;

    println!("{}", "[OK] macOS host bootstrapped successfully!".green().bold());
    Ok(())
}
