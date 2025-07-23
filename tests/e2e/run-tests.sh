#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "🧹 Initial cleanup..."
"${SCRIPT_DIR}/cleanup.sh"

echo "🚀 Starting end-to-end tests for ${PROJECT_NAME}"
echo "----------------------------------------------"

# Exit handler for better error reporting
exit_handler() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo "❌ Tests failed with exit code $exit_code"
    fi
    echo "Running final cleanup..."
    "${SCRIPT_DIR}/cleanup.sh"
    exit $exit_code
}
trap exit_handler EXIT

# Execute test steps
"${SCRIPT_DIR}/setup-environment.sh"
"${SCRIPT_DIR}/start-services.sh"
"${SCRIPT_DIR}/load-test-data.sh"
"${SCRIPT_DIR}/test-queries.sh" | tee "${TEST_OUTPUT_FILE}"
"${SCRIPT_DIR}/validate-results.sh" "${TEST_OUTPUT_FILE}"

echo "✅ All tests passed successfully"