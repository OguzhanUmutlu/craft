pub mod host_servers;
pub mod remote_control;
pub mod remote_backups;

use std::path::PathBuf;
use colored::Colorize;
use craft_core::{CraftPaths, RemoteAuthType, RemoteHostConfig, RemotesRegistry, Result};
use crate::commands::dashboard::screen::{
    box_divider, box_title, box_top, get_content_width, run_input_prompt,
    run_menu, run_paged_list_menu, run_password_prompt, show_modal_message, AltScreenGuard, MenuEntry,
    PagedMenuAction,
};
use host_servers::manage_host_servers;

pub async fn remote_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut current_page = 0;
    let page_size = 7;

    loop {
        let registry = RemotesRegistry::load(paths)?;
        let width = get_content_width(80);

        let mut action_entries = vec![
            MenuEntry::new("i", "Import SSH Hosts").with_aliases(&["import"]),
            MenuEntry::new("a", "Add Remote Host").with_aliases(&["add", "n"]),
        ];
        if !registry.remotes.is_empty() {
            action_entries.push(MenuEntry::new("r", "Remove Host").with_aliases(&["rm", "del"]));
        }

        let action = run_paged_list_menu(
            &registry.remotes,
            &mut current_page,
            page_size,
            |page, total_pages, total_count| {
                let count_str = if total_count == 0 {
                    "No remote hosts configured yet.".dimmed().to_string()
                } else {
                    format!("Configured Hosts: {}", total_count).cyan().bold().to_string()
                };
                let page_info = if total_pages > 1 {
                    format!(" | Page {} of {}", page, total_pages).cyan().to_string()
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
                format!("{:<18} ({}@{}:{}) {}", r.alias, r.user, r.host, r.port, auth_tag.dimmed())
            },
            &action_entries,
            false,
        )?;

        match action {
            PagedMenuAction::Select(global_idx) if global_idx < registry.remotes.len() => {
                let host = &registry.remotes[global_idx];
                manage_host_servers(paths, host).await?;
            }
            PagedMenuAction::Action(act) if act == "i" => {
                    // Import from ~/.ssh/config
                    let detected = craft_remote::discover_ssh_hosts();
                    if detected.is_empty() {
                        show_modal_message(
                            "NO SSH CONFIG HOSTS FOUND",
                            &[
                                "No host definitions found in ~/.ssh/config.".to_string(),
                                "You can add a host manually using option [a].".to_string(),
                            ],
                            false,
                        )?;
                        continue;
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
                            let alias_prompt = format!("Enter alias for '{}' :", candidate.alias);
                            let chosen_alias = match run_input_prompt(
                                "HOST ALIAS",
                                &alias_prompt,
                                Some(&candidate.alias),
                            )? {
                                Some(a) if !a.trim().is_empty() => a.trim().to_string(),
                                _ => candidate.alias.clone(),
                            };

                            let mut reg = RemotesRegistry::load(paths)?;
                            if reg.find(&chosen_alias).is_some() {
                                show_modal_message(
                                    "ALIAS ALREADY EXISTS",
                                    &[format!("A remote host with alias '{}' already exists.", chosen_alias)],
                                    true,
                                )?;
                                continue;
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
                }
            PagedMenuAction::Action(act) if act == "a" => {
                // Add Host Manually
                let alias = match run_input_prompt("NEW REMOTE HOST", "Host Alias (e.g. production-vps):", None)? {
                    Some(a) if !a.trim().is_empty() => a.trim().to_string(),
                    _ => continue,
                };

                let host_addr = match run_input_prompt("HOST ADDRESS", "IP Address or Hostname:", None)? {
                    Some(h) if !h.trim().is_empty() => h.trim().to_string(),
                    _ => continue,
                };

                let port = match run_input_prompt("SSH PORT", "SSH Port:", Some("22"))? {
                    Some(p) => p.trim().parse::<u16>().unwrap_or(22),
                    None => 22,
                };

                let default_user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
                let user = match run_input_prompt("SSH USER", "SSH Username:", Some(&default_user))? {
                    Some(u) if !u.trim().is_empty() => u.trim().to_string(),
                    _ => default_user,
                };

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
                        let kp = run_input_prompt("SSH KEY PATH", "Path to private key:", Some("~/.ssh/id_ed25519"))?;
                        let key = kp.map(|k| PathBuf::from(k.trim()));
                        (RemoteAuthType::Key, key, None)
                    }
                    Some(1) => {
                        let pass = run_password_prompt("SSH PASSWORD", "Enter SSH password:")?;
                        (RemoteAuthType::Password, None, pass)
                    }
                    Some(2) => {
                        (RemoteAuthType::Agent, None, None)
                    }
                    _ => continue,
                };

                let mut reg = RemotesRegistry::load(paths)?;
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
            }
            PagedMenuAction::Action(act) if act == "r" && !registry.remotes.is_empty() => {
                // Remove Host
                let rm_header = " Select host to remove:";
                let mut rm_entries = Vec::new();
                for (i, h) in registry.remotes.iter().enumerate() {
                    let hk = (i + 1).to_string();
                    rm_entries.push(MenuEntry::new(hk, h.alias.clone()));
                }
                rm_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b"]));

                let mut rm_sel = 0;
                if let Some(r_idx) = run_menu(rm_header, &rm_entries, &mut rm_sel)? {
                    if r_idx < registry.remotes.len() {
                        let to_remove = &registry.remotes[r_idx];
                        let mut reg = RemotesRegistry::load(paths)?;
                        reg.remove(&to_remove.alias);
                        reg.save(paths)?;

                        show_modal_message(
                            "HOST REMOVED",
                            &[format!("[OK] Host '{}' removed from registry.", to_remove.alias)
                                .green()
                                .bold()
                                .to_string()],
                            false,
                        )?;
                    }
                }
            }
            PagedMenuAction::Back => return Ok(()),
            _ => {}
        }
    }
}
