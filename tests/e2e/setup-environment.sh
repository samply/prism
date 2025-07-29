#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "🔧 Setting up test environment..."
echo "--------------------------------"

# Remove existing network if it exists
docker network rm ${DOCKER_NETWORK} 2>/dev/null || true

# Create test network
docker network create "${DOCKER_NETWORK}" || true

echo "✅ Environment setup complete"