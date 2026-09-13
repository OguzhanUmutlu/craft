#!/usr/bin/env bash
set -e

# ==============================================================================
# Craft Executables & Binaries Release Tool
# Delegates to scripts/release.sh for GitHub Releases & local packaging
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${SCRIPT_DIR}/release.sh" "$@"
