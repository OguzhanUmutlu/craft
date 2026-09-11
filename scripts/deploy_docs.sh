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

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCS_DIR="${ROOT_DIR}/docs"

banner() {
    echo -e "${CYAN}${BOLD}"
    echo "  ____            __ _    ____                 "
    echo " / ___|_ __ __ _ / _| |_ |  _ \  ___   ___ ___ "
    echo "| |   | '__/ _\` | |_| __|| | | |/ _ \ / __/ __|"
    echo "| |___| | | (_| |  _| |_ | |_| | (_) | (__\__ \\"
    echo " \____|_|  \__,_|_|  \__||____/ \___/ \___|___/"
    echo "  Cloudflare Workers Deployment Tool           "
    echo -e "${NC}"
}

check_prereqs() {
    if ! command -v node &> /dev/null; then
        echo -e "${RED}Error: 'node' is not installed or not in PATH.${NC}"
        echo "Please install Node.js (v18+) from https://nodejs.org/"
        exit 1
    fi

    if ! command -v npm &> /dev/null; then
        echo -e "${RED}Error: 'npm' is not installed or not in PATH.${NC}"
        exit 1
    fi

    if [ ! -d "${DOCS_DIR}/node_modules" ]; then
        echo -e "${YELLOW}==> Installing dependencies (node_modules not found)...${NC}"
        cd "${DOCS_DIR}" && npm install
    fi
}

cmd_whoami() {
    check_prereqs
    echo -e "${BLUE}==> Checking Cloudflare Wrangler authentication...${NC}"
    cd "${DOCS_DIR}" && npx wrangler whoami
}

cmd_login() {
    check_prereqs
    echo -e "${BLUE}==> Initiating Cloudflare Wrangler login...${NC}"
    cd "${DOCS_DIR}" && npx wrangler login
}

sync_static_assets() {
    echo -e "${BLUE}==> Synchronizing static deployment assets into docs/public...${NC}"
    mkdir -p "${DOCS_DIR}/public/downloads"

    # Sync docker-compose.yml
    if [ -f "${ROOT_DIR}/docker-compose.yml" ]; then
        cp "${ROOT_DIR}/docker-compose.yml" "${DOCS_DIR}/public/docker-compose.yml"
        echo -e "${GREEN}✓ Synced docker-compose.yml to docs/public/${NC}"
    fi

    # Sync Linux release binary & archive
    if [ -f "${ROOT_DIR}/target/release/craft" ]; then
        cp "${ROOT_DIR}/target/release/craft" "${DOCS_DIR}/public/downloads/craft-linux-amd64"
        tar -czf "${DOCS_DIR}/public/downloads/craft-linux-amd64.tar.gz" -C "${DOCS_DIR}/public/downloads" craft-linux-amd64
        echo -e "${GREEN}✓ Synced Linux x86_64 binary and archive${NC}"
    fi

    # Sync Windows release binary & zip
    if [ -f "${ROOT_DIR}/target/x86_64-pc-windows-gnu/release/craft.exe" ]; then
        cp "${ROOT_DIR}/target/x86_64-pc-windows-gnu/release/craft.exe" "${DOCS_DIR}/public/downloads/craft-windows-amd64.exe"
        (cd "${DOCS_DIR}/public/downloads" && cp craft-windows-amd64.exe craft.exe && zip -9 -q craft-windows-amd64.zip craft.exe && rm craft.exe)
        echo -e "${GREEN}✓ Synced Windows x64 binary and zip archive${NC}"
    fi

    # Sync macOS Apple Silicon binary & archive
    if [ -f "${ROOT_DIR}/target/aarch64-apple-darwin/release/craft" ]; then
        cp "${ROOT_DIR}/target/aarch64-apple-darwin/release/craft" "${DOCS_DIR}/public/downloads/craft-darwin-arm64"
        tar -czf "${DOCS_DIR}/public/downloads/craft-darwin-arm64.tar.gz" -C "${DOCS_DIR}/public/downloads" craft-darwin-arm64
        echo -e "${GREEN}✓ Synced macOS ARM64 binary and archive${NC}"
    fi

    # Sync macOS Intel binary & archive
    if [ -f "${ROOT_DIR}/target/x86_64-apple-darwin/release/craft" ]; then
        cp "${ROOT_DIR}/target/x86_64-apple-darwin/release/craft" "${DOCS_DIR}/public/downloads/craft-darwin-amd64"
        tar -czf "${DOCS_DIR}/public/downloads/craft-darwin-amd64.tar.gz" -C "${DOCS_DIR}/public/downloads" craft-darwin-amd64
        echo -e "${GREEN}✓ Synced macOS AMD64 binary and archive${NC}"
    fi
}

cmd_build() {
    check_prereqs
    sync_static_assets
    echo -e "${BLUE}==> Building production static bundle (Vite + React + PostCSS)...${NC}"
    cd "${DOCS_DIR}" && npm run build
    echo -e "${GREEN}✓ Build completed successfully! Assets located in docs/dist/${NC}"
}

cmd_preview() {
    check_prereqs
    cmd_build
    echo -e "${BLUE}==> Starting local edge preview server via Wrangler...${NC}"
    cd "${DOCS_DIR}" && npx wrangler dev
}

cmd_deploy() {
    banner
    check_prereqs

    echo -e "${BLUE}==> Verifying Cloudflare credentials...${NC}"
    if ! cd "${DOCS_DIR}" && npx wrangler whoami &> /dev/null; then
        echo -e "${YELLOW}Wrangler is not logged in. Launching login flow...${NC}"
        cd "${DOCS_DIR}" && npx wrangler login
    fi

    sync_static_assets
    echo ""
    echo -e "${BLUE}==> Compiling web application...${NC}"
    cd "${DOCS_DIR}" && npm run build

    echo ""
    echo -e "${BLUE}==> Deploying static assets to Cloudflare Workers...${NC}"
    cd "${DOCS_DIR}" && npx wrangler deploy "$@"

    echo ""
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
    echo -e "${GREEN}${BOLD}✓ Craft Portal successfully deployed to Cloudflare Workers!${NC}"
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
    echo -e "🌐 Live URL:      ${CYAN}${BOLD}https://craft.oguzhanumutlu.net${NC}"
    echo -e "📦 Assets Source: ${YELLOW}${DOCS_DIR}/dist${NC}"
    echo -e "⚙️  Config:        ${YELLOW}${DOCS_DIR}/wrangler.jsonc${NC}"
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
}

cmd_help() {
    banner
    echo -e "${BOLD}Usage:${NC} ./scripts/deploy_docs.sh [COMMAND]"
    echo ""
    echo -e "${BOLD}Commands:${NC}"
    echo -e "  ${GREEN}deploy${NC}    Build production assets and deploy to Cloudflare Workers (default)"
    echo -e "  ${GREEN}build${NC}     Run Vite build and compile static assets to docs/dist"
    echo -e "  ${GREEN}preview${NC}   Build and run local Cloudflare edge preview on localhost:8787"
    echo -e "  ${GREEN}whoami${NC}    Check authenticated Cloudflare account status"
    echo -e "  ${GREEN}login${NC}     Log in to Cloudflare via Wrangler OAuth"
    echo -e "  ${GREEN}help${NC}      Show this help message"
    echo ""
    echo -e "Target domain: ${CYAN}https://craft.oguzhanumutlu.net${NC}"
}

case "${1:-deploy}" in
    deploy)
        shift 2>/dev/null || true
        cmd_deploy "$@"
        ;;
    build)
        cmd_build
        ;;
    preview|dev)
        cmd_preview
        ;;
    whoami|status)
        cmd_whoami
        ;;
    login)
        cmd_login
        ;;
    help|--help|-h)
        cmd_help
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        echo ""
        cmd_help
        exit 1
        ;;
esac
