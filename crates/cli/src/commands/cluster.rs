use crate::cli::ClusterCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    is_server_locked, get_server_running_pid,
    ClusterNode, ClusterRole, ClustersRegistry, CraftError, CraftPaths,
    RemotesRegistry, Result, ServerCluster, ServersRegistry,
};
use craft_daemon::DaemonClient;
use craft_remote::RemoteCraftClient;
use std::fs;
use std::path::PathBuf;

pub async fn handle_cluster(action: ClusterCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        ClusterCommands::Create { name, proxy } => handle_create(&name, proxy, paths),
        ClusterCommands::Add {
            cluster,
            server,
            role,
            remote,
            depends_on,
        } => handle_add(&cluster, &server, &role, remote, depends_on, paths),
        ClusterCommands::Remove { cluster, server } => handle_remove(&cluster, &server, paths),
        ClusterCommands::Ls => handle_ls(paths),
        ClusterCommands::Status { cluster } => handle_status(&cluster, paths).await,
        ClusterCommands::Start { cluster } => handle_start(&cluster, paths).await,
        ClusterCommands::Stop { cluster } => handle_stop(&cluster, paths).await,
        ClusterCommands::SyncRouting { cluster, dry_run } => {
            handle_sync_routing(&cluster, dry_run, paths).await
        }
        ClusterCommands::Delete { cluster } => handle_delete(&cluster, paths),
    }
}

fn handle_create(name: &str, proxy: Option<String>, paths: &CraftPaths) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CraftError::Other(
            "Cluster name cannot be empty. Specify a valid name.".to_string(),
        ));
    }

    ClustersRegistry::modify(paths, |reg| {
        let mut cluster = ServerCluster::new(name);
        if let Some(ref p) = proxy {
            cluster.add_node(ClusterNode {
                name: p.clone(),
                role: ClusterRole::Proxy,
                remote: None,
                depends_on: Vec::new(),
            })?;
        }
        reg.add_cluster(cluster)?;
        Ok(())
    })?;

    println!(
        "{}",
        format!("[OK] Cluster '{}' created successfully.", name)
            .green()
            .bold()
    );
    if let Some(p) = proxy {
        println!("  {:<16} {}", "Proxy Router:".dimmed(), p);
    }
    println!(
        "{}",
        format!("Add servers to this cluster using: craft cluster add {} <server> --role [backend|proxy|lobby]", name).dimmed()
    );
    Ok(())
}

fn handle_add(
    cluster_name: &str,
    server: &str,
    role_str: &str,
    remote: Option<String>,
    depends_on: Vec<String>,
    paths: &CraftPaths,
) -> Result<()> {
    let role = role_str.parse::<ClusterRole>()?;

    ClustersRegistry::modify(paths, |reg| {
        let cluster = reg.get_cluster_mut(cluster_name).ok_or_else(|| {
            CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
        })?;

        cluster.add_node(ClusterNode {
            name: server.to_string(),
            role,
            remote: remote.clone(),
            depends_on,
        })?;
        Ok(())
    })?;

    println!(
        "{}",
        format!(
            "[OK] Added server '{}' to cluster '{}' (role: {}).",
            server, cluster_name, role
        )
        .green()
        .bold()
    );
    if let Some(ref r) = remote {
        println!("  {:<16} {}", "Remote Host:".dimmed(), r);
    }
    Ok(())
}

fn handle_remove(cluster_name: &str, server: &str, paths: &CraftPaths) -> Result<()> {
    ClustersRegistry::modify(paths, |reg| {
        let cluster = reg.get_cluster_mut(cluster_name).ok_or_else(|| {
            CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
        })?;

        if !cluster.remove_node(server) {
            return Err(CraftError::Other(format!(
                "Server '{}' is not a member of cluster '{}'.",
                server, cluster_name
            )));
        }
        Ok(())
    })?;

    println!(
        "{}",
        format!(
            "[OK] Removed server '{}' from cluster '{}'.",
            server, cluster_name
        )
        .green()
        .bold()
    );
    Ok(())
}

fn handle_ls(paths: &CraftPaths) -> Result<()> {
    let registry = ClustersRegistry::load(paths)?;
    if registry.clusters.is_empty() {
        println!(
            "{}",
            "No clusters configured. Use 'craft cluster create <name>' to create one.".yellow()
        );
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("CLUSTER").fg(Color::Cyan),
            Cell::new("NODES").fg(Color::Cyan),
            Cell::new("PROXIES").fg(Color::Cyan),
            Cell::new("BACKENDS").fg(Color::Cyan),
            Cell::new("LOBBIES").fg(Color::Cyan),
            Cell::new("STARTUP ORDER").fg(Color::Cyan),
        ]);

    for cluster in &registry.clusters {
        let proxies: Vec<String> = cluster.proxies().iter().map(|n| n.name.clone()).collect();
        let backends: Vec<String> = cluster.backends().iter().map(|n| n.name.clone()).collect();
        let lobbies: Vec<String> = cluster.lobbies().iter().map(|n| n.name.clone()).collect();

        let startup_summary = match cluster.resolve_startup_order() {
            Ok(order) => order.join(" -> "),
            Err(e) => format!("Error: {}", e),
        };

        table.add_row(Row::from(vec![
            Cell::new(&cluster.name).fg(Color::White),
            Cell::new(cluster.nodes.len().to_string()),
            Cell::new(if proxies.is_empty() {
                "-".to_string()
            } else {
                proxies.join(", ")
            }),
            Cell::new(if backends.is_empty() {
                "-".to_string()
            } else {
                backends.join(", ")
            }),
            Cell::new(if lobbies.is_empty() {
                "-".to_string()
            } else {
                lobbies.join(", ")
            }),
            Cell::new(startup_summary),
        ]));
    }

    println!("{table}");
    Ok(())
}

async fn handle_status(cluster_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ClustersRegistry::load(paths)?;
    let cluster = registry.get_cluster(cluster_name).ok_or_else(|| {
        CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
    })?;

    println!(
        "{}",
        format!("=== Cluster Status: {} ===", cluster.name)
            .bold()
            .cyan()
    );

    let startup_order = cluster.resolve_startup_order()?;
    let shutdown_order = cluster.resolve_shutdown_order()?;

    println!("  {:<20} {}", "Startup Order:".dimmed(), startup_order.join(" -> "));
    println!("  {:<20} {}", "Shutdown Order:".dimmed(), shutdown_order.join(" -> "));
    println!();

    let local_reg = ServersRegistry::load(paths).unwrap_or_default();
    let remotes_reg = RemotesRegistry::load(paths).unwrap_or_default();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("NODE").fg(Color::Cyan),
            Cell::new("ROLE").fg(Color::Cyan),
            Cell::new("HOST").fg(Color::Cyan),
            Cell::new("DEPENDS ON").fg(Color::Cyan),
            Cell::new("STATUS").fg(Color::Cyan),
        ]);

    for node in &cluster.nodes {
        let host_display = node
            .remote
            .as_deref()
            .map(|r| format!("Remote ({})", r))
            .unwrap_or_else(|| "Local".to_string());

        let deps_display = if node.depends_on.is_empty() {
            "-".to_string()
        } else {
            node.depends_on.join(", ")
        };

        let is_running = if let Some(ref remote_alias) = node.remote {
            if let Some(rcfg) = remotes_reg.find(remote_alias) {
                if let Ok(client) = RemoteCraftClient::connect(rcfg) {
                    let remote_path = format!("~/.craft/servers/{}", node.name);
                    let (running, _) = client.check_server_running(&node.name, &remote_path);
                    running
                } else {
                    false
                }
            } else {
                false
            }
        } else if let Some(srv) = local_reg.find_by_name(&node.name) {
            is_server_locked(&srv.path) || get_server_running_pid(&srv.path).is_some()
        } else {
            false
        };

        let status_cell = if is_running {
            Cell::new("[RUNNING]").fg(Color::Green)
        } else {
            Cell::new("[STOPPED]").fg(Color::DarkGrey)
        };

        table.add_row(Row::from(vec![
            Cell::new(&node.name),
            Cell::new(node.role.to_string()),
            Cell::new(host_display),
            Cell::new(deps_display),
            status_cell,
        ]));
    }

    println!("{table}");
    Ok(())
}

async fn handle_start(cluster_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ClustersRegistry::load(paths)?;
    let cluster = registry.get_cluster(cluster_name).ok_or_else(|| {
        CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
    })?;

    let startup_order = cluster.resolve_startup_order()?;
    if startup_order.is_empty() {
        println!("{}", "No nodes in cluster to start.".yellow());
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "[CLUSTER START] Starting cluster '{}' in topological DAG order: {}",
            cluster.name,
            startup_order.join(" -> ")
        )
        .bold()
        .cyan()
    );

    let local_reg = ServersRegistry::load(paths)?;
    let remotes_reg = RemotesRegistry::load(paths)?;

    for node_name in startup_order {
        let node = cluster.find_node(&node_name).unwrap();

        if let Some(ref remote_alias) = node.remote {
            let rcfg = remotes_reg.find(remote_alias).ok_or_else(|| {
                CraftError::Config(format!(
                    "Remote alias '{}' for node '{}' not found in remotes.toml",
                    remote_alias, node_name
                ))
            })?;
            println!(
                "  {} Starting remote node '{}' on '{}' (role: {})...",
                "->".blue().bold(),
                node.name,
                remote_alias,
                node.role
            );
            let client = RemoteCraftClient::connect(rcfg)?;
            client.start_server(&node.name)?;
            println!(
                "  {} Node '{}' started on remote supervisor.",
                "[OK]".green().bold(),
                node.name
            );
        } else {
            let srv = local_reg.find_by_name(&node.name).ok_or_else(|| {
                CraftError::Config(format!(
                    "Local server '{}' not found in servers.toml",
                    node.name
                ))
            })?;

            if is_server_locked(&srv.path) || get_server_running_pid(&srv.path).is_some() {
                println!(
                    "  {} Node '{}' is already running.",
                    "[SKIP]".yellow().bold(),
                    node.name
                );
                continue;
            }

            println!(
                "  {} Starting local node '{}' (role: {})...",
                "->".blue().bold(),
                node.name,
                node.role
            );
            DaemonClient::ensure_daemon_started(paths).await?;
            let mut client = DaemonClient::connect(paths).await?;
            client.start_server(&srv.path).await?;
            println!(
                "  {} Node '{}' started on local daemon.",
                "[OK]".green().bold(),
                node.name
            );
        }
    }

    println!(
        "{}",
        format!("[SUCCESS] Cluster '{}' started successfully.", cluster.name)
            .green()
            .bold()
    );
    Ok(())
}

async fn handle_stop(cluster_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ClustersRegistry::load(paths)?;
    let cluster = registry.get_cluster(cluster_name).ok_or_else(|| {
        CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
    })?;

    let shutdown_order = cluster.resolve_shutdown_order()?;
    if shutdown_order.is_empty() {
        println!("{}", "No nodes in cluster to stop.".yellow());
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "[CLUSTER STOP] Stopping cluster '{}' in reverse dependency order: {}",
            cluster.name,
            shutdown_order.join(" -> ")
        )
        .bold()
        .cyan()
    );

    let local_reg = ServersRegistry::load(paths)?;
    let remotes_reg = RemotesRegistry::load(paths)?;

    for node_name in shutdown_order {
        let node = cluster.find_node(&node_name).unwrap();

        if let Some(ref remote_alias) = node.remote {
            let rcfg = remotes_reg.find(remote_alias).ok_or_else(|| {
                CraftError::Config(format!(
                    "Remote alias '{}' for node '{}' not found in remotes.toml",
                    remote_alias, node_name
                ))
            })?;
            println!(
                "  {} Stopping remote node '{}' on '{}'...",
                "->".blue().bold(),
                node.name,
                remote_alias
            );
            let client = RemoteCraftClient::connect(rcfg)?;
            let _ = client.stop_server(&node.name);
            println!(
                "  {} Node '{}' stopped on remote.",
                "[OK]".green().bold(),
                node.name
            );
        } else if let Some(srv) = local_reg.find_by_name(&node.name) {
            if !is_server_locked(&srv.path) && get_server_running_pid(&srv.path).is_none() {
                println!(
                    "  {} Node '{}' is already stopped.",
                    "[SKIP]".yellow().bold(),
                    node.name
                );
                continue;
            }

            println!("  {} Stopping local node '{}'...", "->".blue().bold(), node.name);
            if DaemonClient::is_daemon_running(paths) {
                if let Ok(mut client) = DaemonClient::connect(paths).await {
                    let _ = client.stop_server(&srv.path, false).await;
                }
            } else if let Some(pid) = get_server_running_pid(&srv.path) {
                let _ = craft_core::kill_process(pid, false);
            }
            println!(
                "  {} Node '{}' stopped on local host.",
                "[OK]".green().bold(),
                node.name
            );
        }
    }

    println!(
        "{}",
        format!("[SUCCESS] Cluster '{}' stopped successfully.", cluster.name)
            .green()
            .bold()
    );
    Ok(())
}

async fn handle_sync_routing(cluster_name: &str, dry_run: bool, paths: &CraftPaths) -> Result<()> {
    let registry = ClustersRegistry::load(paths)?;
    let cluster = registry.get_cluster(cluster_name).ok_or_else(|| {
        CraftError::Other(format!("Cluster '{}' was not found.", cluster_name))
    })?;

    let proxies = cluster.proxies();
    if proxies.is_empty() {
        return Err(CraftError::Other(format!(
            "Cluster '{}' does not have any proxy nodes configured. Add a proxy router with 'craft cluster add {} <server> --role proxy'.",
            cluster.name, cluster.name
        )));
    }

    let non_proxies: Vec<&ClusterNode> = cluster
        .nodes
        .iter()
        .filter(|n| n.role != ClusterRole::Proxy)
        .collect();

    if non_proxies.is_empty() {
        return Err(CraftError::Other(format!(
            "Cluster '{}' does not have any backend or lobby nodes to route traffic to.",
            cluster.name
        )));
    }

    let local_reg = ServersRegistry::load(paths).unwrap_or_default();
    let remotes_reg = RemotesRegistry::load(paths).unwrap_or_default();

    // Resolve endpoints for all non-proxy servers
    let mut endpoints: Vec<(String, String, ClusterRole)> = Vec::new();
    for node in non_proxies {
        let (host, port) = if let Some(ref remote_alias) = node.remote {
            let rcfg = remotes_reg.find(remote_alias).ok_or_else(|| {
                CraftError::Config(format!(
                    "Remote alias '{}' for node '{}' not found.",
                    remote_alias, node.name
                ))
            })?;
            // Query remote port or use default 25565
            let client = RemoteCraftClient::connect(rcfg)?;
            let remote_servers = client.list_servers()?;
            let remote_srv = remote_servers.iter().find(|s| s.name.eq_ignore_ascii_case(&node.name));
            let p = remote_srv.map(|s| s.port).unwrap_or(25565);
            (rcfg.host.clone(), p)
        } else {
            let srv = local_reg.find_by_name(&node.name);
            let p = srv.and_then(|s| s.port).unwrap_or(25565);
            ("127.0.0.1".to_string(), p)
        };

        endpoints.push((node.name.clone(), format!("{}:{}", host, port), node.role));
    }

    println!(
        "{}",
        format!(
            "[ROUTING SYNC] Synchronizing proxy routes for cluster '{}' (dry_run = {})...",
            cluster.name, dry_run
        )
        .bold()
        .cyan()
    );

    for proxy_node in proxies {
        println!("  Checking proxy node '{}'...", proxy_node.name.bold());
        if let Some(ref remote_alias) = proxy_node.remote {
            let rcfg = remotes_reg.find(remote_alias).ok_or_else(|| {
                CraftError::Config(format!("Remote alias '{}' not found.", remote_alias))
            })?;
            let client = RemoteCraftClient::connect(rcfg)?;
            let remote_path = format!(".craft/servers/{}", proxy_node.name);
            let sftp = client.sftp();

            let vel_path = PathBuf::from(&remote_path).join("velocity.toml");
            let bun_path = PathBuf::from(&remote_path).join("config.yml");

            if sftp.remote_exists(&vel_path) {
                let content = sftp.read_file_to_string(&vel_path)?;
                let updated = sync_velocity_config(&content, &endpoints)?;
                if dry_run {
                    println!("    [DRY RUN] Would update remote velocity.toml at '{}'", vel_path.display());
                } else {
                    sftp.write_file(&vel_path, updated.as_bytes())?;
                    println!("    {} Updated remote Velocity configuration.", "[OK]".green().bold());
                }
            } else if sftp.remote_exists(&bun_path) {
                let content = sftp.read_file_to_string(&bun_path)?;
                let updated = sync_bungeecord_config(&content, &endpoints)?;
                if dry_run {
                    println!("    [DRY RUN] Would update remote config.yml at '{}'", bun_path.display());
                } else {
                    sftp.write_file(&bun_path, updated.as_bytes())?;
                    println!("    {} Updated remote BungeeCord configuration.", "[OK]".green().bold());
                }
            } else {
                println!("    {} No velocity.toml or config.yml found on remote proxy server.", "[WARN]".yellow().bold());
            }
        } else {
            let srv = local_reg.find_by_name(&proxy_node.name).ok_or_else(|| {
                CraftError::Config(format!("Local proxy server '{}' not found in servers.toml", proxy_node.name))
            })?;

            let vel_path = srv.path.join("velocity.toml");
            let bun_path = srv.path.join("config.yml");

            if vel_path.exists() {
                let content = fs::read_to_string(&vel_path)?;
                let updated = sync_velocity_config(&content, &endpoints)?;
                if dry_run {
                    println!("    [DRY RUN] Planned Velocity updates for '{}':\n{}", vel_path.display(), updated);
                } else {
                    fs::write(&vel_path, updated)?;
                    println!("    {} Updated local Velocity configuration at '{}'.", "[OK]".green().bold(), vel_path.display());
                }
            } else if bun_path.exists() {
                let content = fs::read_to_string(&bun_path)?;
                let updated = sync_bungeecord_config(&content, &endpoints)?;
                if dry_run {
                    println!("    [DRY RUN] Planned BungeeCord updates for '{}':\n{}", bun_path.display(), updated);
                } else {
                    fs::write(&bun_path, updated)?;
                    println!("    {} Updated local BungeeCord configuration at '{}'.", "[OK]".green().bold(), bun_path.display());
                }
            } else {
                println!("    {} No velocity.toml or config.yml found in '{}'.", "[WARN]".yellow().bold(), srv.path.display());
            }
        }
    }

    println!("{}", "[SUCCESS] Routing synchronization complete.".green().bold());
    Ok(())
}

fn handle_delete(cluster_name: &str, paths: &CraftPaths) -> Result<()> {
    ClustersRegistry::modify(paths, |reg| {
        if !reg.remove_cluster(cluster_name) {
            return Err(CraftError::Other(format!(
                "Cluster '{}' was not found.",
                cluster_name
            )));
        }
        Ok(())
    })?;

    println!(
        "{}",
        format!("[OK] Cluster '{}' deleted.", cluster_name)
            .green()
            .bold()
    );
    Ok(())
}

/// Updates Velocity TOML configuration under [servers] and try = [...]
pub fn sync_velocity_config(
    original_toml: &str,
    endpoints: &[(String, String, ClusterRole)],
) -> Result<String> {
    let mut val: toml::Value = toml::from_str(original_toml).map_err(|e| {
        CraftError::Config(format!("Failed to parse velocity.toml: {}", e))
    })?;

    let root = val.as_table_mut().ok_or_else(|| {
        CraftError::Config("velocity.toml root is not a table".to_string())
    })?;

    // Get or create [servers] table
    let servers_entry = root
        .entry("servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

    let servers_table = servers_entry.as_table_mut().ok_or_else(|| {
        CraftError::Config("[servers] in velocity.toml is not a table".to_string())
    })?;

    // Try list: lobbies first, then backends
    let mut try_list: Vec<String> = Vec::new();
    for (name, endpoint, role) in endpoints {
        servers_table.insert(name.clone(), toml::Value::String(endpoint.clone()));
        if *role == ClusterRole::Lobby {
            try_list.push(name.clone());
        }
    }
    for (name, _, role) in endpoints {
        if *role == ClusterRole::Backend && !try_list.contains(name) {
            try_list.push(name.clone());
        }
    }

    // Update try list
    if !try_list.is_empty() {
        let try_values: Vec<toml::Value> = try_list.into_iter().map(toml::Value::String).collect();
        servers_table.insert("try".to_string(), toml::Value::Array(try_values));
    }

    toml::to_string_pretty(&val).map_err(|e| {
        CraftError::Config(format!("Failed to serialize updated velocity.toml: {}", e))
    })
}

/// Updates BungeeCord YAML configuration servers section
pub fn sync_bungeecord_config(
    original_yaml: &str,
    endpoints: &[(String, String, ClusterRole)],
) -> Result<String> {
    let mut lines: Vec<String> = original_yaml.lines().map(|s| s.to_string()).collect();
    let mut servers_idx = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "servers:" || trimmed.starts_with("servers:") {
            servers_idx = Some(idx);
            break;
        }
    }

    let mut generated_servers = Vec::new();
    generated_servers.push("servers:".to_string());
    for (name, endpoint, _) in endpoints {
        generated_servers.push(format!("  {}:", name));
        generated_servers.push(format!("    motd: '&1Just another Craft cluster server'"));
        generated_servers.push(format!("    address: {}", endpoint));
        generated_servers.push(format!("    restricted: false"));
    }

    if let Some(idx) = servers_idx {
        // Find where servers block ends (next unindented top-level key)
        let mut end_idx = idx + 1;
        while end_idx < lines.len() {
            let line = &lines[end_idx];
            if !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }
            end_idx += 1;
        }
        lines.splice(idx..end_idx, generated_servers);
    } else {
        lines.push("".to_string());
        lines.extend(generated_servers);
    }

    Ok(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_velocity_config() {
        let sample = r#"
[servers]
lobby = "127.0.0.1:30001"
try = ["lobby"]
"#;
        let endpoints = vec![
            ("hub".to_string(), "127.0.0.1:25565".to_string(), ClusterRole::Lobby),
            ("survival".to_string(), "10.0.0.5:25566".to_string(), ClusterRole::Backend),
        ];

        let res = sync_velocity_config(sample, &endpoints).unwrap();
        assert!(res.contains("127.0.0.1:25565"));
        assert!(res.contains("10.0.0.5:25566"));
        assert!(res.contains("hub"));
        assert!(res.contains("survival"));
    }

    #[test]
    fn test_sync_bungeecord_config() {
        let sample = r#"
listeners:
  - query_port: 25577
servers:
  old:
    address: localhost:25565
ip_forward: true
"#;
        let endpoints = vec![
            ("lobby".to_string(), "127.0.0.1:25565".to_string(), ClusterRole::Lobby),
            ("games".to_string(), "127.0.0.1:25567".to_string(), ClusterRole::Backend),
        ];

        let res = sync_bungeecord_config(sample, &endpoints).unwrap();
        assert!(res.contains("lobby:"));
        assert!(res.contains("address: 127.0.0.1:25565"));
        assert!(res.contains("games:"));
        assert!(res.contains("address: 127.0.0.1:25567"));
        assert!(res.contains("ip_forward: true"));
    }
}
