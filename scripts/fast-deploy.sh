#!/bin/bash

# Fast deployment script for development iteration
# Builds locally and deploys to enclave quickly

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

# Configuration
ENCLAVE_CID="${ENCLAVE_CID:-16}"
DEBUG_MODE="${DEBUG_MODE:-true}"

echo -e "${BLUE}⚡ FAST ENCLAVE DEPLOYMENT${NC}"
echo ""

# Step 1: Build the binary locally (uses local cargo cache - FAST!)
echo -e "${YELLOW}[1/4]${NC} Building secure-enclave binary locally..."
cargo build --release --bin secure-enclave

# Copy binary to avoid .dockerignore issues
cp target/release/secure-enclave secure-enclave/secure-enclave-binary

# Step 2: Build minimal Docker image (just copies binary - FAST!)
echo -e "${YELLOW}[2/4]${NC} Building Docker image (minimal)..."
sudo docker build -t secure-enclave:fast -f secure-enclave/Dockerfile.fast .

# Step 3: Build EIF
echo -e "${YELLOW}[3/4]${NC} Building EIF..."
nitro-cli build-enclave \
    --docker-uri secure-enclave:fast \
    --output-file secure-enclave-fast.eif > /tmp/eif-build.log 2>&1

# Step 4: Terminate any existing enclaves and run new one
echo -e "${YELLOW}[4/4]${NC} Deploying enclave..."
nitro-cli terminate-enclave --all 2>/dev/null || true
sleep 1

if [ "$DEBUG_MODE" = "true" ]; then
    nitro-cli run-enclave \
        --cpu-count 2 \
        --memory 2048 \
        --enclave-cid $ENCLAVE_CID \
        --eif-path secure-enclave-fast.eif \
        --debug-mode
else
    nitro-cli run-enclave \
        --cpu-count 2 \
        --memory 2048 \
        --enclave-cid $ENCLAVE_CID \
        --eif-path secure-enclave-fast.eif
fi

# Show status
ENCLAVE_ID=$(nitro-cli describe-enclaves | jq -r '.[0].EnclaveID')
echo ""
echo -e "${GREEN}✅ Enclave deployed!${NC}"
echo -e "   ID: ${YELLOW}$ENCLAVE_ID${NC}"
echo -e "   Console: ${YELLOW}nitro-cli console --enclave-id $ENCLAVE_ID${NC}"
echo ""
