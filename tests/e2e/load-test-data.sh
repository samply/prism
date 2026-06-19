#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "📦 Loading test data..."
echo "------------------------"

# Function to wait for FHIR server
wait_for_fhir_server() {
    local port=$1
    local max_attempts=30
    local attempt=0

    echo "⏳ Waiting for FHIR server to be ready on port ${port}..."
    while ! curl -s -o /dev/null --max-time 1 "http://localhost:${port}/fhir"; do
        attempt=$((attempt+1))
        if [ $attempt -ge $max_attempts ]; then
            echo "❌ FHIR server not ready within ${max_attempts} seconds"
            echo "Blaze logs:"
            docker logs ${BLAZE_SERVICE}
            exit 1
        fi
        sleep 1
    done
    echo "✅ FHIR server ready after ${attempt} seconds"
}

load_fhir_data() {
    local resource=$1
    local file=$2
    local url="http://localhost:${BLAZE_PORT}/fhir"

    echo "Loading ${resource} from ${file}..."
    response=$(curl -s -w "%{http_code}" -o /dev/null -X POST -H "Content-Type: application/fhir+json" \
      -d "@${TEST_DATA_DIR}/${file}" \
      "${url}")

    if [[ $response -ge 200 && $response -lt 300 ]]; then
        echo "  ✅ Success (HTTP $response)"
    else
        echo "  ❌ Failed to load ${resource} (HTTP $response)"
        exit 1
    fi
}

# Wait for FHIR server before loading data
wait_for_fhir_server ${BLAZE_PORT}

load_fhir_data "patients" "patients.json"
load_fhir_data "specimens" "specimens.json"
load_fhir_data "conditions" "conditions.json"

echo "✅ Test data loaded"
