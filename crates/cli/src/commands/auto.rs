use crate::cli::AutoCommands;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_auto(action: AutoCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        AutoCommands::How => {
            #[cfg(target_os = "windows")]
            {
                println!(
                    "{}",
                    "=== How to Enable Craft Auto-Run on Windows ==="
                        .cyan()
                        .bold()
                );
                println!("1. Press Win + R, type 'shell:startup' and press Enter.");
                println!("2. Create a shortcut in that folder pointing to your craft executable:");
                println!("   Target: \"C:\\path\\to\\craft.exe\" service start --foreground");
                println!("Craft service will now start automatically upon Windows login.");
            }

            #[cfg(target_os = "macos")]
            {
                println!(
                    "{}",
                    "=== How to Enable Craft Auto-Run on macOS ==="
                        .cyan()
                        .bold()
                );
                println!("Create a LaunchAgent in ~/Library/LaunchAgents/com.craft.service.plist:");
                println!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
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
</plist>"#
                );
                println!("Then load it with: launchctl load ~/Library/LaunchAgents/com.craft.service.plist");
            }

            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            {
                println!(
                    "{}",
                    "=== How to Enable Craft Auto-Run on Linux ==="
                        .cyan()
                        .bold()
                );
                println!("Option A: User systemd service (recommended):");
                println!("Create ~/.config/systemd/user/craft.service:");
                println!(
                    r#"[Unit]
Description=Craft Minecraft Server Management Daemon
After=network.target

[Service]
ExecStart=/usr/local/bin/craft service start --foreground
Restart=on-failure

[Install]
WantedBy=default.target"#
                );
                println!("Enable and start it with:");
                println!("  systemctl --user daemon-reload");
                println!("  systemctl --user enable --now craft.service");
                println!("  loginctl enable-linger $USER");
            }
        }
        AutoCommands::Add { name, path } => {
            let server_path = paths.resolve_server_path(path.as_deref(), Some(&name), true)?;
            let mut registry = ServersRegistry::load(paths)?;

            if !registry.set_auto(&server_path, true) {
                return Err(CraftError::ServerNotFound(format!(
                    "Server at '{}' is not registered.",
                    server_path.display()
                )));
            }
            registry.save(paths)?;
            println!(
                "{}",
                format!("Server '{}' added to auto-run list.", server_path.display()).green()
            );
        }
        AutoCommands::Rm { name, path } => {
            let server_path = paths.resolve_server_path(path.as_deref(), Some(&name), true)?;
            let mut registry = ServersRegistry::load(paths)?;

            if !registry.set_auto(&server_path, false) {
                return Err(CraftError::ServerNotFound(format!(
                    "Server at '{}' is not registered.",
                    server_path.display()
                )));
            }
            registry.save(paths)?;
            println!(
                "{}",
                format!(
                    "Server '{}' removed from auto-run list.",
                    server_path.display()
                )
                .green()
            );
        }
        AutoCommands::Start => {
            DaemonClient::ensure_daemon_started(paths).await?;
            println!("{}", "Auto-run service started.".green());
        }
    }

    Ok(())
}
