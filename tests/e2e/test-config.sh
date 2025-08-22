#!/bin/bash

# Get script directory safely
THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Project configuration
export PROJECT_NAME="prism"
export TEST_DIR="${THIS_DIR}"
export TEST_DATA_DIR="${TEST_DIR}/test-data"
export TEST_RESULTS_DIR="${TEST_DIR}/test-results"
export TEST_OUTPUT_FILE="${TEST_RESULTS_DIR}/test-output.log"

# Service versions
export BEAM_VERSION="${BEAM_VERSION:-latest}"
export BLAZE_VERSION="${BLAZE_VERSION:-latest}"
export BRIDGEHEAD_VERSION="${BRIDGEHEAD_VERSION:-latest}"
export PRISM_VERSION="${PRISM_VERSION:-latest}"

# Port configuration
export BROKER_PORT=${BROKER_PORT:-8184}
export BEAM_PROXY_PORT=${BEAM_PROXY_PORT:-8182}
export BLAZE_PORT=${BLAZE_PORT:-8183}
export BRIDGEHEAD_PORT=${BRIDGEHEAD_PORT:-8181}
export PRISM_PORT=${PRISM_PORT:-8180}

# Network configuration
export DOCKER_NETWORK="${PROJECT_NAME}-net"
export BROKER_ID="broker"

# Service names
export BROKER_SERVICE="broker"
export BEAM_PROXY_SERVICE="beam-proxy"
export BLAZE_SERVICE="blaze"
export BRIDGEHEAD_SERVICE="bridgehead"
export PRISM_SERVICE="prism"

# Application configuration
export BROKER_URL="http://${BROKER_SERVICE}:8080"
export BEAM_PROXY_URL="http://${BEAM_PROXY_SERVICE}:8082"
export FHIR_BASE_URL="http://${BLAZE_SERVICE}:8080/fhir"
export PRISM_URL="http://localhost:${PRISM_PORT}"

# Security configuration
export API_KEY="${API_KEY:-test-key}"

# Create required directories
mkdir -p "${TEST_RESULTS_DIR}"

# Generate PKI secret file
export PKI_SECRET_FILE="${TEST_DIR}/pki.secret"
echo -n "${API_KEY}" > "${PKI_SECRET_FILE}"
