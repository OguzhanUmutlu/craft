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

INSTALL_MODE="cli"
for arg in "$@"; do
  case "$arg" in
    --ui|--gui)
      INSTALL_MODE="ui"
      ;;
  esac
done

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
  local silent="${3:-false}"
  if command -v curl &>/dev/null; then
    if [ "$silent" = true ]; then
      curl -fsSL "$url" -o "$dest" 2>/dev/null
    else
      curl -fsSL "$url" -o "$dest"
    fi
  elif command -v wget &>/dev/null; then
    if [ "$silent" = true ]; then
      wget -qO "$dest" "$url" 2>/dev/null
    else
      wget -qO "$dest" "$url"
    fi
  else
    echo -e "${RED}Error: neither 'curl' nor 'wget' is available.${NC}"
    exit 1
  fi
}

if [ "$INSTALL_MODE" = "ui" ]; then
  echo -e "${BLUE}==> Selected package:${NC} ${CYAN}Craft Desktop Studio${NC}"
  TARGET_ARCHIVE="craft-studio-${OS_TYPE}-${ARCH_TYPE}.tar.gz"
  UI_DOWNLOAD_URL="${BASE_URL}/download/ui?platform=${OS_TYPE}-${ARCH_TYPE}"

  TMP_ARCHIVE="${TMP_DIR}/${TARGET_ARCHIVE}"
  echo -e "${BLUE}==> Fetching Craft Desktop Studio archive...${NC}"
  fetch_file "$UI_DOWNLOAD_URL" "$TMP_ARCHIVE" false

  APP_DIR="${HOME}/.local/share/craft-studio"
  BIN_DIR="${HOME}/.local/bin"
  if [ -w "/usr/local/bin" ] && [ -w "/opt" ] && [ "$EUID" -eq 0 ]; then
    APP_DIR="/opt/craft-studio"
    BIN_DIR="/usr/local/bin"
  fi

  mkdir -p "$APP_DIR" "$BIN_DIR"
  echo -e "${BLUE}==> Extracting package to ${APP_DIR}...${NC}"
  tar -xzf "$TMP_ARCHIVE" -C "$APP_DIR" --strip-components=1 2>/dev/null || tar -xzf "$TMP_ARCHIVE" -C "$APP_DIR"

  chmod +x "${APP_DIR}/craft-studio" "${APP_DIR}/craft-studio-bin" "${APP_DIR}/craft" 2>/dev/null || true

  ln -sf "${APP_DIR}/craft-studio" "${BIN_DIR}/craft-studio"
  ln -sf "${APP_DIR}/craft" "${BIN_DIR}/craft"

  DESKTOP_DIR="${HOME}/.local/share/applications"
  if [ -d "$DESKTOP_DIR" ] || mkdir -p "$DESKTOP_DIR" 2>/dev/null; then
    if [ -f "${APP_DIR}/craft-studio.desktop" ]; then
      cp -f "${APP_DIR}/craft-studio.desktop" "${DESKTOP_DIR}/"
    fi
  fi

  ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"
  if mkdir -p "$ICON_DIR" 2>/dev/null; then
    if [ -f "${APP_DIR}/icons/128x128.png" ]; then
      cp -f "${APP_DIR}/icons/128x128.png" "${ICON_DIR}/craft-studio.png"
    fi
  fi

  echo -e "${GREEN}${BOLD}[OK] Craft Desktop Studio installed successfully!${NC}"
  echo -e "  Launcher:  ${CYAN}${BIN_DIR}/craft-studio${NC}"
  echo -e "  CLI tool:  ${CYAN}${BIN_DIR}/craft${NC}"
  echo -e "  Location:  ${CYAN}${APP_DIR}${NC}"
  echo ""
  echo -e "Launch Craft Studio with:"
  echo -e "  ${YELLOW}craft-studio${NC}"
  exit 0
fi

INSTALLED=false

if [ "$COMPRESSION" = "zst" ]; then
  TMP_ZST="${TMP_DIR}/craft.zst"
  if fetch_file "${DOWNLOAD_URL}.zst" "${TMP_ZST}" true; then
    if zstd -d -q -f "${TMP_ZST}" -o "${TMP_FILE}" 2>/dev/null; then
      INSTALLED=true
      echo -e "${BLUE}==> Downloaded compressed package (${CYAN}zstd${BLUE}, ~4.9 MB, saves 68% bandwidth)${NC}"
    fi
  fi
fi

if [ "$INSTALLED" = false ] && { [ "$COMPRESSION" = "gz" ] || [ "$COMPRESSION" = "zst" ]; }; then
  if command -v gzip &> /dev/null; then
    TMP_GZ="${TMP_DIR}/craft.gz"
    if fetch_file "${DOWNLOAD_URL}.gz" "${TMP_GZ}" true; then
      if gzip -d -c "${TMP_GZ}" > "${TMP_FILE}" 2>/dev/null; then
        INSTALLED=true
        echo -e "${BLUE}==> Downloaded compressed package (${CYAN}gzip${BLUE}, ~6.0 MB, saves 60% bandwidth)${NC}"
      fi
    fi
  fi
fi

if [ "$INSTALLED" = false ]; then
  echo -e "${BLUE}==> Fetching Craft executable...${NC}"
  fetch_file "$DOWNLOAD_URL" "$TMP_FILE" false
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
