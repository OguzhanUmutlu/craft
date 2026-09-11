#!/usr/bin/env bash
# ==============================================================================
# Craft Universal One-Line Installer
# ==============================================================================
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}${BOLD}"
echo "  ____            __ _     ____    ___  "
echo " / ___|_ __ __ _ / _| |_  |___ \  / _ \ "
echo "| |   | '__/ _\` | |_| __|   __) || | | |"
echo "| |___| | | (_| |  _| |_   / __/ | |_| |"
echo " \____|_|  \__,_|_|  \__| |_____(_)___/ "
echo "  Craft Native Installer"
echo -e "${NC}"

BASE_URL="{{BASE_URL}}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    OS_TYPE="linux"
    ;;
  Darwin)
    OS_TYPE="darwin"
    ;;
  *)
    echo -e "${RED}Error: Unsupported operating system '$OS'. Please download manually.${NC}"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)
    ARCH_TYPE="amd64"
    ;;
  aarch64|arm64)
    ARCH_TYPE="arm64"
    ;;
  *)
    echo -e "${RED}Error: Unsupported architecture '$ARCH'.${NC}"
    exit 1
    ;;
esac

TARGET_NAME="craft-${OS_TYPE}-${ARCH_TYPE}"
DOWNLOAD_URL="${BASE_URL}/api/v1/download/${TARGET_NAME}"

echo -e "${BLUE}==> Detected system:${NC} ${OS_TYPE} (${ARCH_TYPE})"
echo -e "${BLUE}==> Fetching Craft executable...${NC}"

TMP_DIR="$(mktemp -d)"
TMP_FILE="${TMP_DIR}/craft"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl &>/dev/null; then
  curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_FILE"
elif command -v wget &>/dev/null; then
  wget -q --show-progress "$DOWNLOAD_URL" -O "$TMP_FILE"
else
  echo -e "${RED}Error: neither 'curl' nor 'wget' is available.${NC}"
  exit 1
fi

chmod +x "$TMP_FILE"

INSTALL_DIR="/usr/local/bin"
if [ ! -w "$INSTALL_DIR" ]; then
  echo -e "${YELLOW}==> Elevated permissions required to install to ${INSTALL_DIR}...${NC}"
  sudo mv "$TMP_FILE" "${INSTALL_DIR}/craft"
else
  mv "$TMP_FILE" "${INSTALL_DIR}/craft"
fi

echo -e "${GREEN}${BOLD}✓ Craft installed successfully to ${INSTALL_DIR}/craft!${NC}"
echo ""
echo -e "Get started by creating a new server:"
echo -e "  ${YELLOW}craft new paper 1.21.4 survival --memory 4G${NC}"
echo ""
echo -e "Explore all available commands:"
echo -e "  ${YELLOW}craft --help${NC}"
