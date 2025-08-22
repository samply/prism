#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

if [ $# -ne 1 ]; then
  echo "❌ Usage: $0 <test-output-file>"
  exit 1
fi

OUTPUT_FILE=$1

echo "🔬 Validating test results..."
echo "-----------------------------"

# Check HTTP response codes
if ! grep -q "HTTP/1.1 200 OK" "${OUTPUT_FILE}"; then
  echo "❌ Missing successful HTTP response"
  exit 1
fi

# Define required stratifiers
REQUIRED_STRATIFIERS=("Age" "Gender" "Custodian" "diagnosis" "sample_kind")

# Validate stratifier presence
for strat in "${REQUIRED_STRATIFIERS[@]}"; do
  if ! grep -q "\"${strat}\"" "${OUTPUT_FILE}"; then
    echo "❌ Missing '${strat}' stratifier"
    exit 1
  fi
done

# Validate sample counts
if ! grep -q '"blood-plasma"' "${OUTPUT_FILE}" || \
   ! grep -q '"blood-serum"' "${OUTPUT_FILE}"; then
  echo "❌ Missing sample kind counts"
  exit 1
fi

echo "✅ All validations passed"
