use colored::Colorize;
use craft_core::{CraftPaths, RemoteHostConfig, Result};
use craft_remote::RemoteCraftClient;
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status,
    run_menu, show_modal_message, MenuEntry,
};
use super::remote_control::remote_server_control_panel;

pub async fn manage_host_servers(
    paths: &CraftPaths,
    host_config: &RemoteHostConfig,
) -> Result<()> {
    print_in_place_status(
        "CONNECTING TO REMOTE HOST",
        &[format!(
            "Establishing SSH connection to '{}' ({}@{}:{})...",
            host_config.alias, host_config.user, host_config.host, host_config.port
        )],
    )?;

    let client = match RemoteCraftClient::connect(host_config) {
        Ok(c) => c,
        Err(e) => {
            show_modal_message(
                "SSH CONNECTION FAILED",
                &[
                    format!("Failed to connect to host '{}':", host_config.alias),
                    format!("[ERROR] {}", e),
                    "".to_string(),
                    "Check network, host address, SSH credentials, and port.".to_string(),
                ],
                true,
            )?;
            return Ok(());
        }
    };

    // Check if craft is installed on remote host
    if !client.is_craft_installed() {
        let width = get_content_width(80);
        let warn_header = format!(
            "{}\r\n{}\r\n{}\r\n Warning: 'craft' CLI is not found on remote host '{}'.\r\n To manage game servers, Craft needs to be installed on the remote machine.\r\n{}\r\n Choose an action:\r\n{}",
            box_top(width).yellow().bold(),
            box_title("CRAFT NOT FOUND ON REMOTE", width, false).yellow().bold(),
            box_divider(width).yellow().bold(),
            host_config.alias.cyan().bold(),
            box_divider(width).dimmed(),
            box_divider(width).dimmed(),
        );

        let warn_entries = vec![
            MenuEntry::new("1", "Bootstrap & Install Craft").with_aliases(&["b", "i"]),
            MenuEntry::new("2", "Continue Anyway").with_aliases(&["c"]),
            MenuEntry::new("0", "Cancel").with_aliases(&["q"]),
        ];

        let mut w_sel = 0;
        match run_menu(&warn_header, &warn_entries, &mut w_sel)? {
            Some(0) => {
                print_in_place_status(
                    "BOOTSTRAPPING REMOTE HOST",
                    &[format!("Installing Craft daemon and CLI on '{}'...", host_config.alias)],
                )?;

                match craft_remote::run_bootstrap(&client.session) {
                    Ok(_) => {

                        show_modal_message(
                            "BOOTSTRAP SUCCESS",
                            &[format!(
                                "[OK] Craft successfully installed on remote host '{}'!",
                                host_config.alias
                            )
                            .green()
                            .bold()
                            .to_string()],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "BOOTSTRAP FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                        return Ok(());
                    }
                }
            }
            Some(1) => {
                // Continue anyway
            }
            _ => return Ok(()),
        }
    }

    let mut selected = 0;

    loop {
        let servers = match client.list_servers() {
            Ok(s) => s,
            Err(e) => {
                show_modal_message(
                    "REMOTE SERVERS ERROR",
                    &[format!("Failed to retrieve servers from remote host: {}", e)],
                    true,
                )?;
                return Ok(());
            }
        };

        let width = get_content_width(80);

        if servers.is_empty() {
            let header = format!(
                "{}\r\n{}\r\n{}\r\n Host: {:<16} | {}@{}:{}\r\n No Craft servers currently registered on this remote host.\r\n{}",
                box_top(width).cyan().bold(),
                box_title(&format!("REMOTE SERVERS: {}", host_config.alias), width, false).cyan().bold(),
                box_divider(width).cyan().bold(),
                host_config.alias.white().bold(),
                host_config.user,
                host_config.host,
                host_config.port,
                box_divider(width).dimmed(),
            );

            let entries = vec![
                MenuEntry::new("0", "Back").with_aliases(&["b", "q"]),
            ];

            let mut empty_sel = 0;
            let _ = run_menu(&header, &entries, &mut empty_sel)?;
            return Ok(());
        }

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Host: {:<16} | {}@{}:{}\r\n Total Servers: {}\r\n Select a remote server to inspect details, control lifecycle, or view backups.\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("REMOTE SERVERS: {}", host_config.alias), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            host_config.alias.white().bold(),
            host_config.user,
            host_config.host,
            host_config.port,
            servers.len(),
            box_divider(width).dimmed(),
        );

        let mut entries = Vec::new();
        for (idx, s) in servers.iter().enumerate() {
            let hotkey = if idx < 9 {
                (idx + 1).to_string()
            } else {
                ((b'a' + (idx - 9) as u8) as char).to_string()
            };

            let status_badge = if s.is_running {
                if let Some(p) = s.pid {
                    format!("[RUNNING (PID: {})]", p).green().bold().to_string()
                } else {
                    "[RUNNING]".green().bold().to_string()
                }
            } else {
                "[STOPPED]".dimmed().to_string()
            };

            entries.push(MenuEntry::new(
                hotkey,
                format!(
                    "{:<20} {:<18} | {:<8} {:<8} | Port: {:<5}",
                    s.name, status_badge, s.server_type, s.version, s.port
                ),
            ));
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

        match run_menu(&header, &entries, &mut selected)? {
            Some(idx) if idx < servers.len() => {
                remote_server_control_panel(paths, &client, &servers[idx]).await?;
            }
            _ => return Ok(()),
        }
    }
}
