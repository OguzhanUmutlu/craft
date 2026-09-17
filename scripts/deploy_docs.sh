#!/usr/bin/env bash
set -e

# ==============================================================================
# Craft Documentation & Portal Build & Deployment Tool (GitHub Pages)
# ==============================================================================

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCS_DIR="${ROOT_DIR}/docs"

DRY_RUN=false
SUBCOMMAND="deploy"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        build|preview|dev|status|trigger|help|--help|-h)
            SUBCOMMAND="$1"
            shift
            ;;
        deploy)
            SUBCOMMAND="deploy"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

banner() {
    echo -e "${CYAN}${BOLD}"
    echo "  ____            __ _    ____                 "
    echo " / ___|_ __ __ _ / _| |_ |  _ \  ___   ___ ___ "
    echo "| |   | '__/ _\` | |_| __|| | | |/ _ \ / __/ __|"
    echo "| |___| | | (_| |  _| |_ | |_| | (_) | (__\__ \\"
    echo " \____|_|  \__,_|_|  \__||____/ \___/ \___|___/"
    echo "  Documentation Portal (GitHub Pages)          "
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

cmd_build() {
    check_prereqs
    echo -e "${BLUE}==> Building production static bundle (Vite + React + PostCSS)...${NC}"
    cd "${DOCS_DIR}" && npm run build
    echo -e "${GREEN}[OK] Build completed successfully! Assets located in docs/dist/${NC}"
}

cmd_preview() {
    check_prereqs
    cmd_build
    echo -e "${BLUE}==> Starting local preview server...${NC}"
    cd "${DOCS_DIR}" && npm run preview
}

cmd_status() {
    if command -v gh &> /dev/null; then
        echo -e "${BLUE}==> Checking latest GitHub Actions deployment runs...${NC}"
        gh run list --workflow=deploy-docs.yml --limit 5
    else
        echo -e "${YELLOW}GitHub CLI (gh) is not installed.${NC}"
        echo "You can check workflow status online at:"
        echo -e "${CYAN}https://github.com/OguzhanUmutlu/craft/actions/workflows/deploy-docs.yml${NC}"
    fi
}

cmd_deploy() {
    banner
    check_prereqs
    cmd_build

    if [ "$DRY_RUN" = true ]; then
        echo -e "${YELLOW}[DRY RUN] Docs build verified. Skipping deployment trigger.${NC}"
        exit 0
    fi

    echo ""
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
    echo -e "${GREEN}${BOLD}[OK] Documentation static bundle compiled and verified!${NC}"
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
    echo -e " Target domain: ${CYAN}${BOLD}https://craft.oguzhanumutlu.com${NC}"
    echo -e " Assets source: ${YELLOW}${DOCS_DIR}/dist${NC}"
    echo -e " CI/CD Engine:  ${BLUE}GitHub Actions (deploy-docs.yml)${NC}"
    echo -e "${GREEN}${BOLD}==================================================================${NC}"
    echo ""

    if command -v gh &> /dev/null; then
        echo -e "${BLUE}==> Triggering GitHub Pages deployment workflow via GitHub CLI...${NC}"
        if gh workflow run deploy-docs.yml 2>/dev/null; then
            echo -e "${GREEN}[OK] GitHub Actions workflow dispatched successfully!${NC}"
            echo -e "View live run: ${CYAN}gh run watch${NC}"
            return 0
        fi
    fi

    echo -e "${BLUE}==> Notice: Deployments are automated via GitHub Actions on push to main.${NC}"
    echo -e "To deploy to production, simply commit and push your changes:"
    echo -e "  ${CYAN}git add docs/ && git commit -m \"Update docs\" && git push origin main${NC}"
    echo ""
    echo -e "Monitor live workflow runs at:"
    echo -e "  ${CYAN}https://github.com/OguzhanUmutlu/craft/actions/workflows/deploy-docs.yml${NC}"
}

cmd_help() {
    banner
    echo -e "${BOLD}Usage:${NC} ./scripts/deploy_docs.sh [COMMAND] [OPTIONS]"
    echo ""
    echo -e "${BOLD}Commands:${NC}"
    echo -e "  ${GREEN}deploy${NC}    Build production assets and trigger/guide GitHub Pages deployment (default)"
    echo -e "  ${GREEN}build${NC}     Run Vite build and compile static assets to docs/dist"
    echo -e "  ${GREEN}preview${NC}   Build and run local preview server on localhost:4173"
    echo -e "  ${GREEN}status${NC}    Check GitHub Actions deployment run status"
    echo -e "  ${GREEN}help${NC}      Show this help message"
    echo ""
    echo -e "${BOLD}Options:${NC}"
    echo -e "  --dry-run   Build and verify static bundle without triggering deployment"
    echo ""
    echo -e "Target domain: ${CYAN}https://craft.oguzhanumutlu.com${NC}"
}

case "$SUBCOMMAND" in
    deploy|trigger)
        cmd_deploy
        ;;
    build)
        cmd_build
        ;;
    preview|dev)
        cmd_preview
        ;;
    status)
        cmd_status
        ;;
    help|--help|-h)
        cmd_help
        ;;
    *)
        echo -e "${RED}Unknown command: $SUBCOMMAND${NC}"
        cmd_help
        exit 1
        ;;
esac
