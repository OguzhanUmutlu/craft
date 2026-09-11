#!/usr/bin/env bash
set -e

# ANSI styling
BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR"

banner() {
    echo -e "${CYAN}${BOLD}"
    echo "  ____            __ _   "
    echo " / ___|_ __ __ _ / _| |_ "
    echo "| |   | '__/ _\` | |_| __|"
    echo "| |___| | | (_| |  _| |_ "
    echo " \____|_|  \__,_|_|  \__|"
    echo "  Docker Stack Orchestrator"
    echo -e "${NC}"
}

check_docker() {
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}Error: 'docker' is not installed or not in PATH.${NC}"
        echo "Please install Docker from https://docs.docker.com/get-docker/"
        exit 1
    fi

    if ! docker compose version &> /dev/null; then
        echo -e "${RED}Error: 'docker compose' plugin is not available.${NC}"
        echo "Please install Docker Compose V2."
        exit 1
    fi
}

cmd_up() {
    check_docker
    echo -e "${BLUE}==> Deploying Craft container stack...${NC}"
    docker compose up -d "$@"
    echo -e "${GREEN}[OK] Craft container deployed successfully!${NC}"
    echo ""
    docker compose ps
    echo ""
    echo -e "Attach to console:  ${YELLOW}./scripts/deploy.sh shell${NC}"
    echo -e "Stream live logs:   ${YELLOW}./scripts/deploy.sh logs${NC}"
    echo -e "Stop containers:    ${YELLOW}./scripts/deploy.sh down${NC}"
}

cmd_down() {
    check_docker
    echo -e "${YELLOW}==> Tearing down Craft container stack...${NC}"
    docker compose down "$@"
    echo -e "${GREEN}[OK] Craft containers stopped and removed.${NC}"
}

cmd_restart() {
    check_docker
    echo -e "${BLUE}==> Restarting Craft container...${NC}"
    docker compose restart
    docker compose ps
}

cmd_logs() {
    check_docker
    docker compose logs -f "$@"
}

cmd_status() {
    check_docker
    docker compose ps "$@"
}

cmd_shell() {
    check_docker
    if ! docker compose ps --status running -q craft &> /dev/null; then
        echo -e "${RED}Craft container is not running. Starting it now...${NC}"
        docker compose up -d
    fi
    echo -e "${BLUE}==> Attaching interactive shell to container...${NC}"
    docker compose exec -it craft bash
}

cmd_build() {
    check_docker
    echo -e "${BLUE}==> Building Craft Docker image...${NC}"
    DOCKER_BUILDKIT=1 docker compose build "$@"
    echo -e "${GREEN}[OK] Image built successfully!${NC}"
}

cmd_help() {
    banner
    echo -e "${BOLD}Usage:${NC} ./scripts/deploy.sh [COMMAND] [OPTIONS]"
    echo ""
    echo -e "${BOLD}Commands:${NC}"
    echo -e "  ${GREEN}up${NC}        Deploy and start the Craft stack in background"
    echo -e "  ${GREEN}down${NC}      Stop and clean up containers"
    echo -e "  ${GREEN}restart${NC}   Restart running Craft containers"
    echo -e "  ${GREEN}logs${NC}      Stream real-time logs from Craft and servers"
    echo -e "  ${GREEN}status${NC}    Show container status, health, and port bindings"
    echo -e "  ${GREEN}shell${NC}     Open an interactive bash shell in the container"
    echo -e "  ${GREEN}build${NC}     Rebuild container image from local workspace"
    echo -e "  ${GREEN}help${NC}      Show this usage guide"
    echo ""
}

case "$1" in
    up)
        shift
        cmd_up "$@"
        ;;
    down)
        shift
        cmd_down "$@"
        ;;
    restart)
        shift
        cmd_restart
        ;;
    logs)
        shift
        cmd_logs "$@"
        ;;
    status)
        shift
        cmd_status "$@"
        ;;
    shell)
        shift
        cmd_shell
        ;;
    build)
        shift
        cmd_build "$@"
        ;;
    help|--help|-h|"")
        cmd_help
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        cmd_help
        exit 1
        ;;
esac
