use craft_core::Result;
use crate::session::RemoteSession;

pub fn bootstrap_macos(session: &RemoteSession, progress: &mut dyn FnMut(&str)) -> Result<()> {
    progress("Bootstrapping macOS Remote Host...");

    let sw_vers = session.exec_checked("sw_vers")?;
    let sw_first = sw_vers.lines().next().unwrap_or("macOS");
    progress(&format!("Remote macOS: {}", sw_first));

    // 1. Check Java 21+
    progress("Checking remote Java installation...");
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((0, stdout, stderr)) => {
            let out = format!("{}\n{}", stdout, stderr);
            !out.contains("\"21") && !out.contains("\"22") && !out.contains("\"23") && !out.contains("\"24") && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        progress("Installing OpenJDK 21 on remote macOS host...");
        if session.exec("which brew").map(|(c, ..)| c == 0).unwrap_or(false) {
            progress("Installing OpenJDK 21 via Homebrew...");
            let _ = session.exec("brew install openjdk@21");
            let _ = session.exec("sudo ln -sfn /opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk /Library/Java/JavaVirtualMachines/openjdk-21.jdk 2>/dev/null || true");
        } else {
            progress("Warning: Homebrew not found. Please install Java 21+ on remote macOS.");
        }
    } else {
        progress("[OK] Compatible Java 21+ already present on remote host.");
    }

    // 2. Create Craft directories
    progress("Creating Craft remote directories...");
    session.exec_checked("mkdir -p ~/Library/craft/servers ~/Library/craft/download_cache ~/Library/craft/backups ~/bin ~/Library/LaunchAgents")?;

    // 3. Configure LaunchAgent plist for background daemon
    progress("Configuring LaunchAgent for Craft daemon...");
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

    progress("[OK] macOS host bootstrapped successfully!");
    Ok(())
}
