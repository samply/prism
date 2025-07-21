#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "🧹 Cleaning up test environment..."
echo "---------------------------------"

# Stop and remove containers
docker stop ${PRISM_SERVICE} ${BRIDGEHEAD_SERVICE} ${BLAZE_SERVICE} ${BEAM_PROXY_SERVICE} 2>/dev/null || true
docker rm ${PRISM_SERVICE} ${BRIDGEHEAD_SERVICE} ${BLAZE_SERVICE} ${BEAM_PROXY_SERVICE} 2>/dev/null || true

# Remove any containers that might have different naming
docker stop beam-proxy bridgehead blaze prism 2>/dev/null || true
docker rm beam-proxy bridgehead blaze prism 2>/dev/null || true

# Remove network
docker network rm ${DOCKER_NETWORK} 2>/dev/null || true

echo "✅ Cleanup complete"