#!/usr/bin/env bash
# ==============================================================================
# Craft Version Catalog Publisher
# Scrapes all server softwares, generates catalog assets, and publishes to GitHub Release.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/output"

echo "[1/3] Running version catalog scraper..."
python3 "${SCRIPT_DIR}/scraper.py"

if [ ! -f "${OUTPUT_DIR}/versions.zst" ] || [ ! -f "${OUTPUT_DIR}/catalog.json" ]; then
    echo "ERROR: Catalog output files not found in ${OUTPUT_DIR}" >&2
    exit 1
fi

echo "[2/3] Checking GitHub release 'catalog'..."
if gh release view catalog >/dev/null 2>&1; then
    echo "Release 'catalog' exists. Uploading updated assets..."
    gh release upload catalog "${OUTPUT_DIR}"/* --clobber
else
    echo "Creating new release 'catalog'..."
    gh release create catalog "${OUTPUT_DIR}"/* \
        --title "Craft Version Catalog" \
        --notes "Centralized, high-performance version catalog for Craft server manager. Includes 4,000+ historical and modern server versions across 20+ game and software platforms."
fi

echo "[3/3] Verifying published release assets..."
gh release view catalog --json assets --jq '.assets[].name' | head -n 10
echo "Catalog published successfully to GitHub Releases!"
