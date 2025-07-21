#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/test-config.sh"

echo "🚀 Starting services..."
echo "-----------------------"

# Function to wait for service
wait_for_service() {
    local port=$1
    local service_name=$2
    local max_attempts=30
    local attempt=0

    echo "⏳ Waiting for ${service_name} to start on port ${port}..."
    while ! curl -s -o /dev/null --max-time 1 "http://localhost:${port}"; do
        attempt=$((attempt+1))
        if [ $attempt -ge $max_attempts ]; then
            echo "❌ ${service_name} did not start within ${max_attempts} seconds"
            echo "${service_name} logs:"
            docker logs ${service_name}
            exit 1
        fi
        sleep 1
    done
    echo "✅ ${service_name} started after ${attempt} seconds"
}

# Start Beam Proxy
docker run -d --name ${BEAM_PROXY_SERVICE} \
  --network ${DOCKER_NETWORK} \
  -p ${BEAM_PROXY_PORT}:8082 \
  -e BROKER_URL="http://${BEAM_PROXY_SERVICE}:8082" \
  -e PROXY_ID="${BEAM_PROXY_SERVICE}.${BROKER_ID}" \
  samply/beam-proxy:${BEAM_VERSION}
wait_for_service ${BEAM_PROXY_PORT} ${BEAM_PROXY_SERVICE}

# Start Blaze FHIR Server
docker run -d --name ${BLAZE_SERVICE} \
  --network ${DOCKER_NETWORK} \
  -p ${BLAZE_PORT}:8080 \
  -v "${TEST_DATA_DIR}:/data" \
  samply/blaze:${BLAZE_VERSION} --port 8080
wait_for_service ${BLAZE_PORT} ${BLAZE_SERVICE}

# Start Bridgehead
docker run -d --name ${BRIDGEHEAD_SERVICE} \
  --network ${DOCKER_NETWORK} \
  -p ${BRIDGEHEAD_PORT}:8081 \
  -e BEAM_PROXY_URL="http://${BEAM_PROXY_SERVICE}:8082" \
  -e BEAM_PROXY_APP_ID="${BRIDGEHEAD_SERVICE}.${BEAM_PROXY_SERVICE}.${BROKER_ID}" \
  -e BEAM_PROXY_API_KEY="${API_KEY}" \
  -e FHIR_BASE_URL="http://${BLAZE_SERVICE}:8080/fhir" \
  samply/bridgehead-echo:${BRIDGEHEAD_VERSION}
wait_for_service ${BRIDGEHEAD_PORT} ${BRIDGEHEAD_SERVICE}

# Start Prism
docker run -d --name ${PRISM_SERVICE} \
  --network ${DOCKER_NETWORK} \
  -p ${PRISM_PORT}:8080 \
  -e BEAM_PROXY_URL="http://${BEAM_PROXY_SERVICE}:8082" \
  -e BEAM_APP_ID_LONG="${PRISM_SERVICE}.${BEAM_PROXY_SERVICE}.${BROKER_ID}" \
  -e API_KEY="${API_KEY}" \
  -e SITES="${BRIDGEHEAD_SERVICE}" \
  -e CORS_ORIGIN="any" \
  -e PROJECT="${PROJECT_NAME}" \
  -e TARGET_APP="focus" \
  samply/prism:${PRISM_VERSION}
wait_for_service ${PRISM_PORT} ${PRISM_SERVICE}

echo "✅ All services started"