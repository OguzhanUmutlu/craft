use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{CraftError, CraftPaths, RemoteAuthType, RemoteHostConfig, RemotesRegistry, Result};
use craft_remote::{run_bootstrap, run_remote_pty_session, sync_local_to_remote, RemoteSession};
use crate::cli::RemoteCommands;

pub async fn handle_remote(action: RemoteCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        RemoteCommands::Add { alias, connection, key, password, dir } => {
            let (user, host, port) = parse_connection_string(&connection)?;
            let mut registry = RemotesRegistry::load(paths)?;

            let auth_type = if password.is_some() {
                RemoteAuthType::Password
            } else {
                RemoteAuthType::Key
            };

            let remote_config = RemoteHostConfig {
                alias: alias.clone(),
                host,
                port,
                user,
                auth_type,
                key_path: key,
                password,
                remote_dir: dir,
                os_type: None,
            };

            registry.add(remote_config)?;
            registry.save(paths)?;

            println!("{}", format!("✓ Successfully added remote host '{}'!", alias).green().bold());
            println!("Test connection with: craft remote test {}", alias);
            println!("Bootstrap host with:   craft remote setup {}", alias);
        }
        RemoteCommands::Ls => {
            let registry = RemotesRegistry::load(paths)?;
            if registry.remotes.is_empty() {
                println!("{}", "No remote hosts configured. Add one with 'craft remote add <alias> <user@host>'.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            table.set_header(vec![
                Cell::new("Alias").fg(Color::Cyan),
                Cell::new("User").fg(Color::Cyan),
                Cell::new("Host").fg(Color::Cyan),
                Cell::new("Port").fg(Color::Cyan),
                Cell::new("Auth Type").fg(Color::Cyan),
                Cell::new("OS").fg(Color::Cyan),
            ]);

            for r in registry.remotes {
                table.add_row(Row::from(vec![
                    Cell::new(r.alias).fg(Color::Green),
                    Cell::new(r.user),
                    Cell::new(r.host),
                    Cell::new(r.port.to_string()),
                    Cell::new(format!("{:?}", r.auth_type)),
                    Cell::new(r.os_type.map(|o| o.to_string()).unwrap_or_else(|| "Auto".to_string())),
                ]));
            }

            println!("{table}");
        }
        RemoteCommands::Rm { alias } => {
            let mut registry = RemotesRegistry::load(paths)?;
            if registry.remove(&alias).is_none() {
                return Err(CraftError::Other(format!("Remote host '{}' not found", alias)));
            }
            registry.save(paths)?;
            println!("{}", format!("✓ Removed remote host '{}'.", alias).green());
        }
        RemoteCommands::Test { alias } => {
            let registry = RemotesRegistry::load(paths)?;
            let config = registry.find(&alias).ok_or_else(|| {
                CraftError::Other(format!("Remote host '{}' not found. Use 'craft remote ls' to list remotes.", alias))
            })?;

            println!("{}", format!("Testing SSH connection to '{}' ({}@{}:{})...", alias, config.user, config.host, config.port).cyan());
            let session = RemoteSession::connect(config)?;
            let os = session.probe_os()?;

            let (_, echo_out, _) = session.exec("echo connection-verified")?;
            if echo_out.trim() == "connection-verified" {
                println!("{}", format!("✓ Connection successful! Remote OS: {}", os).green().bold());
            } else {
                println!("{}", "Connection established but command output differed.".yellow());
            }
        }
        RemoteCommands::Setup { alias } => {
            let registry = RemotesRegistry::load(paths)?;
            let config = registry.find(&alias).ok_or_else(|| {
                CraftError::Other(format!("Remote host '{}' not found", alias))
            })?;

            println!("{}", format!("Connecting to remote host '{}' for automated bootstrapping...", alias).cyan());
            let session = RemoteSession::connect(config)?;
            run_bootstrap(&session)?;
        }
        RemoteCommands::Sync { alias, local_dir, remote_dir } => {
            let registry = RemotesRegistry::load(paths)?;
            let config = registry.find(&alias).ok_or_else(|| {
                CraftError::Other(format!("Remote host '{}' not found", alias))
            })?;

            let session = RemoteSession::connect(config)?;
            sync_local_to_remote(&session, &local_dir, &remote_dir)?;
        }
        RemoteCommands::Deploy { alias } => {
            let registry = RemotesRegistry::load(paths)?;
            let config = registry.find(&alias).ok_or_else(|| {
                CraftError::Other(format!("Remote host '{}' not found. Use 'craft remote add' to configure it.", alias))
            })?;

            println!("{}", format!("Connecting to remote host '{}' ({}:{})...", config.alias, config.host, config.port).cyan());
            let session = RemoteSession::connect(config)?;

            println!("{}", "Checking remote Docker and Docker Compose installation...".cyan());
            let (code, stdout, _) = session.exec("docker compose version 2>/dev/null || docker-compose --version 2>/dev/null")?;
            if code != 0 {
                println!("{}", "Docker or Docker Compose not detected on remote host.".yellow());
                println!("{}", "Attempting automated Docker installation on remote Linux host...".yellow());
                let (inst_code, inst_out, inst_err) = session.exec("curl -fsSL https://get.docker.com | sh")?;
                if inst_code != 0 {
                    return Err(CraftError::Other(format!(
                        "Failed to auto-install Docker on remote: {}. Please install Docker manually on host.",
                        inst_err.trim()
                    )));
                }
                println!("{}", inst_out);
            } else {
                println!("{}", format!("✓ Found remote Docker: {}", stdout.trim()).green());
            }

            let remote_dir = config.remote_dir.clone().unwrap_or_else(|| std::path::PathBuf::from("craft-deploy"));
            println!("{}", format!("Preparing remote deployment directory '{}'...", remote_dir.display()).cyan());
            session.exec(&format!("mkdir -p {}", remote_dir.display()))?;

            // Generate or read docker-compose.yml
            let compose_content = if std::path::Path::new("docker-compose.yml").exists() {
                std::fs::read_to_string("docker-compose.yml")?
            } else {
                r#"services:
  craft:
    image: ghcr.io/oguzhanumutlu/craft:latest
    container_name: craft
    restart: unless-stopped
    stdin_open: true
    tty: true
    ports:
      - "25565:25565"
      - "19132:19132/udp"
      - "25575:25575"
      - "8123:8123"
    volumes:
      - ./craft-data:/craft
    environment:
      - CRAFT_HOME=/craft
      - TZ=UTC
"#.to_string()
            };

            let temp_compose = paths.home.join(".remote-compose.tmp");
            std::fs::write(&temp_compose, compose_content)?;
            let remote_compose_path = remote_dir.join("docker-compose.yml");
            let sftp = craft_remote::SftpOps::new(&session);
            sftp.upload_file(&temp_compose, &remote_compose_path)?;
            let _ = std::fs::remove_file(&temp_compose);

            println!("{}", "Launching Craft container stack on remote host via 'docker compose up -d'...".cyan().bold());
            let launch_cmd = format!("cd {} && docker compose up -d", remote_dir.display());
            let (up_code, up_stdout, up_stderr) = session.exec(&launch_cmd)?;
            if up_code != 0 {
                return Err(CraftError::Other(format!("Failed to start containers on remote host: {}", up_stderr)));
            }
            if !up_stdout.is_empty() {
                println!("{}", up_stdout);
            }

            println!("{}", format!("✓ Craft successfully deployed to remote host '{}'!", alias).green().bold());
            println!("Check remote status: craft remote test {}", alias);
        }
    }

    Ok(())
}

/// Dispatches a command to a remote host over SSH
pub fn execute_remote(
    alias: &str,
    remote_command: &str,
    is_interactive: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let registry = RemotesRegistry::load(paths)?;
    let config = registry.find(alias).ok_or_else(|| {
        CraftError::Other(format!("Remote host '{}' not found. Use 'craft remote add' to configure it.", alias))
    })?;

    let session = RemoteSession::connect(config)?;

    if is_interactive {
        run_remote_pty_session(&session, remote_command)
    } else {
        println!("{}", format!("Executing on remote '{}': {}", alias, remote_command).dimmed());
        let (code, stdout, stderr) = session.exec(remote_command)?;
        if !stdout.is_empty() {
            print!("{}", stdout);
        }
        if !stderr.is_empty() {
            eprint!("{}", stderr);
        }
        if code != 0 {
            return Err(CraftError::Other(format!("Remote command exited with status code {}", code)));
        }
        Ok(())
    }
}

fn parse_connection_string(conn: &str) -> Result<(String, String, u16)> {
    // Format: user@host or user@host:port
    let parts: Vec<&str> = conn.split('@').collect();
    if parts.len() != 2 {
        return Err(CraftError::Other(
            "Invalid connection string. Expected format: user@host or user@host:port (e.g. root@192.168.1.100)".to_string(),
        ));
    }

    let user = parts[0].to_string();
    let host_port = parts[1];

    if let Some(colon) = host_port.find(':') {
        let host = host_port[..colon].to_string();
        let port: u16 = host_port[colon + 1..].parse().map_err(|_| {
            CraftError::Other("Invalid SSH port number".to_string())
        })?;
        Ok((user, host, port))
    } else {
        Ok((user, host_port.to_string(), 22))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connection_string() {
        let (user, host, port) = parse_connection_string("root@192.168.1.100").unwrap();
        assert_eq!(user, "root");
        assert_eq!(host, "192.168.1.100");
        assert_eq!(port, 22);

        let (user, host, port) = parse_connection_string("ubuntu@vps.example.com:2222").unwrap();
        assert_eq!(user, "ubuntu");
        assert_eq!(host, "vps.example.com");
        assert_eq!(port, 2222);

        assert!(parse_connection_string("no-user-format").is_err());
        assert!(parse_connection_string("root@host:not_a_port").is_err());
    }
}

