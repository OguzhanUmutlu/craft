#!/usr/bin/env bash
# ==============================================================================
# Craft VDS Automated Docker Setup Script
# Safely connects to a remote VDS over SSH, checks and installs Docker CE +
# Docker Compose if missing, deploys the Craft container stack, and registers
# the host into the local Craft remote registry.
#
# Usage:
#   ./scripts/setup-vds.sh <ssh-host-or-alias> [options]
#   ./scripts/setup-vds.sh saga
# ==============================================================================

set -euo pipefail

# ANSI Color Codes
BOLD='\033[1m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Resolve project root directory robustly even when called via symlink
RESOLVED_SOURCE="$(readlink -f "${BASH_SOURCE[0]}" 2>/dev/null || realpath "${BASH_SOURCE[0]}" 2>/dev/null || echo "${BASH_SOURCE[0]}")"
SCRIPT_BASE_DIR="$(cd -P "$(dirname "${RESOLVED_SOURCE}")" && pwd)"
if [ -f "${SCRIPT_BASE_DIR}/../Cargo.toml" ]; then
    PROJECT_ROOT="$(cd -P "${SCRIPT_BASE_DIR}/.." && pwd)"
elif [ -f "${SCRIPT_BASE_DIR}/Cargo.toml" ]; then
    PROJECT_ROOT="${SCRIPT_BASE_DIR}"
else
    PROJECT_ROOT="$(pwd)"
fi

# Banner
echo -e "${CYAN}${BOLD}"
echo "=================================================================="
echo "          Craft VDS Automated Docker Setup (Safe SSH)            "
echo "=================================================================="
echo -e "${NC}"

# 1. Argument validation
if [ $# -lt 1 ] || [ -z "$1" ] || [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
    echo -e "${BOLD}Usage:${NC} $0 <ssh-host-or-alias> [options]"
    echo ""
    echo "Arguments:"
    echo "  <ssh-host-or-alias>   Host alias from ~/.ssh/config (e.g. 'saga'),"
    echo "                        or direct user@hostname[:port]"
    echo ""
    echo "Examples:"
    echo "  $0 saga"
    echo "  $0 root@185.157.46.103"
    echo "  $0 ubuntu@vps.example.com"
    exit 1
fi

TARGET="$1"
REMOTE_DEPLOY_DIR="${CRAFT_REMOTE_DIR:-}"

# 2. Check local SSH binary
if ! command -v ssh &>/dev/null; then
    echo -e "${RED}[ERROR] OpenSSH client ('ssh') is not installed on this machine.${NC}" >&2
    exit 1
fi

# 3. Resolve target host configuration using OpenSSH
echo -e "${CYAN}==> Resolving SSH target: ${BOLD}${TARGET}${NC}"

SSH_G_OUTPUT="$(ssh -G "${TARGET}" 2>/dev/null || true)"
if [ -n "${SSH_G_OUTPUT}" ]; then
    RESOLVED_HOST="$(echo "${SSH_G_OUTPUT}" | awk '$1 == "hostname" {print $2; exit}')"
    RESOLVED_USER="$(echo "${SSH_G_OUTPUT}" | awk '$1 == "user" {print $2; exit}')"
    RESOLVED_PORT="$(echo "${SSH_G_OUTPUT}" | awk '$1 == "port" {print $2; exit}')"
else
    RESOLVED_HOST="${TARGET}"
    RESOLVED_USER="${USER:-root}"
    RESOLVED_PORT="22"
fi

RESOLVED_HOST="${RESOLVED_HOST:-${TARGET}}"
RESOLVED_USER="${RESOLVED_USER:-${USER:-root}}"
RESOLVED_PORT="${RESOLVED_PORT:-22}"

echo "    Host Address: ${RESOLVED_HOST}"
echo "    Remote User:  ${RESOLVED_USER}"
echo "    Remote Port:  ${RESOLVED_PORT}"

# Safe SSH Options:
# - BatchMode=no allows interactive password/key passphrase prompt if needed
# - ConnectTimeout=15 avoids hanging indefinitely on dead connections
# - StrictHostKeyChecking=accept-new automatically saves trusted new keys, rejects changed keys
# - ServerAliveInterval=30 keeps session alive through NAT/firewalls during long installs
SSH_OPTS=(
    -o "BatchMode=no"
    -o "ConnectTimeout=15"
    -o "StrictHostKeyChecking=accept-new"
    -o "ServerAliveInterval=30"
    -o "ServerAliveCountMax=3"
)

# 4. Test SSH Connection
echo -e "${CYAN}==> Testing safe SSH connection to ${BOLD}${TARGET}${NC}..."
if ! ssh "${SSH_OPTS[@]}" "${TARGET}" "echo __craft_ssh_ok__" 2>/dev/null | grep -q "__craft_ssh_ok__"; then
    echo -e "${RED}[ERROR] Unable to connect to '${TARGET}' over SSH.${NC}" >&2
    echo "Please verify:"
    echo "  - The host '${TARGET}' is reachable at ${RESOLVED_HOST}:${RESOLVED_PORT}"
    echo "  - Your SSH keys or credentials are authorized for user '${RESOLVED_USER}'"
    echo "  - Test manually with: ssh ${TARGET}"
    exit 1
fi
echo -e "${GREEN}[OK] SSH connection verified successfully!${NC}"

# 5. Remote system discovery
echo -e "${CYAN}==> Inspecting remote system environment...${NC}"
REMOTE_INFO="$(ssh "${SSH_OPTS[@]}" "${TARGET}" bash << 'EOF'
    OS_NAME="Linux"
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        OS_NAME="${PRETTY_NAME:-$NAME}"
    fi
    ARCH="$(uname -m)"
    IS_ROOT="no"
    if [ "$(id -u)" -eq 0 ]; then
        IS_ROOT="yes"
    elif sudo -n true 2>/dev/null; then
        IS_ROOT="sudo"
    fi
    echo "OS=${OS_NAME}"
    echo "ARCH=${ARCH}"
    echo "ROOT=${IS_ROOT}"
EOF
)"

REMOTE_OS="$(echo "${REMOTE_INFO}" | grep '^OS=' | cut -d= -f2-)"
REMOTE_ARCH="$(echo "${REMOTE_INFO}" | grep '^ARCH=' | cut -d= -f2-)"
REMOTE_PRIV="$(echo "${REMOTE_INFO}" | grep '^ROOT=' | cut -d= -f2-)"

echo "    Remote OS:    ${REMOTE_OS}"
echo "    Architecture: ${REMOTE_ARCH}"
echo "    Privilege:    ${REMOTE_PRIV}"

if [ "${REMOTE_PRIV}" = "no" ]; then
    echo -e "${RED}[ERROR] Remote user '${RESOLVED_USER}' lacks root or passwordless sudo privileges to manage Docker.${NC}" >&2
    exit 1
fi

SUDO_PREFIX=""
if [ "${REMOTE_PRIV}" = "sudo" ]; then
    SUDO_PREFIX="sudo "
fi

# Determine remote deploy directory
if [ -z "${REMOTE_DEPLOY_DIR}" ]; then
    if [ "${RESOLVED_USER}" = "root" ]; then
        REMOTE_DEPLOY_DIR="/opt/craft"
    else
        REMOTE_DEPLOY_DIR="craft-deploy"
    fi
fi

# 6. Verify or Install Docker & Docker Compose
echo -e "${CYAN}==> Checking remote Docker and Docker Compose installation...${NC}"

DOCKER_STATUS="$(ssh "${SSH_OPTS[@]}" "${TARGET}" bash << 'EOF'
    HAS_DOCKER="no"
    HAS_COMPOSE="no"
    if command -v docker >/dev/null 2>&1; then
        HAS_DOCKER="yes"
    fi
    if docker compose version >/dev/null 2>&1 || docker-compose --version >/dev/null 2>&1; then
        HAS_COMPOSE="yes"
    fi
    echo "DOCKER=${HAS_DOCKER}"
    echo "COMPOSE=${HAS_COMPOSE}"
EOF
)"

HAS_DOCKER="$(echo "${DOCKER_STATUS}" | grep '^DOCKER=' | cut -d= -f2-)"
HAS_COMPOSE="$(echo "${DOCKER_STATUS}" | grep '^COMPOSE=' | cut -d= -f2-)"

if [ "${HAS_DOCKER}" = "yes" ] && [ "${HAS_COMPOSE}" = "yes" ]; then
    DOCKER_VER="$(ssh "${SSH_OPTS[@]}" "${TARGET}" "docker --version" | head -n1)"
    COMPOSE_VER="$(ssh "${SSH_OPTS[@]}" "${TARGET}" "docker compose version 2>/dev/null || docker-compose --version" | head -n1)"
    echo -e "${GREEN}[OK] Docker is installed: ${DOCKER_VER}${NC}"
    echo -e "${GREEN}[OK] Compose is installed: ${COMPOSE_VER}${NC}"
else
    echo -e "${YELLOW}==> Docker or Docker Compose missing. Starting safe official installation...${NC}"
    ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash" << 'EOF'
        echo "[1/3] Ensuring curl and ca-certificates are installed..."
        if command -v apt-get >/dev/null 2>&1; then
            export DEBIAN_FRONTEND=noninteractive
            apt-get update -qq && apt-get install -y -qq curl ca-certificates
        elif command -v dnf >/dev/null 2>&1; then
            dnf install -y -q curl ca-certificates
        elif command -v yum >/dev/null 2>&1; then
            yum install -y -q curl ca-certificates
        fi

        echo "[2/3] Installing Docker CE and Compose plugin via official Docker script..."
        curl -fsSL https://get.docker.com -o /tmp/get-docker.sh
        sh /tmp/get-docker.sh
        rm -f /tmp/get-docker.sh

        echo "[3/3] Enabling and starting Docker daemon..."
        systemctl enable --now docker 2>/dev/null || service docker start 2>/dev/null || true
EOF
    echo -e "${GREEN}[OK] Docker and Docker Compose installed successfully!${NC}"
fi

# Ensure user is in docker group if non-root
if [ "${RESOLVED_USER}" != "root" ]; then
    ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}usermod -aG docker ${RESOLVED_USER} 2>/dev/null || true"
fi

# 7. Prepare Craft Deployment Directory on Remote Host
echo -e "${CYAN}==> Preparing remote deployment directory at ${BOLD}${REMOTE_DEPLOY_DIR}${NC}..."
ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    DEPLOY_DIR="$1"
    mkdir -p "${DEPLOY_DIR}/craft-data/servers"
    mkdir -p "${DEPLOY_DIR}/craft-data/backups"
    mkdir -p "${DEPLOY_DIR}/craft-data/cache"
    mkdir -p "${DEPLOY_DIR}/target/release"
    chmod -R 755 "${DEPLOY_DIR}"
EOF

# 8. Deploy Docker Build Context & Compose Configuration
echo -e "${CYAN}==> Deploying Docker files and Craft binary to remote host...${NC}"
LOCAL_CRAFT_BIN="${PROJECT_ROOT}/bin/craft"
if [ ! -f "${LOCAL_CRAFT_BIN}" ] && [ -f "${PROJECT_ROOT}/target/release/craft" ]; then
    LOCAL_CRAFT_BIN="${PROJECT_ROOT}/target/release/craft"
fi
DOCKERFILE_PATH="${PROJECT_ROOT}/Dockerfile"
ENTRYPOINT_PATH="${PROJECT_ROOT}/docker-entrypoint.sh"

if [ ! -f "${LOCAL_CRAFT_BIN}" ]; then
    echo -e "${RED}[ERROR] Craft binary not found at ${LOCAL_CRAFT_BIN}. Please run 'cargo build --release' first.${NC}" >&2
    exit 1
fi

if [ ! -f "${DOCKERFILE_PATH}" ]; then
    echo -e "${RED}[ERROR] Dockerfile not found at ${DOCKERFILE_PATH}.${NC}" >&2
    exit 1
fi

if [ ! -f "${ENTRYPOINT_PATH}" ]; then
    echo -e "${RED}[ERROR] docker-entrypoint.sh not found at ${ENTRYPOINT_PATH}.${NC}" >&2
    exit 1
fi

# Upload binary, Dockerfile, and entrypoint
echo "    Uploading ${LOCAL_CRAFT_BIN}..."
scp "${SSH_OPTS[@]}" "${LOCAL_CRAFT_BIN}" "${TARGET}:/tmp/craft-bin.tmp"
ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    DEPLOY_DIR="$1"
    cp /tmp/craft-bin.tmp "${DEPLOY_DIR}/target/release/craft"
    chmod +x "${DEPLOY_DIR}/target/release/craft"
    # Also make available natively on host
    cp /tmp/craft-bin.tmp /usr/local/bin/craft
    chmod +x /usr/local/bin/craft
    rm -f /tmp/craft-bin.tmp
EOF

echo "    Uploading Dockerfile..."
scp "${SSH_OPTS[@]}" "${DOCKERFILE_PATH}" "${TARGET}:/tmp/craft-dockerfile.tmp"
ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    DEPLOY_DIR="$1"
    mv /tmp/craft-dockerfile.tmp "${DEPLOY_DIR}/Dockerfile"
EOF

echo "    Uploading docker-entrypoint.sh..."
scp "${SSH_OPTS[@]}" "${ENTRYPOINT_PATH}" "${TARGET}:/tmp/craft-entrypoint.tmp"
ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    DEPLOY_DIR="$1"
    mv /tmp/craft-entrypoint.tmp "${DEPLOY_DIR}/docker-entrypoint.sh"
    chmod +x "${DEPLOY_DIR}/docker-entrypoint.sh"
EOF


COMPOSE_CONTENT="services:
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
      # Minecraft Java Edition
      - \"25565:25565\"
      # Minecraft Bedrock Edition
      - \"19132:19132/udp\"
      # Minecraft RCON Remote Console
      - \"25575:25575\"
      # Craft Daemon Web / Dynmap / BlueMap
      - \"8123:8123\"
    volumes:
      - ./craft-data:/craft
    environment:
      - CRAFT_HOME=/craft
      - TZ=UTC
    healthcheck:
      test: [\"CMD-SHELL\", \"craft ls || exit 0\"]
      interval: 20s
      timeout: 5s
      retries: 3
      start_period: 15s
"

ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << EOF
    DEPLOY_DIR="\$1"
    cat << 'EOC' > "\${DEPLOY_DIR}/docker-compose.yml"
${COMPOSE_CONTENT}
EOC
EOF
echo -e "${GREEN}[OK] Dockerfile and docker-compose.yml deployed to remote.${NC}"

# 9. Build image and launch the Craft Docker stack
echo -e "${CYAN}==> Building Craft container image on remote VDS...${NC}"
ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    set -euo pipefail
    DEPLOY_DIR="$1"
    cd "${DEPLOY_DIR}"
    if docker compose version >/dev/null 2>&1; then
        docker compose build
        echo "==> Launching container stack..."
        docker compose up -d
    else
        docker-compose build
        echo "==> Launching container stack..."
        docker-compose up -d
    fi
EOF

# 10. Check container status
echo -e "${CYAN}==> Checking running container status...${NC}"
sleep 2
REMOTE_PS="$(ssh "${SSH_OPTS[@]}" "${TARGET}" "${SUDO_PREFIX}bash -s -- \"${REMOTE_DEPLOY_DIR}\"" << 'EOF'
    DEPLOY_DIR="$1"
    cd "${DEPLOY_DIR}"
    docker compose ps 2>/dev/null || docker-compose ps 2>/dev/null || docker ps --filter name=craft
EOF
)"
echo "${REMOTE_PS}"

# 11. Register host into local Craft remotes registry if craft CLI is present
CRAFT_CLI_LOCAL="$(command -v craft 2>/dev/null || echo "${PROJECT_ROOT}/bin/craft")"
if [ -x "${CRAFT_CLI_LOCAL}" ]; then
    echo -e "${CYAN}==> Synchronizing host into local Craft remote registry...${NC}"
    # Check if alias already registered
    if ! "${CRAFT_CLI_LOCAL}" remote ls 2>/dev/null | grep -qw "${TARGET}"; then
        "${CRAFT_CLI_LOCAL}" remote add "${TARGET}" "${RESOLVED_USER}@${RESOLVED_HOST}:${RESOLVED_PORT}" 2>/dev/null || true
        echo -e "${GREEN}[OK] Registered '${TARGET}' in local Craft registry.${NC}"
    else
        echo "    '${TARGET}' already registered in local Craft registry."
    fi
fi

# 12. Final Summary
echo ""
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e "${GREEN}${BOLD}       Craft VDS Docker Setup Completed Successfully!             ${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo ""
echo -e "  Host Alias:       ${BOLD}${TARGET}${NC} (${RESOLVED_USER}@${RESOLVED_HOST}:${RESOLVED_PORT})"
echo -e "  Deploy Path:      ${REMOTE_DEPLOY_DIR}"
echo -e "  Container Name:   craft"
echo -e "  Minecraft Java:   Port 25565 (TCP)"
echo -e "  Minecraft Bedrock:Port 19132 (UDP)"
echo -e "  RCON Console:     Port 25575 (TCP)"
echo -e "  Web / BlueMap:    Port 8123  (TCP)"
echo ""
echo -e "${BOLD}Next Steps:${NC}"
echo -e "  1. Connect directly to VDS:       ${CYAN}ssh ${TARGET}${NC}"
echo -e "  2. Test remote connection:        ${CYAN}craft remote test ${TARGET}${NC}"
echo -e "  3. Stream remote TUI dashboard:   ${CYAN}craft ui --remote ${TARGET}${NC}"
echo -e "  4. Create a server on VDS:        ${CYAN}craft new my-server --remote ${TARGET}${NC}"
echo -e "  5. View container logs:           ${CYAN}ssh ${TARGET} 'docker compose -f ${REMOTE_DEPLOY_DIR}/docker-compose.yml logs -f'${NC}"
echo ""
