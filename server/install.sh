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

# Determine best compression format based on local utilities
COMPRESSION="raw"
if command -v zstd &> /dev/null; then
  COMPRESSION="zst"
elif command -v gzip &> /dev/null; then
  COMPRESSION="gz"
fi

echo -e "${BLUE}==> Detected system:${NC} ${OS_TYPE} (${ARCH_TYPE})"

TMP_DIR="$(mktemp -d)"
TMP_FILE="${TMP_DIR}/craft"
trap 'rm -rf "$TMP_DIR"' EXIT

fetch_file() {
  local url="$1"
  local dest="$2"
  if command -v curl &>/dev/null; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget &>/dev/null; then
    wget -qO "$dest" "$url"
  else
    echo -e "${RED}Error: neither 'curl' nor 'wget' is available.${NC}"
    exit 1
  fi
}

INSTALLED=false

if [ "$COMPRESSION" = "zst" ]; then
  echo -e "${BLUE}==> Fetching compressed package (${CYAN}zstd${BLUE}, ~4.9 MB, saves 68% bandwidth)...${NC}"
  TMP_ZST="${TMP_DIR}/craft.zst"
  if fetch_file "${DOWNLOAD_URL}.zst" "${TMP_ZST}"; then
    if zstd -d -q -f "${TMP_ZST}" -o "${TMP_FILE}" 2>/dev/null; then
      INSTALLED=true
    fi
  fi
fi

if [ "$INSTALLED" = false ] && { [ "$COMPRESSION" = "gz" ] || [ "$COMPRESSION" = "zst" ]; }; then
  if command -v gzip &> /dev/null; then
    echo -e "${BLUE}==> Fetching compressed package (${CYAN}gzip${BLUE}, ~6.0 MB, saves 60% bandwidth)...${NC}"
    TMP_GZ="${TMP_DIR}/craft.gz"
    if fetch_file "${DOWNLOAD_URL}.gz" "${TMP_GZ}"; then
      if gzip -d -c "${TMP_GZ}" > "${TMP_FILE}" 2>/dev/null; then
        INSTALLED=true
      fi
    fi
  fi
fi

if [ "$INSTALLED" = false ]; then
  echo -e "${BLUE}==> Fetching Craft executable...${NC}"
  fetch_file "$DOWNLOAD_URL" "$TMP_FILE"
fi

chmod +x "$TMP_FILE"

INSTALL_DIR="/usr/local/bin"
if [ ! -w "$INSTALL_DIR" ]; then
  echo -e "${YELLOW}==> Elevated permissions required to install to ${INSTALL_DIR}...${NC}"
  sudo mv "$TMP_FILE" "${INSTALL_DIR}/craft"
else
  mv "$TMP_FILE" "${INSTALL_DIR}/craft"
fi

echo -e "${GREEN}${BOLD}[OK] Craft installed successfully to ${INSTALL_DIR}/craft!${NC}"
echo ""
echo -e "Get started by creating a new server:"
echo -e "  ${YELLOW}craft new paper 1.21.4 survival --memory 4G${NC}"
echo ""
echo -e "Explore all available commands:"
echo -e "  ${YELLOW}craft --help${NC}"
