use crate::cli::AutoCommands;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_auto(action: AutoCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        AutoCommands::How => {
            println!(
                "{}",
                "=== Craft Background Auto-Run Service Setup ==="
                    .cyan()
                    .bold()
            );
            println!(
                "{}",
                "Recommended: Run 'craft service install' to automatically configure and start"
                    .green()
                    .bold()
            );
            println!(
                "{}",
                "the background supervisor daemon as an OS-managed service for your user account."
                    .green()
            );
            println!("\nCommands:");
            println!("  {}  - Install and enable auto-start service", "craft service install".yellow().bold());
            println!("  {}   - Start the service immediately", "craft service start".yellow());
            println!("  {}  - View current service status", "craft service status".yellow());
            println!("  {} - Uninstall the auto-start service", "craft service uninstall".yellow());

            println!("\n{}", "--- Manual Configuration Reference ---".dimmed());

            #[cfg(target_os = "windows")]
            {
                println!("Windows Scheduled Task / Startup Shortcut:");
                println!("1. Press Win + R, type 'shell:startup' and press Enter.");
                println!("2. Create a shortcut in that folder pointing to your craft executable:");
                println!("   Target: \"C:\\path\\to\\craft.exe\" service start --foreground");
                println!("Craft service will now start automatically upon Windows login.");
            }

            #[cfg(target_os = "macos")]
            {
                println!("macOS LaunchAgent (~/Library/LaunchAgents/com.craft.service.plist):");
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
                println!("Linux user systemd service (~/.config/systemd/user/craft.service):");
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
