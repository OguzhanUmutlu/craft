use colored::Colorize;
use craft_core::{CraftPaths, Result};
use craft_remote::{RemoteCraftClient, RemoteServerInfo};
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, exec_console_action, get_content_width,
    print_in_place_status, run_menu, show_modal_message, MenuEntry,
};
use super::remote_backups::manage_remote_backups;

enum RemoteControlAction {
    ToggleStart,
    ToggleStop,
    Restart,
    AttachConsole,
    Backups,
    Back,
}

pub async fn remote_server_control_panel(
    paths: &CraftPaths,
    client: &RemoteCraftClient,
    server: &RemoteServerInfo,
) -> Result<()> {
    let mut selected = 0;

    loop {
        let (is_running, pid) = client.check_server_running(&server.name, &server.path);

        let status_badge = if is_running {
            if let Some(p) = pid {
                format!("[RUNNING (PID: {})]", p).green().bold().to_string()
            } else {
                "[RUNNING]".green().bold().to_string()
            }
        } else {
            "[STOPPED]".dimmed().to_string()
        };

        let width = get_content_width(80);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Server Name: {:<20} | Status: {}\r\n Server Type: {:<20} | Version: {}\r\n Network Port: {:<19} | Remote Host: {}\r\n Remote Path: {}\r\n{}",
            box_top(width).cyan().bold(),
            box_title(&format!("REMOTE SERVER CONTROL: {}", server.name), width, false).cyan().bold(),
            box_divider(width).cyan().bold(),
            server.name.white().bold(),
            status_badge,
            server.server_type.cyan(),
            server.version.cyan(),
            server.port.to_string().yellow(),
            client.session.config.alias.cyan().bold(),
            server.path.dimmed(),
            box_divider(width).dimmed(),
        );

        let mut menu_entries = Vec::new();
        let mut action_map = Vec::new();

        if is_running {
            // Dynamic Toggle: Stop button
            menu_entries.push(MenuEntry::new("1", "Stop Server").with_aliases(&["s"]));
            action_map.push(RemoteControlAction::ToggleStop);

            menu_entries.push(MenuEntry::new("2", "Restart Server").with_aliases(&["r"]));
            action_map.push(RemoteControlAction::Restart);

            menu_entries.push(MenuEntry::new("3", "Live Console").with_aliases(&["a", "v", "c"]));
            action_map.push(RemoteControlAction::AttachConsole);

            menu_entries.push(MenuEntry::new("4", "Backups").with_aliases(&["b"]));
            action_map.push(RemoteControlAction::Backups);
        } else {
            // Dynamic Toggle: Start button
            menu_entries.push(MenuEntry::new("1", "Start Server").with_aliases(&["s"]));
            action_map.push(RemoteControlAction::ToggleStart);

            menu_entries.push(MenuEntry::new("2", "Backups").with_aliases(&["b"]));
            action_map.push(RemoteControlAction::Backups);
        }

        menu_entries.push(MenuEntry::new("0", "Back").with_aliases(&["q"]));
        action_map.push(RemoteControlAction::Back);

        let selection = run_menu(&header, &menu_entries, &mut selected)?;

        let action = match selection {
            Some(idx) if idx < action_map.len() => &action_map[idx],
            _ => return Ok(()),
        };

        match action {
            RemoteControlAction::ToggleStart => {
                print_in_place_status(
                    "STARTING REMOTE SERVER",
                    &[format!("Starting '{}' on remote host in background...", server.name)],
                )?;

                match client.start_server(&server.name) {
                    Ok(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                    }
                    Err(e) => {
                        show_modal_message(
                            "START FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            RemoteControlAction::ToggleStop => {
                print_in_place_status(
                    "STOPPING REMOTE SERVER",
                    &[format!("Sending graceful stop command to '{}' on remote host...", server.name)],
                )?;

                match client.stop_server(&server.name) {
                    Ok(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                    }
                    Err(e) => {
                        show_modal_message(
                            "STOP FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            RemoteControlAction::Restart => {
                print_in_place_status(
                    "RESTARTING REMOTE SERVER",
                    &[format!("Restarting '{}' on remote host...", server.name)],
                )?;

                match client.restart_server(&server.name) {
                    Ok(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
                    }
                    Err(e) => {
                        show_modal_message(
                            "RESTART FAILED",
                            &[format!("[ERROR] {}", e)],
                            true,
                        )?;
                    }
                }
            }
            RemoteControlAction::AttachConsole => {
                let cmd = format!("craft view {}", server.name);
                let _ = exec_console_action(|| async {
                    let _ = craft_remote::run_remote_pty_session(&client.session, &cmd);
                    Ok(())
                }).await;

            }
            RemoteControlAction::Backups => {
                manage_remote_backups(paths, client, server).await?;
            }
            RemoteControlAction::Back => return Ok(()),
        }
    }
}
