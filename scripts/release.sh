#!/usr/bin/env bash
set -e

# ==============================================================================
# Craft Release Packager & GitHub Releases Publisher
# ==============================================================================

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASES_DIR="${ROOT_DIR}/releases"
DOCS_PUBLIC_DIR="${ROOT_DIR}/docs/public"

DRY_RUN=false
SKIP_BUILD=false
CI_MODE=false
DRAFT=false
CUSTOM_VERSION=""
LTS_VERSION="1.0.0"

# Parse command line options
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            CUSTOM_VERSION="$2"
            shift 2
            ;;
        --lts)
            LTS_VERSION="$2"
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
        --draft)
            DRAFT=true
            shift
            ;;
        --ci)
            CI_MODE=true
            shift
            ;;
        -h|--help)
            echo -e "${BOLD}Usage:${NC} ./scripts/release.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --version <v>   Specify release version (defaults to Cargo.toml)"
            echo "  --lts <v>       Specify LTS version (defaults to 1.0.0)"
            echo "  --dry-run       Build, package, and generate manifests locally without uploading to GitHub"
            echo "  --skip-build    Skip 'cargo build --release' and package with existing binaries"
            echo "  --draft         Create release as draft on GitHub"
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
    echo "  ____            __ _     ____      _                     "
    echo " / ___|_ __ __ _ / _| |_  |  _ \ ___| | ___  __ _ ___  ___ "
    echo "| |   | '__/ _\` | |_| __| | |_) / _ \ |/ _ \/ _\` / __|/ _ \\"
    echo "| |___| | | (_| |  _| |_  |  _ <  __/ |  __/ (_| \__ \  __/"
    echo " \____|_|  \__,_|_|  \__| |_| \_\___|_|\___|\__,_|___/\___|"
    echo "        GitHub Releases & Local Version Archive            "
    echo -e "${NC}"
}

banner

# Determine version
if [ -n "$CUSTOM_VERSION" ]; then
    VERSION="$CUSTOM_VERSION"
else
    VERSION=$(grep -m1 '^version = ' "${ROOT_DIR}/Cargo.toml" | cut -d '"' -f2)
fi

echo -e "${BLUE}==> Target version:${NC}     ${GREEN}${BOLD}v${VERSION}${NC}"
echo -e "${BLUE}==> LTS version:${NC}        ${CYAN}${BOLD}v${LTS_VERSION}${NC}"
echo -e "${BLUE}==> Dry run mode:${NC}       ${YELLOW}${DRY_RUN}${NC}"

# Step 1: Compile Rust release binary
if [ "$SKIP_BUILD" = false ]; then
    echo -e "${BLUE}==> [1/5] Compiling release binary with Cargo (Linux x86_64)...${NC}"
    cd "${ROOT_DIR}"
    cargo build --release
    mkdir -p "${ROOT_DIR}/bin"
    cp -f "${ROOT_DIR}/target/release/craft" "${ROOT_DIR}/bin/craft"
    echo -e "${GREEN}[OK] Compiled and updated bin/craft${NC}"
else
    echo -e "${YELLOW}==> [1/5] Skipping Cargo build step (--skip-build specified)${NC}"
fi

# Step 2: Assemble local versioned archive
echo -e "${BLUE}==> [2/5] Packaging versioned artifacts in ${RELEASES_DIR}/${VERSION}...${NC}"
TARGET_DIR="${RELEASES_DIR}/${VERSION}"
mkdir -p "${TARGET_DIR}"

# Package Linux binary
if [ -f "${ROOT_DIR}/target/release/craft" ]; then
    cp -f "${ROOT_DIR}/target/release/craft" "${TARGET_DIR}/craft-linux-amd64"
    chmod +x "${TARGET_DIR}/craft-linux-amd64"
    tar -czf "${TARGET_DIR}/craft-linux-amd64.tar.gz" -C "${TARGET_DIR}" craft-linux-amd64
    gzip -c "${TARGET_DIR}/craft-linux-amd64" > "${TARGET_DIR}/craft-linux-amd64.gz"
    (cd "${TARGET_DIR}" && sha256sum craft-linux-amd64 > craft-linux-amd64.sha256)
    echo -e "${GREEN}[OK] Packaged Linux x86_64${NC}"
fi

# Package Windows binary if present
WIN_SRC="${ROOT_DIR}/target/x86_64-pc-windows-gnu/release/craft.exe"
if [ -f "${WIN_SRC}" ]; then
    cp -f "${WIN_SRC}" "${TARGET_DIR}/craft-windows-amd64.exe"
    (cd "${TARGET_DIR}" && rm -f craft-windows-amd64.zip && zip -q craft-windows-amd64.zip craft-windows-amd64.exe)
    (cd "${TARGET_DIR}" && sha256sum craft-windows-amd64.exe > craft-windows-amd64.sha256)
    echo -e "${GREEN}[OK] Packaged Windows x86_64${NC}"
fi

# Package macOS ARM64 (Apple Silicon) if present
DARWIN_ARM_SRC="${ROOT_DIR}/target/aarch64-apple-darwin/release/craft"
if [ -f "${DARWIN_ARM_SRC}" ]; then
    cp -f "${DARWIN_ARM_SRC}" "${TARGET_DIR}/craft-darwin-arm64"
    chmod +x "${TARGET_DIR}/craft-darwin-arm64"
    tar -czf "${TARGET_DIR}/craft-darwin-arm64.tar.gz" -C "${TARGET_DIR}" craft-darwin-arm64
    (cd "${TARGET_DIR}" && sha256sum craft-darwin-arm64 > craft-darwin-arm64.sha256)
    rm -f "${TARGET_DIR}/craft-darwin-arm64"
    echo -e "${GREEN}[OK] Packaged macOS Apple Silicon (ARM64)${NC}"
fi

# Package macOS x86_64 (Intel) if present
DARWIN_AMD_SRC="${ROOT_DIR}/target/x86_64-apple-darwin/release/craft"
if [ -f "${DARWIN_AMD_SRC}" ]; then
    cp -f "${DARWIN_AMD_SRC}" "${TARGET_DIR}/craft-darwin-amd64"
    chmod +x "${TARGET_DIR}/craft-darwin-amd64"
    tar -czf "${TARGET_DIR}/craft-darwin-amd64.tar.gz" -C "${TARGET_DIR}" craft-darwin-amd64
    (cd "${TARGET_DIR}" && sha256sum craft-darwin-amd64 > craft-darwin-amd64.sha256)
    rm -f "${TARGET_DIR}/craft-darwin-amd64"
    echo -e "${GREEN}[OK] Packaged macOS Intel (x86_64)${NC}"
fi

# Step 3: Populate LTS directory (if not yet populated)
LTS_DIR="${RELEASES_DIR}/${LTS_VERSION}"
mkdir -p "${LTS_DIR}"
if [ ! -f "${LTS_DIR}/craft-linux-amd64.tar.gz" ]; then
    echo -e "${BLUE}==> Populating LTS archive in ${LTS_DIR}...${NC}"
    if [ -f "${ROOT_DIR}/target/release/craft" ]; then
        cp -f "${ROOT_DIR}/target/release/craft" "${LTS_DIR}/craft-linux-amd64"
        chmod +x "${LTS_DIR}/craft-linux-amd64"
        tar -czf "${LTS_DIR}/craft-linux-amd64.tar.gz" -C "${LTS_DIR}" craft-linux-amd64
        gzip -c "${LTS_DIR}/craft-linux-amd64" > "${LTS_DIR}/craft-linux-amd64.gz"
        (cd "${LTS_DIR}" && sha256sum craft-linux-amd64 > craft-linux-amd64.sha256)
    fi
    if [ -f "${WIN_SRC}" ]; then
        cp -f "${WIN_SRC}" "${LTS_DIR}/craft-windows-amd64.exe"
        (cd "${LTS_DIR}" && rm -f craft-windows-amd64.zip && zip -q craft-windows-amd64.zip craft-windows-amd64.exe)
        (cd "${LTS_DIR}" && sha256sum craft-windows-amd64.exe > craft-windows-amd64.sha256)
    fi
    if [ -f "${DARWIN_ARM_SRC}" ]; then
        cp -f "${DARWIN_ARM_SRC}" "${LTS_DIR}/craft-darwin-arm64"
        chmod +x "${LTS_DIR}/craft-darwin-arm64"
        tar -czf "${LTS_DIR}/craft-darwin-arm64.tar.gz" -C "${LTS_DIR}" craft-darwin-arm64
        (cd "${LTS_DIR}" && sha256sum craft-darwin-arm64 > craft-darwin-arm64.sha256)
        rm -f "${LTS_DIR}/craft-darwin-arm64"
    fi
    if [ -f "${DARWIN_AMD_SRC}" ]; then
        cp -f "${DARWIN_AMD_SRC}" "${LTS_DIR}/craft-darwin-amd64"
        chmod +x "${LTS_DIR}/craft-darwin-amd64"
        tar -czf "${LTS_DIR}/craft-darwin-amd64.tar.gz" -C "${LTS_DIR}" craft-darwin-amd64
        (cd "${LTS_DIR}" && sha256sum craft-darwin-amd64 > craft-darwin-amd64.sha256)
        rm -f "${LTS_DIR}/craft-darwin-amd64"
    fi
    echo -e "${GREEN}[OK] Populated LTS ${LTS_VERSION} archive${NC}"
fi

# Mirror target version into releases/latest/
LATEST_DIR="${RELEASES_DIR}/latest"
mkdir -p "${LATEST_DIR}"
cp -f "${TARGET_DIR}/craft-"* "${LATEST_DIR}/" 2>/dev/null || true
echo -e "${GREEN}[OK] Synced ${VERSION} artifacts into ${LATEST_DIR}${NC}"

# Helper to format file size
get_file_size() {
    local file="$1"
    if [ -f "$file" ]; then
        ls -lh "$file" | awk '{print $5}'
    else
        echo "N/A"
    fi
}

LINUX_TAR_SIZE=$(get_file_size "${TARGET_DIR}/craft-linux-amd64.tar.gz")
LINUX_BIN_SIZE=$(get_file_size "${TARGET_DIR}/craft-linux-amd64")
WIN_ZIP_SIZE=$(get_file_size "${TARGET_DIR}/craft-windows-amd64.zip")
WIN_EXE_SIZE=$(get_file_size "${TARGET_DIR}/craft-windows-amd64.exe")
DARWIN_ARM_SIZE=$(get_file_size "${TARGET_DIR}/craft-darwin-arm64.tar.gz")
DARWIN_AMD_SIZE=$(get_file_size "${TARGET_DIR}/craft-darwin-amd64.tar.gz")

LTS_LINUX_TAR_SIZE=$(get_file_size "${LTS_DIR}/craft-linux-amd64.tar.gz")
LTS_LINUX_BIN_SIZE=$(get_file_size "${LTS_DIR}/craft-linux-amd64")
LTS_WIN_ZIP_SIZE=$(get_file_size "${LTS_DIR}/craft-windows-amd64.zip")
LTS_WIN_EXE_SIZE=$(get_file_size "${LTS_DIR}/craft-windows-amd64.exe")
LTS_DARWIN_ARM_SIZE=$(get_file_size "${LTS_DIR}/craft-darwin-arm64.tar.gz")
LTS_DARWIN_AMD_SIZE=$(get_file_size "${LTS_DIR}/craft-darwin-amd64.tar.gz")

UPDATED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Step 4: Generate versions.json manifest
echo -e "${BLUE}==> [3/5] Generating versions.json manifest...${NC}"

cat <<EOF > "${LATEST_DIR}/versions.json"
{
  "latest": "${VERSION}",
  "lts": "${LTS_VERSION}",
  "updated_at": "${UPDATED_AT}",
  "versions": [
    {
      "version": "${VERSION}",
      "channel": "latest",
      "label": "v${VERSION} (Latest)",
      "release_date": "$(date -u +%Y-%m-%d)",
      "notes": "Remote TUI streaming, bidirectional version checking, and GitHub Releases CDN distribution.",
      "assets": {
        "linux_tar": {
          "name": "craft-linux-amd64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-linux-amd64.tar.gz",
          "size": "${LINUX_TAR_SIZE}"
        },
        "linux_bin": {
          "name": "craft-linux-amd64",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-linux-amd64",
          "size": "${LINUX_BIN_SIZE}"
        },
        "windows_zip": {
          "name": "craft-windows-amd64.zip",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-windows-amd64.zip",
          "size": "${WIN_ZIP_SIZE}"
        },
        "windows_exe": {
          "name": "craft-windows-amd64.exe",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-windows-amd64.exe",
          "size": "${WIN_EXE_SIZE}"
        },
        "darwin_arm64_tar": {
          "name": "craft-darwin-arm64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-darwin-arm64.tar.gz",
          "size": "${DARWIN_ARM_SIZE}"
        },
        "darwin_amd64_tar": {
          "name": "craft-darwin-amd64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${VERSION}/craft-darwin-amd64.tar.gz",
          "size": "${DARWIN_AMD_SIZE}"
        }
      }
    },
    {
      "version": "${LTS_VERSION}",
      "channel": "lts",
      "label": "v${LTS_VERSION} (LTS)",
      "release_date": "2026-09-10",
      "notes": "Long Term Support release with backup engines, systemd daemon, and plugins manager.",
      "assets": {
        "linux_tar": {
          "name": "craft-linux-amd64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-linux-amd64.tar.gz",
          "size": "${LTS_LINUX_TAR_SIZE}"
        },
        "linux_bin": {
          "name": "craft-linux-amd64",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-linux-amd64",
          "size": "${LTS_LINUX_BIN_SIZE}"
        },
        "windows_zip": {
          "name": "craft-windows-amd64.zip",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-windows-amd64.zip",
          "size": "${LTS_WIN_ZIP_SIZE}"
        },
        "windows_exe": {
          "name": "craft-windows-amd64.exe",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-windows-amd64.exe",
          "size": "${LTS_WIN_EXE_SIZE}"
        },
        "darwin_arm64_tar": {
          "name": "craft-darwin-arm64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-darwin-arm64.tar.gz",
          "size": "${LTS_DARWIN_ARM_SIZE}"
        },
        "darwin_amd64_tar": {
          "name": "craft-darwin-amd64.tar.gz",
          "url": "https://github.com/OguzhanUmutlu/craft/releases/download/v${LTS_VERSION}/craft-darwin-amd64.tar.gz",
          "size": "${LTS_DARWIN_AMD_SIZE}"
        }
      }
    }
  ]
}
EOF

# Copy manifest across targets
cp -f "${LATEST_DIR}/versions.json" "${TARGET_DIR}/versions.json"
cp -f "${LATEST_DIR}/versions.json" "${LTS_DIR}/versions.json"
mkdir -p "${DOCS_PUBLIC_DIR}"
cp -f "${LATEST_DIR}/versions.json" "${DOCS_PUBLIC_DIR}/versions.json"
echo -e "${GREEN}[OK] Synchronized versions.json across releases and docs/public${NC}"

# Step 5: Upload to GitHub Releases
echo -e "${BLUE}==> [4/5] Preparing GitHub Releases upload...${NC}"

COLLECTED_ASSETS=()
for file in "${TARGET_DIR}"/*; do
    if [ -f "$file" ]; then
        COLLECTED_ASSETS+=("$file")
    fi
done

if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}[DRY RUN] Would publish release v${VERSION} to GitHub with ${#COLLECTED_ASSETS[@]} assets:${NC}"
    for asset in "${COLLECTED_ASSETS[@]}"; do
        echo "  - $(basename "$asset")"
    done
    echo -e "${YELLOW}[DRY RUN] Local releases and versions.json updated successfully.${NC}"
    exit 0
fi

# Ensure gh is available
if ! command -v gh &> /dev/null; then
    echo -e "${RED}Error: 'gh' CLI tool is not installed or not in PATH.${NC}"
    echo "Install gh or run with --dry-run for local packaging."
    exit 1
fi

echo -e "${BLUE}==> [5/5] Publishing v${VERSION} to GitHub Releases...${NC}"

DRAFT_FLAG=""
if [ "$DRAFT" = true ]; then
    DRAFT_FLAG="--draft"
fi

# Check if release exists
if gh release view "v${VERSION}" &>/dev/null; then
    echo -e "${YELLOW}Release v${VERSION} already exists. Uploading/updating assets...${NC}"
    gh release upload "v${VERSION}" "${COLLECTED_ASSETS[@]}" --clobber
    echo -e "${GREEN}[OK] Uploaded assets to existing release v${VERSION}${NC}"
else
    echo -e "${BLUE}Creating new GitHub release v${VERSION}...${NC}"
    gh release create "v${VERSION}" "${COLLECTED_ASSETS[@]}" \
        --title "v${VERSION}" \
        --notes "Craft release v${VERSION}: includes remote streaming, version negotiation, and cross-platform pre-compiled binaries." \
        $DRAFT_FLAG
    echo -e "${GREEN}[OK] Successfully published GitHub release v${VERSION}${NC}"
fi

# Also ensure LTS release exists on GitHub so LTS links are live
if ! gh release view "v${LTS_VERSION}" &>/dev/null; then
    echo -e "${BLUE}LTS release v${LTS_VERSION} does not exist on GitHub. Creating it now...${NC}"
    LTS_ASSETS=()
    for file in "${LTS_DIR}"/*; do
        if [ -f "$file" ]; then
            LTS_ASSETS+=("$file")
        fi
    done
    if [ ${#LTS_ASSETS[@]} -gt 0 ]; then
        gh release create "v${LTS_VERSION}" "${LTS_ASSETS[@]}" \
            --title "v${LTS_VERSION} (LTS)" \
            --notes "Craft Long Term Support release v${LTS_VERSION}." \
            $DRAFT_FLAG
        echo -e "${GREEN}[OK] Successfully published GitHub release v${LTS_VERSION}${NC}"
    fi
fi

# Ensure latest pointer points to active version if not LTS
if [ "$VERSION" != "$LTS_VERSION" ]; then
    gh release edit "v${VERSION}" --latest &>/dev/null || true
fi

echo ""
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e "${GREEN}${BOLD}[OK] Release v${VERSION} successfully packaged and published!${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
echo -e " Pinned URL: ${CYAN}https://github.com/OguzhanUmutlu/craft/releases/tag/v${VERSION}${NC}"
echo -e " Latest URL: ${CYAN}https://github.com/OguzhanUmutlu/craft/releases/latest${NC}"
echo -e " Local Dir:  ${YELLOW}${TARGET_DIR}${NC}"
echo -e "${GREEN}${BOLD}==================================================================${NC}"
