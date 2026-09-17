use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use std::fs;

pub fn handle_dockerize(server: &str, paths: &CraftPaths) -> Result<()> {
    let server_path = paths.resolve_server_path(None, Some(server), true)?;
    let registry = ServersRegistry::load(paths)?;
    let server_config = registry.find_by_path(&server_path).ok_or_else(|| {
        CraftError::ServerNotFound(format!("Server '{}' is not registered.", server))
    })?;

    let is_bedrock =
        server_config.software.contains("bedrock") || server_config.software.contains("pocketmine");
    let memory = server_config.memory.as_deref().unwrap_or("2G");

    if is_bedrock {
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
"#;
        fs::write(server_path.join("Dockerfile"), dockerfile)?;

        let compose = format!(
            r#"services:
  {}:
    build: .
    restart: unless-stopped
    ports:
      - "19132:19132/udp"
    volumes:
      - .:/server
"#,
            server
        );
        fs::write(server_path.join("docker-compose.yml"), compose)?;
    } else {
        let dockerfile = format!(
            r#"FROM eclipse-temurin:21-jre-jammy

RUN useradd -m -u 1000 minecraft
WORKDIR /server
COPY --chown=minecraft:minecraft . /server

USER minecraft
EXPOSE 25565
EXPOSE 25575

CMD ["java", "-Xms{}", "-Xmx{}", "-XX:+UseG1GC", "-jar", "server.jar", "nogui"]
"#,
            memory, memory
        );
        fs::write(server_path.join("Dockerfile"), dockerfile)?;

        let compose = format!(
            r#"services:
  {}:
    build: .
    restart: unless-stopped
    ports:
      - "25565:25565"
      - "25575:25575"
    volumes:
      - .:/server
"#,
            server
        );
        fs::write(server_path.join("docker-compose.yml"), compose)?;
    }

    println!(
        "{}",
        format!(
            "[OK] Generated Dockerfile and docker-compose.yml in '{}'!",
            server_path.display()
        )
        .green()
        .bold()
    );
    println!("Run 'docker compose up -d' in that directory to launch.");
    Ok(())
}
