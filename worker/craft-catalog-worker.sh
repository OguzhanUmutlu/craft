#!/usr/bin/env bash
# ==============================================================================
# Craft Server Version Catalog Periodic Worker
# Compiles upstream server software versions, zstd-compresses into catalog,
# and uploads to Cloudflare Workers endpoint.
#
# Usage (run manually or via crontab):
#   CATALOG_UPLOAD_SECRET="your-secret" ./craft-catalog-worker.sh
#
# Recommended cron setup (every 6 hours):
#   0 */6 * * * CATALOG_UPLOAD_SECRET="your-secret" /path/to/craft-catalog-worker.sh >> /var/log/craft-catalog.log 2>&1
# ==============================================================================

set -euo pipefail

# Configuration
ENDPOINT_URL="${CRAFT_UPLOAD_URL:-https://craft-versions-worker.someoneontheinternet.workers.dev/api/versions.zst}"
TEMP_OUTPUT="/tmp/craft-versions-catalog.zst"
LOG_PREFIX="[$(date '+%Y-%m-%d %H:%M:%S')] [CraftCatalogWorker]"

echo "${LOG_PREFIX} Starting version catalog generation..."

if [ -z "${CATALOG_UPLOAD_SECRET:-}" ]; then
    echo "${LOG_PREFIX} ERROR: CATALOG_UPLOAD_SECRET environment variable is not set." >&2
    exit 1
fi

# Locate craft binary (check PATH or default install paths)
CRAFT_BIN="$(command -v craft || echo "/usr/local/bin/craft")"
if [ ! -x "${CRAFT_BIN}" ]; then
    if [ -x "$HOME/.cargo/bin/craft" ]; then
        CRAFT_BIN="$HOME/.cargo/bin/craft"
    else
        echo "${LOG_PREFIX} ERROR: 'craft' binary not found or not executable at ${CRAFT_BIN}" >&2
        exit 1
    fi
fi

# Compile catalog
echo "${LOG_PREFIX} Running '${CRAFT_BIN} catalog build --output ${TEMP_OUTPUT}'..."
"${CRAFT_BIN}" catalog build --output "${TEMP_OUTPUT}"

if [ ! -f "${TEMP_OUTPUT}" ]; then
    echo "${LOG_PREFIX} ERROR: Expected output file was not created: ${TEMP_OUTPUT}" >&2
    exit 1
fi

FILE_SIZE="$(stat -c%s "${TEMP_OUTPUT}" 2>/dev/null || stat -f%z "${TEMP_OUTPUT}" 2>/dev/null || echo "0")"
FILE_SIZE_KB="$(awk "BEGIN {printf \"%.2f\", ${FILE_SIZE}/1024}")"

echo "${LOG_PREFIX} Catalog generated successfully: ${FILE_SIZE_KB} KB (${FILE_SIZE} bytes)."
echo "${LOG_PREFIX} Uploading to ${ENDPOINT_URL}..."

# Upload to Cloudflare Worker
HTTP_RESPONSE="$(curl -s -w "\n%{http_code}" -X PUT "${ENDPOINT_URL}" \
    -H "Authorization: Bearer ${CATALOG_UPLOAD_SECRET}" \
    -H "Content-Type: application/zstd" \
    --data-binary "@${TEMP_OUTPUT}")"

HTTP_BODY="$(echo "${HTTP_RESPONSE}" | sed '$d')"
HTTP_STATUS="$(echo "${HTTP_RESPONSE}" | tail -n 1)"

if [ "${HTTP_STATUS}" -ge 200 ] && [ "${HTTP_STATUS}" -lt 300 ]; then
    echo "${LOG_PREFIX} Upload SUCCESS (HTTP ${HTTP_STATUS}). Response: ${HTTP_BODY}"
    rm -f "${TEMP_OUTPUT}"
    echo "${LOG_PREFIX} Completed successfully."
    exit 0
else
    echo "${LOG_PREFIX} ERROR: Upload failed with HTTP ${HTTP_STATUS}." >&2
    echo "${LOG_PREFIX} Server response: ${HTTP_BODY}" >&2
    exit 1
fi
