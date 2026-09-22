use crate::cli::DeployCommands;
use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

const DOCKERFILE_TEMPLATE: &str = r#"# Stage 1: Build or extract Craft binary
FROM rust:1.80-bullseye AS builder
WORKDIR /workspace
COPY . .
RUN if [ -f target/release/craft ]; then \
        cp target/release/craft /craft-bin; \
    elif [ -f craft ]; then \
        cp craft /craft-bin; \
    else \
        cargo build --release -p craft && cp target/release/craft /craft-bin; \
    fi

# Stage 2: Runtime environment
FROM eclipse-temurin:21-jre-jammy

LABEL org.opencontainers.image.title="Craft" \
      org.opencontainers.image.description="High-performance Minecraft Server Management & Background Daemon"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl tar gzip procps tini gosu libcurl4 libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 craft && useradd -u 1000 -g craft -m -s /bin/bash craft
ENV CRAFT_HOME=/craft
ENV PATH="/usr/local/bin:${PATH}"

RUN mkdir -p /craft && chown -R craft:craft /craft

COPY --from=builder /craft-bin /usr/local/bin/craft
RUN chmod +x /usr/local/bin/craft

COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

EXPOSE 25565 19132/udp 25575 8123
VOLUME ["/craft"]
WORKDIR /craft

ENTRYPOINT ["tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
CMD ["daemon"]
"#;

const DOCKER_COMPOSE_TEMPLATE: &str = r#"services:
  craft:
    build:
      context: .
      dockerfile: Dockerfile
    image: craft:latest
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
    healthcheck:
      test: ["CMD-SHELL", "craft service status || exit 1"]
      interval: 15s
      timeout: 5s
      retries: 3
      start_period: 10s
"#;

const ENTRYPOINT_TEMPLATE: &str = r#"#!/bin/bash
set -e
CRAFT_DATA_DIR="${CRAFT_HOME:-/craft}"
mkdir -p "$CRAFT_DATA_DIR"
if [ "$(id -u)" = '0' ]; then
    chown -R craft:craft "$CRAFT_DATA_DIR"
    EXEC_CMD="gosu craft"
else
    EXEC_CMD=""
fi
if [ $# -eq 0 ] || [ "$1" = 'daemon' ] || [ "$1" = 'service' ]; then
    echo "================================================================"
    echo "  Starting Craft Background Supervisor Daemon"
    echo "  Data Directory: $CRAFT_DATA_DIR"
    echo "================================================================"
    exec $EXEC_CMD craft service start --foreground
fi
if [ "${1:0:1}" = '-' ]; then
    exec $EXEC_CMD craft "$@"
fi
case "$1" in
    new|run|stop|view|ls|rm|load|ver|update|cache|service|auto|fix|plugin|ping|rcon|backup|firewall|loopback|template|dockerize|remote|deploy)
        exec $EXEC_CMD craft "$@"
        ;;
    *)
        exec $EXEC_CMD "$@"
        ;;
esac
"#;

pub async fn handle_deploy(action: Option<DeployCommands>, paths: &CraftPaths) -> Result<()> {
    let action = action.unwrap_or(DeployCommands::Up {
        detach: true,
        build: false,
    });

    match action {
        DeployCommands::Vds { target, dir } => {
            crate::commands::remote::handle_remote_setup_docker(&target, dir, paths).await?;
        }
        DeployCommands::Up { detach, build } => {
            println!(
                "{}",
                "Deploying Craft container stack with Docker Compose..."
                    .cyan()
                    .bold()
            );
            ensure_compose_files_exist()?;

            let mut args = vec!["compose", "up"];
            if detach {
                args.push("-d");
            }
            if build {
                args.push("--build");
            }

            let status = Command::new("docker").args(&args).status().map_err(|e| {
                CraftError::Other(format!(
                    "Failed to execute 'docker compose up': {}. Is Docker installed?",
                    e
                ))
            })?;

            if status.success() {
                println!(
                    "{}",
                    "[OK] Craft container stack deployed successfully!"
                        .green()
                        .bold()
                );
                println!(
                    "Run '{}' to view container health.",
                    "craft deploy status".yellow()
                );
                println!("Run '{}' to stream logs.", "craft deploy logs -f".yellow());
            } else {
                return Err(CraftError::Other(
                    "docker compose up exited with an error".to_string(),
                ));
            }
        }
        DeployCommands::Down { volumes } => {
            println!(
                "{}",
                "Tearing down Craft container stack...".yellow().bold()
            );
            let mut args = vec!["compose", "down"];
            if volumes {
                args.push("-v");
            }

            let status = Command::new("docker").args(&args).status().map_err(|e| {
                CraftError::Other(format!("Failed to execute 'docker compose down': {}", e))
            })?;

            if status.success() {
                println!(
                    "{}",
                    "[OK] Craft container stack stopped and removed."
                        .green()
                        .bold()
                );
            } else {
                return Err(CraftError::Other(
                    "docker compose down exited with an error".to_string(),
                ));
            }
        }
        DeployCommands::Status => {
            println!("{}", "Craft Container Status:".cyan().bold());
            let _ = Command::new("docker").args(["compose", "ps"]).status();
        }
        DeployCommands::Logs { follow, tail } => {
            let mut args = vec!["compose", "logs"];
            if follow {
                args.push("-f");
            }
            let tail_val;
            if let Some(ref t) = tail {
                args.push("--tail");
                tail_val = t.clone();
                args.push(&tail_val);
            }

            let _ = Command::new("docker").args(&args).status();
        }
        DeployCommands::Exec { command } => {
            if command.is_empty() {
                return Err(CraftError::Other(
                    "No command specified to execute inside container".to_string(),
                ));
            }

            let mut args = vec!["compose", "exec", "craft", "craft"];
            for arg in &command {
                args.push(arg);
            }

            let status = Command::new("docker").args(&args).status().map_err(|e| {
                CraftError::Other(format!("Failed to execute docker compose exec: {}", e))
            })?;

            if !status.success() {
                return Err(CraftError::Other(format!(
                    "In-container command exited with status {:?}",
                    status.code()
                )));
            }
        }
        DeployCommands::Init { force } => {
            init_docker_templates(force)?;
        }
    }

    Ok(())
}

fn ensure_compose_files_exist() -> Result<()> {
    if !Path::new("docker-compose.yml").exists() && !Path::new("compose.yaml").exists() {
        println!(
            "{}",
            "docker-compose.yml not found. Initializing default Docker files...".yellow()
        );
        init_docker_templates(false)?;
    }
    Ok(())
}

fn init_docker_templates(force: bool) -> Result<()> {
    let dockerfile = Path::new("Dockerfile");
    let compose = Path::new("docker-compose.yml");
    let entrypoint = Path::new("docker-entrypoint.sh");

    if (!force) && dockerfile.exists() && compose.exists() {
        println!(
            "{}",
            "Docker deployment files already exist. Use --force to overwrite.".yellow()
        );
        return Ok(());
    }

    fs::write(dockerfile, DOCKERFILE_TEMPLATE)?;
    println!("{}", "[OK] Created Dockerfile".green());

    fs::write(compose, DOCKER_COMPOSE_TEMPLATE)?;
    println!("{}", "[OK] Created docker-compose.yml".green());

    fs::write(entrypoint, ENTRYPOINT_TEMPLATE)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(entrypoint)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(entrypoint, perms)?;
    }
    println!("{}", "[OK] Created docker-entrypoint.sh".green());

    println!(
        "{}",
        "Successfully initialized Docker deployment templates!"
            .green()
            .bold()
    );
    Ok(())
}
