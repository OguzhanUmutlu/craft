#!/usr/bin/env bash
# ==============================================================================
# Craft Universal Static One-Line Installer (Linux & macOS)
# https://craft.oguzhanumutlu.com
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
echo "  Craft Native Standalone Installer"
echo -e "${NC}"

GITHUB_REPO="OguzhanUmutlu/craft"

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
    echo -e "${RED}Error: Unsupported operating system '$OS'. Please download manually from https://craft.oguzhanumutlu.com.${NC}"
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

# Determine best compression format based on local utilities
COMPRESSION="raw"
if command -v zstd &> /dev/null; then
  COMPRESSION="zst"
elif command -v gzip &> /dev/null; then
  COMPRESSION="gz"
fi

if [ -n "${CRAFT_DOWNLOAD_URL:-}" ]; then
  DOWNLOAD_URL="${CRAFT_DOWNLOAD_URL}"
  COMPRESSION="raw"
elif [ -n "${CRAFT_VERSION:-}" ]; then
  # Strip leading 'v' if user typed CRAFT_VERSION=v0.1.0
  CLEAN_VERSION="${CRAFT_VERSION#v}"
  DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/v${CLEAN_VERSION}/${TARGET_NAME}"
  echo -e "${BLUE}==> Target version requested:${NC} ${GREEN}v${CLEAN_VERSION}${NC}"
elif [ -n "${CRAFT_BASE_URL:-}" ]; then
  DOWNLOAD_URL="${CRAFT_BASE_URL}/downloads/${TARGET_NAME}"
else
  DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/latest/download/${TARGET_NAME}"
fi

echo -e "${BLUE}==> Detected system:${NC} ${OS_TYPE} (${ARCH_TYPE})"

# Determine installation directory
if [ -w "/usr/local/bin" ] && [ "${EUID:-$(id -u)}" -eq 0 ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "${INSTALL_DIR}"
fi

DEST_FILE="${INSTALL_DIR}/craft"
TMP_FILE="$(mktemp "${TMPDIR:-/tmp}/craft.XXXXXX")"

fetch_file() {
  local url="$1"
  local dest="$2"
  if command -v curl &> /dev/null; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget &> /dev/null; then
    wget -qO "$dest" "$url"
  else
    echo -e "${RED}Error: Neither curl nor wget was found on your system.${NC}"
    exit 1
  fi
}

INSTALLED=false

if [ "$COMPRESSION" = "zst" ]; then
  echo -e "${BLUE}==> Downloading compressed package (${CYAN}zstd${BLUE}, ~4.9 MB, saves 68% bandwidth)...${NC}"
  TMP_ZST="${TMP_FILE}.zst"
  if fetch_file "${DOWNLOAD_URL}.zst" "${TMP_ZST}"; then
    if zstd -d -q -f "${TMP_ZST}" -o "${TMP_FILE}" 2>/dev/null; then
      rm -f "${TMP_ZST}"
      INSTALLED=true
    fi
  fi
  rm -f "${TMP_ZST}" 2>/dev/null || true
fi

if [ "$INSTALLED" = false ] && { [ "$COMPRESSION" = "gz" ] || [ "$COMPRESSION" = "zst" ]; }; then
  if command -v gzip &> /dev/null; then
    echo -e "${BLUE}==> Downloading compressed package (${CYAN}gzip${BLUE}, ~6.0 MB, saves 60% bandwidth)...${NC}"
    TMP_GZ="${TMP_FILE}.gz"
    if fetch_file "${DOWNLOAD_URL}.gz" "${TMP_GZ}"; then
      if gzip -d -c "${TMP_GZ}" > "${TMP_FILE}" 2>/dev/null; then
        rm -f "${TMP_GZ}"
        INSTALLED=true
      fi
    fi
    rm -f "${TMP_GZ}" 2>/dev/null || true
  fi
fi

if [ "$INSTALLED" = false ]; then
  echo -e "${BLUE}==> Fetching Craft executable from:${NC} ${DOWNLOAD_URL}"
  fetch_file "${DOWNLOAD_URL}" "${TMP_FILE}"
fi

chmod +x "${TMP_FILE}"
mv "${TMP_FILE}" "${DEST_FILE}"

echo -e "${GREEN}${BOLD}[OK] Craft successfully installed to ${DEST_FILE}!${NC}"

# Check PATH
case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo -e "${YELLOW}Notice: ${INSTALL_DIR} is not in your current PATH.${NC}"
    echo "To access 'craft' globally, add this to your shell profile (~/.bashrc, ~/.zshrc):"
    echo -e "  ${CYAN}export PATH=\"\$PATH:${INSTALL_DIR}\"${NC}"
    ;;
esac

echo ""
echo -e "Run '${CYAN}${BOLD}craft --help${NC}' to get started."
