#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "🔍 Executing test queries..."
echo "----------------------------"

run_query() {
    local name=$1
    local data=$2

    echo "📊 ${name}"
    curl -s -X POST -H "Content-Type: application/json" \
      -d "${data}" \
      "${PRISM_URL}/criteria"
    echo "" # Add newline after response
}

# Query for all sites
run_query "Query 1: All sites" '{"sites":[]}'

# Query for specific site
run_query "Query 2: Specific site" '{"sites":["'${BRIDGEHEAD_SERVICE}'"]}'

# Query with no sites
run_query "Query 3: No sites" '{"sites":[]}'

echo "✅ Queries executed"
