pub mod host_servers;
pub mod remote_backups;
pub mod remote_control;

use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, print_in_place_status, run_input_prompt,
    run_menu, run_paged_list_menu_ext, run_password_prompt, run_titled_menu_with_shortcuts,
    show_modal_message, AltScreenGuard, ConfirmModal, ConfirmOutcome, FormField, FormModal,
    MenuEntry, NavGuard, PagedMenuAction,
};
use colored::Colorize;
use craft_core::{CraftPaths, RemoteAuthType, RemoteHostConfig, RemotesRegistry, Result};
pub use host_servers::{connect_with_cancellation, manage_host_servers};
use std::path::PathBuf;

pub async fn remote_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let _nav = NavGuard::enter("Remote Hosts");
    let mut current_page = 0;
    let page_size = 7;

    loop {
        let registry = RemotesRegistry::load(paths)?;
        let width = get_content_width(80);

        // Exactly one extra button at the end: New Host
        let action_entries =
            vec![MenuEntry::new("n", "New Host").with_aliases(&["add", "a", "new", "+"])];

        let action = run_paged_list_menu_ext(
            &registry.remotes,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "No remote hosts configured yet.".dimmed().to_string()
                } else {
                    format!("Configured Hosts: {}", total_count)
                        .cyan()
                        .bold()
                        .to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages)
                        .cyan()
                        .to_string()
                } else {
                    "".to_string()
                };

                format!(
                    "{}\r\n{}\r\n{}\r\n Connect to remote VPS/cloud hosts over SSH to inspect servers and backups.\r\n {}{}\r\n{}",
                    box_top(width).cyan().bold(),
                    box_title("REMOTE SERVERS & HOSTS (SSH)", width, false).cyan().bold(),
                    box_divider(width).cyan().bold(),
                    count_str,
                    page_info,
                    box_divider(width).dimmed(),
                )
            },
            |_local_idx, _global_idx, r| {
                let auth_tag = match r.auth_type {
                    RemoteAuthType::Key => "[KEY]",
                    RemoteAuthType::Agent => "[AGENT]",
                    RemoteAuthType::Password => "[PASS]",
                };
                format!(
                    "{:<18} ({}@{}:{}) {}",
                    r.alias,
                    r.user,
                    r.host,
                    r.port,
                    auth_tag.dimmed()
                )
            },
            &action_entries,
            false,
            &['e'],
            Some(
                &modal_tui::Shortcuts::new()
                    .move_selection()
                    .add("Enter/→", "Connect")
                    .add("e", "Edit Offline")
                    .back()
                    .to_footer_string(),
            ),
        )?;

        match action {
            PagedMenuAction::Select(global_idx) if global_idx < registry.remotes.len() => {
                let host = &registry.remotes[global_idx];
                manage_host_servers(paths, host).await?;
            }
            PagedMenuAction::ItemAction('e', global_idx) if global_idx < registry.remotes.len() => {
                let host = &registry.remotes[global_idx];
                offline_host_actions_menu(paths, &host.alias).await?;
            }
            PagedMenuAction::Action(act) if act == "n" || act == "a" || act == "+" => {
                new_host_flow(paths).await?;
            }
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}

/// Branches New Host creation into Create Manually or Import from SSH config.
async fn new_host_flow(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("New Host");
    let header = " Select how you want to add the new remote host:";
    let entries = vec![
        MenuEntry::new("1", "Create Manually").with_aliases(&["manual", "m", "c"]),
        MenuEntry::new("2", "Import from ~/.ssh/config").with_aliases(&["import", "ssh", "i"]),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];

    let mut sel = 0;
    match run_menu(header, &entries, &mut sel)? {
        Some(0) => create_host_manually_flow(paths).await?,
        Some(1) => import_host_from_ssh_flow(paths).await?,
        _ => {}
    }
    Ok(())
}

/// Creates a new host manually using a unified multi-field `FormModal`.
async fn create_host_manually_flow(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Create Manually");
    let default_user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());

    let mut form = FormModal::new("NEW REMOTE HOST")
        .with_header_row("Configure connection details for the remote SSH host.")
        .with_field(
            FormField::string("alias", "Host Alias")
                .with_placeholder("e.g. production-vps")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Alias cannot be empty".to_string())
                    } else if s.contains(' ') {
                        Err("Alias cannot contain spaces".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .with_field(
            FormField::string("host", "IP Address or Hostname")
                .with_placeholder("e.g. 192.168.1.50 or vps.example.com")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Host address cannot be empty".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .with_field(
            FormField::integer("port", "SSH Port")
                .with_default("22")
                .with_placeholder("22")
                .with_validator(|s| match s.trim().parse::<u16>() {
                    Ok(p) if p > 0 => Ok(()),
                    _ => Err("Invalid port number (1-65535)".to_string()),
                }),
        )
        .with_field(
            FormField::string("user", "SSH Username")
                .with_default(&default_user)
                .with_placeholder("root")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Username cannot be empty".to_string())
                    } else {
                        Ok(())
                    }
                }),
        );

    let form_result = match form.run()? {
        Some(r) => r,
        None => return Ok(()),
    };

    let alias = form_result.get_string("alias").trim().to_string();
    let host_addr = form_result.get_string("host").trim().to_string();
    let port = form_result.get_u16("port").unwrap_or(22);
    let user = form_result.get_string("user").trim().to_string();

    let mut reg = RemotesRegistry::load(paths)?;
    if reg.find(&alias).is_some() {
        show_modal_message(
            "ALIAS ALREADY EXISTS",
            &[format!(
                "A remote host with alias '{}' already exists.",
                alias
            )],
            true,
        )?;
        return Ok(());
    }

    let auth_header = " Choose authentication method:";
    let auth_entries = vec![
        MenuEntry::new("1", "SSH Private Key"),
        MenuEntry::new("2", "Password"),
        MenuEntry::new("3", "SSH Agent"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b"]),
    ];
    let mut a_sel = 0;
    let (auth_type, key_path, password) = match run_menu(auth_header, &auth_entries, &mut a_sel)? {
        Some(0) => {
            let kp = run_input_prompt(
                "SSH KEY PATH",
                "Path to private key:",
                Some("~/.ssh/id_ed25519"),
            )?;
            let key = kp.map(|k| PathBuf::from(k.trim()));
            (RemoteAuthType::Key, key, None)
        }
        Some(1) => {
            let pass = run_password_prompt("SSH PASSWORD", "Enter SSH password:")?;
            (RemoteAuthType::Password, None, pass)
        }
        Some(2) => (RemoteAuthType::Agent, None, None),
        _ => return Ok(()),
    };

    reg.add(RemoteHostConfig {
        alias: alias.clone(),
        host: host_addr,
        port,
        user,
        auth_type,
        key_path,
        password,
        remote_dir: None,
        os_type: None,
    })?;
    reg.save(paths)?;

    show_modal_message(
        "HOST ADDED",
        &[format!("[OK] Remote host '{}' added successfully!", alias)
            .green()
            .bold()
            .to_string()],
        false,
    )?;

    Ok(())
}

/// Imports host definition from ~/.ssh/config.
async fn import_host_from_ssh_flow(paths: &CraftPaths) -> Result<()> {
    let _nav = NavGuard::enter("Import SSH");
    let detected = craft_remote::discover_ssh_hosts();
    if detected.is_empty() {
        show_modal_message(
            "NO SSH CONFIG HOSTS FOUND",
            &[
                "No host definitions found in ~/.ssh/config.".to_string(),
                "You can add a host manually using 'Create Manually'.".to_string(),
            ],
            false,
        )?;
        return Ok(());
    }

    let import_header = " Select host from ~/.ssh/config to import:";
    let mut import_entries = Vec::new();
    for (i, h) in detected.iter().enumerate() {
        let hk = (i + 1).to_string();
        import_entries.push(MenuEntry::new(
            hk,
            format!("{:<16} ({}@{}:{})", h.alias, h.user, h.host, h.port),
        ));
    }
    import_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

    let mut imp_sel = 0;
    if let Some(i_idx) = run_menu(import_header, &import_entries, &mut imp_sel)? {
        if i_idx < detected.len() {
            let candidate = &detected[i_idx];
            let alias_prompt = format!("Enter alias for '{}':", candidate.alias);
            let chosen_alias =
                match run_input_prompt("HOST ALIAS", &alias_prompt, Some(&candidate.alias))? {
                    Some(a) if !a.trim().is_empty() => a.trim().to_string(),
                    _ => candidate.alias.clone(),
                };

            let mut reg = RemotesRegistry::load(paths)?;
            if reg.find(&chosen_alias).is_some() {
                show_modal_message(
                    "ALIAS ALREADY EXISTS",
                    &[format!(
                        "A remote host with alias '{}' already exists.",
                        chosen_alias
                    )],
                    true,
                )?;
                return Ok(());
            }

            let mut new_host = candidate.clone();
            new_host.alias = chosen_alias.clone();
            reg.add(new_host)?;
            reg.save(paths)?;

            show_modal_message(
                "HOST IMPORTED",
                &[format!(
                    "[OK] Remote host '{}' successfully imported from ~/.ssh/config!",
                    chosen_alias
                )
                .green()
                .bold()
                .to_string()],
                false,
            )?;
        }
    }
    Ok(())
}

/// Offline actions menu for a specific host, accessed via pressing 'e'.
async fn offline_host_actions_menu(paths: &CraftPaths, host_alias: &str) -> Result<()> {
    loop {
        let registry = RemotesRegistry::load(paths)?;
        let host = match registry.find(host_alias) {
            Some(h) => h.clone(),
            None => return Ok(()), // Host was removed
        };

        let _nav_host = NavGuard::enter(&host.alias);
        let _nav_actions = NavGuard::enter("Offline Actions");
        let auth_tag = match host.auth_type {
            RemoteAuthType::Key => "[KEY]",
            RemoteAuthType::Agent => "[AGENT]",
            RemoteAuthType::Password => "[PASS]",
        };

        let entries = vec![
            MenuEntry::new("a", "Authentication").with_aliases(&["auth", "edit", "e", "1"]),
            MenuEntry::new("p", "Ping").with_aliases(&["test", "t", "2"]),
            MenuEntry::new("u", "Uninstall").with_aliases(&["un", "3"]),
            MenuEntry::new("r", "Remove").with_aliases(&["rm", "del", "delete", "4"]),
            MenuEntry::new("0", "Back").with_aliases(&["b"]),
        ];

        let mut sel = 0;
        let shortcuts = modal_tui::Shortcuts::new()
            .move_selection()
            .select()
            .back()
            .exit_if(NavGuard::depth() <= 1);
        let choice = run_titled_menu_with_shortcuts(
            format!("OFFLINE HOST ACTIONS: {}", host.alias.to_uppercase()),
            &[
                format!(
                    "Host: {}@{}:{} {}",
                    host.user,
                    host.host,
                    host.port,
                    auth_tag.cyan()
                ),
                format!("Offline Host Actions for '{}':", host.alias.yellow().bold()),
            ],
            &entries,
            &mut sel,
            shortcuts,
        )?;

        match choice {
            Some(0) => {
                edit_host_authentication_flow(paths, &host).await?;
            }
            Some(1) => {
                ping_host_flow(&host).await?;
            }
            Some(2) => {
                uninstall_host_flow(&host).await?;
            }
            Some(3) => {
                if remove_host_flow(paths, &host)? {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    }
}

/// Multi-field FormModal to view and edit host authentication and connection details.
async fn edit_host_authentication_flow(paths: &CraftPaths, host: &RemoteHostConfig) -> Result<()> {
    let _nav = NavGuard::enter("Authentication");

    let auth_str = match host.auth_type {
        RemoteAuthType::Key => "key",
        RemoteAuthType::Password => "password",
        RemoteAuthType::Agent => "agent",
    };

    let key_default = host
        .key_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.ssh/id_ed25519".to_string());

    let mut form = FormModal::new(format!("AUTHENTICATION: {}", host.alias))
        .with_header_row(format!(
            "Edit connection and authentication settings for '{}'.",
            host.alias
        ))
        .with_field(
            FormField::string("alias", "Host Alias")
                .with_default(&host.alias)
                .with_placeholder("alias")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Alias cannot be empty".to_string())
                    } else if s.contains(' ') {
                        Err("Alias cannot contain spaces".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .with_field(
            FormField::string("host", "SSH Host / IP")
                .with_default(&host.host)
                .with_placeholder("192.168.1.100")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Host address cannot be empty".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .with_field(
            FormField::integer("port", "SSH Port")
                .with_default(host.port.to_string())
                .with_placeholder("22")
                .with_validator(|s| match s.trim().parse::<u16>() {
                    Ok(p) if p > 0 => Ok(()),
                    _ => Err("Invalid port number (1-65535)".to_string()),
                }),
        )
        .with_field(
            FormField::string("user", "SSH Username")
                .with_default(&host.user)
                .with_placeholder("root")
                .with_validator(|s| {
                    if s.trim().is_empty() {
                        Err("Username cannot be empty".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .with_field(
            FormField::string("auth_type", "Auth Type [key / password / agent]")
                .with_default(auth_str)
                .with_placeholder("key")
                .with_validator(|s| {
                    let clean = s.trim().to_ascii_lowercase();
                    if clean == "key" || clean == "password" || clean == "agent" {
                        Ok(())
                    } else {
                        Err("Must be 'key', 'password', or 'agent'".to_string())
                    }
                }),
        )
        .with_field(
            FormField::string("key_path", "Private Key Path (for 'key' auth)")
                .with_default(key_default)
                .with_placeholder("~/.ssh/id_ed25519"),
        )
        .with_field(
            FormField::password("password", "Password (for 'password' auth)")
                .with_default(host.password.clone().unwrap_or_default())
                .with_placeholder("SSH Password"),
        );

    let form_result = match form.run()? {
        Some(r) => r,
        None => return Ok(()),
    };

    let new_alias = form_result.get_string("alias").trim().to_string();
    let new_host = form_result.get_string("host").trim().to_string();
    let new_port = form_result.get_u16("port").unwrap_or(22);
    let new_user = form_result.get_string("user").trim().to_string();
    let new_auth_str = form_result
        .get_string("auth_type")
        .trim()
        .to_ascii_lowercase();
    let new_key_path = form_result.get_string("key_path").trim().to_string();
    let new_password = form_result.get_string("password");

    let (new_auth_type, key_opt, pass_opt) = match new_auth_str.as_str() {
        "password" => (
            RemoteAuthType::Password,
            None,
            if new_password.is_empty() {
                None
            } else {
                Some(new_password)
            },
        ),
        "agent" => (RemoteAuthType::Agent, None, None),
        _ => (
            RemoteAuthType::Key,
            if new_key_path.is_empty() {
                None
            } else {
                Some(PathBuf::from(new_key_path))
            },
            None,
        ),
    };

    let mut reg = RemotesRegistry::load(paths)?;
    if new_alias != host.alias && reg.find(&new_alias).is_some() {
        show_modal_message(
            "ALIAS ALREADY EXISTS",
            &[format!(
                "A remote host with alias '{}' already exists.",
                new_alias
            )],
            true,
        )?;
        return Ok(());
    }

    reg.remove(&host.alias);
    reg.add(RemoteHostConfig {
        alias: new_alias.clone(),
        host: new_host,
        port: new_port,
        user: new_user,
        auth_type: new_auth_type,
        key_path: key_opt,
        password: pass_opt,
        remote_dir: host.remote_dir.clone(),
        os_type: host.os_type,
    })?;
    reg.save(paths)?;

    show_modal_message(
        "AUTHENTICATION UPDATED",
        &[format!(
            "[OK] Host '{}' authentication updated successfully!",
            new_alias
        )
        .green()
        .bold()
        .to_string()],
        false,
    )?;

    Ok(())
}

/// Tests SSH connectivity and responsiveness.
async fn ping_host_flow(host: &RemoteHostConfig) -> Result<()> {
    if let Some(_client) = connect_with_cancellation(host).await? {
        show_modal_message(
            "PING SUCCESSFUL",
            &[
                format!(
                    "[OK] Successfully established SSH connection to '{}' ({}@{}:{}).",
                    host.alias, host.user, host.host, host.port
                )
                .green()
                .bold()
                .to_string(),
                "Remote host is online and responsive.".to_string(),
            ],
            false,
        )?;
    }
    Ok(())
}

/// Runs the remote Craft uninstaller wizard for the target host after prompt.
async fn uninstall_host_flow(host: &RemoteHostConfig) -> Result<()> {
    let confirm = ConfirmModal::new(
        "UNINSTALL CRAFT",
        format!(
            "Are you sure you want to uninstall Craft from remote host '{}'?",
            host.alias
        ),
    )
    .as_danger()
    .with_detail(format!(
        "This will connect to {}@{}:{} and remove Craft daemons and CLI binaries.",
        host.user, host.host, host.port
    ))
    .with_yes_label("Uninstall")
    .with_no_label("Cancel")
    .default_yes(false)
    .run()?;

    if confirm != ConfirmOutcome::Confirmed {
        return Ok(());
    }

    if let Some(client) = connect_with_cancellation(host).await? {
        let _ = print_in_place_status(
            "UNINSTALLING CRAFT",
            &[format!(
                "Uninstalling Craft and daemon from '{}'...",
                host.alias
            )],
        );
        match client.uninstall_craft() {
            Ok(_) => {
                show_modal_message(
                    "CRAFT UNINSTALLED",
                    &[
                        format!(
                            "[OK] Craft has been successfully uninstalled from remote host '{}'.",
                            host.alias
                        )
                        .green()
                        .bold()
                        .to_string(),
                        "Daemon services stopped and removed from ~/.local/bin/craft.".to_string(),
                    ],
                    false,
                )?;
            }
            Err(e) => {
                show_modal_message(
                    "UNINSTALL FAILED",
                    &[format!(
                        "Failed to uninstall Craft from '{}': {}",
                        host.alias, e
                    )],
                    true,
                )?;
            }
        }
    }
    Ok(())
}

/// Prompts confirmation to remove host from local Craft registry.
fn remove_host_flow(paths: &CraftPaths, host: &RemoteHostConfig) -> Result<bool> {
    let confirm = ConfirmModal::new(
        "REMOVE HOST",
        format!(
            "Are you sure you want to remove '{}' ({}@{}:{}) from the Craft registry?",
            host.alias, host.user, host.host, host.port
        ),
    )
    .default_yes(false)
    .run()?;

    if confirm == ConfirmOutcome::Confirmed {
        let mut reg = RemotesRegistry::load(paths)?;
        reg.remove(&host.alias);
        reg.save(paths)?;

        show_modal_message(
            "HOST REMOVED",
            &[format!("[OK] Host '{}' removed from registry.", host.alias)
                .green()
                .bold()
                .to_string()],
            false,
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}
