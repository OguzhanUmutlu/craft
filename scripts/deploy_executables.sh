#!/usr/bin/env bash
set -e

# ==============================================================================
# Craft Executables & Binaries Deployment Tool (Cloudflare Workers)
# ==============================================================================

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXECUTABLES_DIR="${ROOT_DIR}/executables"
DIST_DIR="${EXECUTABLES_DIR}/dist"

DRY_RUN=false
SKIP_BUILD=false
CI_MODE=false
CUSTOM_VERSION=""

# Parse command-line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            CUSTOM_VERSION="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --ci)
            CI_MODE=true
            shift
            ;;
        -h|--help)
            echo -e "${BOLD}Usage:${NC} ./scripts/deploy_executables.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --version <v>   Specify release version (defaults to version in Cargo.toml)"
            echo "  --dry-run       Build and assemble distribution directory without deploying"
            echo "  --skip-build    Skip 'cargo build --release' and assemble with existing binaries"
            echo "  --ci            Run in non-interactive CI mode"
            echo "  -h, --help      Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

banner() {
    echo -e "${CYAN}${BOLD}"
    echo "  ____            __ _     ____  _             "
    echo " / ___|_ __ __ _ / _| |_  | __ )(_)_ __  ___   "
    echo "| |   | '__/ _\` | |_| __| |  _ \| | '_ \/ __|  "
    echo "| |___| | | (_| |  _| |_  | |_) | | | | \__ \  "
    echo " \____|_|  \__,_|_|  \__| |____/|_|_| |_|___/  "
    echo "  Executables & Release Distribution Packager  "
    echo -e "${NC}"
}

banner

# Determine version
if [ -n "$CUSTOM_VERSION" ]; then
    VERSION="$CUSTOM_VERSION"
else
    VERSION=$(grep -m1 '^version = ' "${ROOT_DIR}/Cargo.toml" | cut -d '"' -f2)
fi

echo -e "${BLUE}==> Target release version:${NC} ${GREEN}${BOLD}v${VERSION}${NC}"

# Step 1: Compile Rust release binary if not skipped
if [ "$SKIP_BUILD" = false ]; then
    echo -e "${BLUE}==> Building release binary with Cargo (Linux x86_64)...${NC}"
    cd "${ROOT_DIR}"
    cargo build --release
    mkdir -p "${ROOT_DIR}/bin"
    cp -f "${ROOT_DIR}/target/release/craft" "${ROOT_DIR}/bin/craft"
    echo -e "${GREEN}[OK] Local binary compiled and installed to bin/craft${NC}"
fi

# Step 2: Assemble distribution directory
echo -e "${BLUE}==> Assembling release artifacts in ${DIST_DIR}...${NC}"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"
mkdir -p "${DIST_DIR}/releases/v${VERSION}"
mkdir -p "${DIST_DIR}/releases/latest"
mkdir -p "${DIST_DIR}/downloads"

# Copy landing page
cp -f "${EXECUTABLES_DIR}/index.html" "${DIST_DIR}/index.html"

# Write plain text version file
echo -n "${VERSION}" > "${DIST_DIR}/version"

# Copy installation scripts and docker-compose
if [ -f "${ROOT_DIR}/docs/public/install.sh" ]; then
    cp -f "${ROOT_DIR}/docs/public/install.sh" "${DIST_DIR}/install.sh"
fi
if [ -f "${ROOT_DIR}/docs/public/install.ps1" ]; then
    cp -f "${ROOT_DIR}/docs/public/install.ps1" "${DIST_DIR}/install.ps1"
fi
if [ -f "${ROOT_DIR}/docker-compose.yml" ]; then
    cp -f "${ROOT_DIR}/docker-compose.yml" "${DIST_DIR}/docker-compose.yml"
fi

# Package Linux binary
if [ -f "${ROOT_DIR}/target/release/craft" ]; then
    LINUX_BIN="${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64"
    cp -f "${ROOT_DIR}/target/release/craft" "${LINUX_BIN}"
    chmod +x "${LINUX_BIN}"
    tar -czf "${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64.tar.gz" -C "${DIST_DIR}/releases/v${VERSION}" craft-linux-amd64
    gzip -c "${LINUX_BIN}" > "${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64.gz"
    
    # Checksum
    (cd "${DIST_DIR}/releases/v${VERSION}" && sha256sum craft-linux-amd64 > craft-linux-amd64.sha256)
    
    # Copy to latest & downloads
    cp -f "${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64"* "${DIST_DIR}/releases/latest/"
    cp -f "${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64"* "${DIST_DIR}/downloads/"
    echo -e "${GREEN}[OK] Packaged Linux x86_64 binaries & archives${NC}"
fi

# Copy any existing pre-compiled Windows or macOS binaries from docs/public/downloads if available
EXISTING_DL="${ROOT_DIR}/docs/public/downloads"
if [ -d "${EXISTING_DL}" ]; then
    for f in "${EXISTING_DL}"/craft-windows-*; do
        if [ -f "$f" ]; then
            fname=$(basename "$f")
            cp -f "$f" "${DIST_DIR}/releases/v${VERSION}/${fname}"
            cp -f "$f" "${DIST_DIR}/releases/latest/${fname}"
            cp -f "$f" "${DIST_DIR}/downloads/${fname}"
        fi
    done
    for f in "${EXISTING_DL}"/craft-darwin-*; do
        if [ -f "$f" ]; then
            fname=$(basename "$f")
            cp -f "$f" "${DIST_DIR}/releases/v${VERSION}/${fname}"
            cp -f "$f" "${DIST_DIR}/releases/latest/${fname}"
            cp -f "$f" "${DIST_DIR}/downloads/${fname}"
        fi
    done
fi

# Generate releases.json manifest
LINUX_SHA=$(cat "${DIST_DIR}/releases/v${VERSION}/craft-linux-amd64.sha256" 2>/dev/null | awk '{print $1}' || echo "")
UPDATED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

cat <<EOF > "${DIST_DIR}/releases.json"
{
  "latest": "${VERSION}",
  "updated_at": "${UPDATED_AT}",
  "releases": {
    "${VERSION}": {
      "version": "${VERSION}",
      "release_date": "${UPDATED_AT}",
      "files": {
        "craft-linux-amd64": {
          "url": "/releases/v${VERSION}/craft-linux-amd64",
          "sha256": "${LINUX_SHA}"
        },
        "craft-linux-amd64.tar.gz": {
          "url": "/releases/v${VERSION}/craft-linux-amd64.tar.gz"
        },
        "craft-windows-amd64.exe": {
          "url": "/releases/v${VERSION}/craft-windows-amd64.exe"
        },
        "craft-windows-amd64.zip": {
          "url": "/releases/v${VERSION}/craft-windows-amd64.zip"
        },
        "craft-darwin-arm64.tar.gz": {
          "url": "/releases/v${VERSION}/craft-darwin-arm64.tar.gz"
        },
        "craft-darwin-amd64.tar.gz": {
          "url": "/releases/v${VERSION}/craft-darwin-amd64.tar.gz"
        }
      }
    }
  }
}
EOF

echo -e "${GREEN}[OK] Generated releases.json manifest and /version file${NC}"

# Dry run check
if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}[DRY RUN] Distribution bundle assembled at ${DIST_DIR}.${NC}"
    echo -e "${YELLOW}[DRY RUN] Skipping Cloudflare deployment.${NC}"
    exit 0
fi

# Step 3: Deploy via Wrangler
echo ""
echo -e "${BLUE}==> Deploying craft-executables to Cloudflare Workers...${NC}"
cd "${EXECUTABLES_DIR}"

if [ "$CI_MODE" = false ] && [ -z "$CLOUDFLARE_API_TOKEN" ]; then
    if ! npx --yes wrangler whoami &> /dev/null; then
        echo -e "${YELLOW}Wrangler authentication required. Initiating login...${NC}"
        npx --yes wrangler login
    fi
fi

npx --yes wrangler deploy

echo ""
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e "${GREEN}${BOLD}[OK] Craft Executables successfully deployed to Cloudflare Workers!${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e " Version:        ${CYAN}${BOLD}v${VERSION}${NC}"
echo -e " Source:         ${YELLOW}${DIST_DIR}${NC}"
echo -e " Worker:         ${CYAN}craft-executables${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
