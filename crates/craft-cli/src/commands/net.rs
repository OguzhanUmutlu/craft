use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_net::{
    allow_ip_port, enable_bedrock_loopback, is_bedrock_loopback_enabled, ping_bedrock_server,
    ping_java_server, RconClient,
};
use crate::cli::LoopbackCommands;

pub async fn handle_ping(target: &str, is_bedrock: bool) -> Result<()> {
    let (host, port) = if let Some(idx) = target.find(':') {
        let (h, p) = target.split_at(idx);
        let port: u16 = p[1..].parse().map_err(|_| CraftError::Other("Invalid port number".to_string()))?;
        (h, port)
    } else {
        (target, if is_bedrock { 19132 } else { 25565 })
    };

    println!("{}", format!("Pinging {} on port {}...", host, port).cyan());

    if is_bedrock {
        let res = ping_bedrock_server(host, port).await?;
        println!("{}", "=== Bedrock Server Status ===".green().bold());
        println!("  Server Name:    {}", res.server_name);
        println!("  Version:        {} (Protocol {})", res.version, res.protocol_version);
        println!("  Players:        {}/{}", res.online_players, res.max_players);
        println!("  World:          {}", res.world_name);
        println!("  Game Mode:      {}", res.game_mode);
        println!("  Latency:        {} ms", res.latency_ms);
    } else {
        let res = ping_java_server(host, port).await?;
        println!("{}", "=== Java Server Status ===".green().bold());
        println!("  MOTD:           {}", res.motd);
        println!("  Version:        {} (Protocol {})", res.version_name, res.protocol_version);
        println!("  Players:        {}/{}", res.online_players, res.max_players);
        println!("  Latency:        {} ms", res.latency_ms);
    }

    Ok(())
}

pub async fn handle_rcon(
    server: &str,
    password_arg: Option<String>,
    command: &str,
    paths: &CraftPaths,
) -> Result<()> {
    let (host, port, password) = if server.contains(':') {
        let parts: Vec<&str> = server.split(':').collect();
        let port: u16 = parts[1].parse().map_err(|_| CraftError::Other("Invalid port".to_string()))?;
        let pass = password_arg.ok_or_else(|| CraftError::Other("RCON password required with --password".to_string()))?;
        (parts[0].to_string(), port, pass)
    } else {
        // Look up registered server properties
        let server_path = paths.resolve_server_path(None, Some(server), true)?;
        let props_file = server_path.join("server.properties");
        if !props_file.exists() {
            return Err(CraftError::Other("server.properties not found in server folder".to_string()));
        }

        let content = std::fs::read_to_string(&props_file)?;
        let mut port = 25575;
        let mut pass = String::new();

        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("rcon.port=") {
                if let Ok(p) = rest.trim().parse::<u16>() {
                    port = p;
                }
            } else if let Some(rest) = line.strip_prefix("rcon.password=") {
                pass = rest.trim().to_string();
            }
        }

        if let Some(p) = password_arg {
            pass = p;
        }

        if pass.is_empty() {
            return Err(CraftError::Other("No RCON password found in server.properties or specified via --password".to_string()));
        }

        ("127.0.0.1".to_string(), port, pass)
    };

    println!("{}", format!("Connecting to RCON on {}:{}...", host, port).cyan());
    let mut client = RconClient::connect(&host, port, &password).await?;
    let response = client.send_command(command).await?;

    println!("{}", "=== RCON Response ===".green());
    println!("{}", response);
    Ok(())
}

pub fn handle_firewall(server: &str, ip: &str, paths: &CraftPaths) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server), true)?;
    let registry = ServersRegistry::load(paths)?;
    let server_config = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!("Server '{}' is not registered.", server))
    })?;

    let is_bedrock = server_config.software.contains("bedrock") || server_config.software.contains("pocketmine");
    let port = server_config.port.unwrap_or(if is_bedrock { 19132 } else { 25565 });

    println!("{}", format!("Adding firewall rule to allow {} on port {} ({})...", ip, port, if is_bedrock { "UDP" } else { "TCP" }).cyan());
    allow_ip_port(ip, port, is_bedrock)?;
    println!("{}", "Firewall rule created successfully!".green().bold());
    Ok(())
}

pub fn handle_loopback(action: Option<LoopbackCommands>) -> Result<()> {
    match action.unwrap_or(LoopbackCommands::Status) {
        LoopbackCommands::Status => {
            let enabled = is_bedrock_loopback_enabled()?;
            if enabled {
                println!("{}", "Windows Bedrock UWP Loopback exemption: ENABLED".green().bold());
            } else {
                println!("{}", "Windows Bedrock UWP Loopback exemption: DISABLED".yellow().bold());
                println!("Run 'craft loopback enable' to allow joining local Bedrock servers from the same PC.");
            }
        }
        LoopbackCommands::Enable => {
            enable_bedrock_loopback()?;
            println!("{}", "Windows Bedrock UWP Loopback exemption enabled successfully!".green().bold());
        }
    }
    Ok(())
}
