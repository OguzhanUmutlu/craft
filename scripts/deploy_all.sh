#!/usr/bin/env bash
set -e

# ==============================================================================
# Craft Unified Deployment Orchestrator (Executables & Docs)
# ==============================================================================

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${CYAN}${BOLD}"
echo "=================================================================="
echo "          Craft Dual-Worker Deployment Orchestration             "
echo "=================================================================="
echo -e "${NC}"

echo -e "${BLUE}==> [1/2] Deploying Executables & Release Binaries...${NC}"
"${SCRIPT_DIR}/deploy_executables.sh" "$@"

echo ""
echo -e "${BLUE}==> [2/2] Deploying Documentation Portal...${NC}"
"${SCRIPT_DIR}/deploy_docs.sh" "$@"

echo ""
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e "${GREEN}${BOLD}[OK] All Craft release assets and documentation successfully prepared!${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
