use colored::Colorize;
use craft_core::{
    find_game, get_jar_java_version, CraftError, CraftPaths, Result, ServerProperties,
    ServersRegistry,
};
use craft_providers::{find_software, ServerEdition};
use std::fs;

pub fn handle_dockerize(server: &str, paths: &CraftPaths) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server), true)?;
    let registry = ServersRegistry::load(paths)?;
    let server_config = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!("Server '{}' is not registered.", server))
    })?;

    let software = find_software(&server_config.software)
        .ok_or_else(|| CraftError::UnknownSoftware(server_config.software.clone()))?;

    let edition = software.edition();
    let game_id = software.game_id();
    let game_def = find_game(game_id);

    // Resolve active primary port
    let default_port = game_def.map(|g| g.default_port).unwrap_or(25565);
    let mut primary_port = default_port;
    let mut rcon_port: Option<u16> = None;

    if server_path.join("server.properties").exists() {
        if let Ok(props) = ServerProperties::load(&server_path) {
            if let Some(p) = props.get("server-port").and_then(|v| v.parse::<u16>().ok()) {
                primary_port = p;
            }
            if props
                .get("enable-rcon")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            {
                if let Some(r) = props.get("rcon.port").and_then(|v| v.parse::<u16>().ok()) {
                    rcon_port = Some(r);
                }
            }
        }
    }

    // Ensure start.sh exists
    let start_sh = server_path.join("start.sh");
    if !start_sh.exists() {
        let memory = server_config.memory.as_deref().unwrap_or("2G");
        let _ = software.generate_start_script_with_flags(
            &server_path,
            &server_config.version,
            server_config.java_path.as_deref(),
            memory,
            server_config.jvm_args.as_deref(),
        );
    }

    let (dockerfile, compose_ports) = match edition {
        ServerEdition::Java | ServerEdition::Proxy => {
            let jar_path = server_path.join(software.default_server_file());
            let java_ver = if jar_path.exists() {
                if let Ok(ver) = get_jar_java_version(&jar_path) {
                    if ver <= 8 {
                        8
                    } else if ver <= 17 {
                        17
                    } else {
                        21
                    }
                } else {
                    21
                }
            } else {
                21
            };

            let mut expose_lines = vec![format!("EXPOSE {}", primary_port)];
            let mut ports = vec![format!("\"{}:{}\"", primary_port, primary_port)];

            if let Some(rc) = rcon_port {
                expose_lines.push(format!("EXPOSE {}", rc));
                ports.push(format!("\"{}:{}\"", rc, rc));
            }

            let dockerfile = format!(
                r#"FROM eclipse-temurin:{java_ver}-jre-jammy

RUN useradd -m -u 1000 minecraft
WORKDIR /server
COPY --chown=minecraft:minecraft . /server
RUN chmod +x /server/start.sh 2>/dev/null || true

USER minecraft
{expose_block}

CMD ["/bin/sh", "start.sh"]
"#,
                java_ver = java_ver,
                expose_block = expose_lines.join("\n")
            );

            (dockerfile, ports)
        }
        ServerEdition::Bedrock => {
            let dockerfile = r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates libcurl4 libssl3 procps \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/bedrock_server 2>/dev/null || true
RUN chmod +x /server/bin/php7/bin/php 2>/dev/null || true
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE 19132/udp
CMD ["/bin/sh", "start.sh"]
"#
            .to_string();

            let ports = vec!["\"19132:19132/udp\"".to_string()];
            (dockerfile, ports)
        }
        ServerEdition::Native => {
            let (dockerfile, ports) = match game_id {
                "factorio" => {
                    let df = r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates procps libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/bin/x64/factorio 2>/dev/null || true
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE 34197/udp
CMD ["/bin/sh", "start.sh"]
"#
                    .to_string();
                    let p = vec!["\"34197:34197/udp\"".to_string()];
                    (df, p)
                }
                "terraria" => {
                    let df = r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates procps libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/TerrariaServer.bin.x86_64 2>/dev/null || true
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE 7777
CMD ["/bin/sh", "start.sh"]
"#
                    .to_string();
                    let p = vec!["\"7777:7777\"".to_string()];
                    (df, p)
                }
                "palworld" => {
                    let df = r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates procps libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/PalServer.sh 2>/dev/null || true
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE 8211/udp
EXPOSE 27015/udp
CMD ["/bin/sh", "start.sh"]
"#
                    .to_string();
                    let p = vec![
                        "\"8211:8211/udp\"".to_string(),
                        "\"27015:27015/udp\"".to_string(),
                    ];
                    (df, p)
                }
                "valheim" => {
                    let df = r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates procps libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/valheim_server.x86_64 2>/dev/null || true
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE 2456-2457/udp
CMD ["/bin/sh", "start.sh"]
"#
                    .to_string();
                    let p = vec!["\"2456-2457:2456-2457/udp\"".to_string()];
                    (df, p)
                }
                _ => {
                    let df = format!(
                        r#"FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates procps libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /server
COPY . /server
RUN chmod +x /server/start.sh 2>/dev/null || true

EXPOSE {port}
CMD ["/bin/sh", "start.sh"]
"#,
                        port = primary_port
                    );
                    let p = vec![format!("\"{}:{}\"", primary_port, primary_port)];
                    (df, p)
                }
            };
            (dockerfile, ports)
        }
    };

    fs::write(server_path.join("Dockerfile"), dockerfile)?;

    let ports_yaml = compose_ports
        .iter()
        .map(|p| format!("      - {}", p))
        .collect::<Vec<_>>()
        .join("\n");

    let compose = format!(
        r#"services:
  {name}:
    build: .
    restart: unless-stopped
    ports:
{ports}
    volumes:
      - .:/server
"#,
        name = server,
        ports = ports_yaml
    );
    fs::write(server_path.join("docker-compose.yml"), compose)?;

    println!(
        "{}",
        format!(
            "[OK] Generated Dockerfile and docker-compose.yml for '{}' ({}) in '{}'!",
            server,
            software.name(),
            server_path.display()
        )
        .green()
        .bold()
    );
    println!("Run 'docker compose up -d' in that directory to launch.");
    Ok(())
}
