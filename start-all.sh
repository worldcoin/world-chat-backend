#!/bin/bash

# Complete startup script for enclave + worker + proxy

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}           STARTING ENCLAVE NOTIFICATION SYSTEM              ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Check if BRAZE_API_KEY is set
if [ -z "$BRAZE_API_KEY" ]; then
    echo -e "${RED}[ERROR]${NC} BRAZE_API_KEY is not set!"
    echo -e "${YELLOW}Please set it first:${NC}"
    echo "  export BRAZE_API_KEY='your-actual-api-key-here'"
    exit 1
fi

# Step 1: Check if enclave is running
echo -e "${YELLOW}[1/3]${NC} Checking enclave status..."
ENCLAVE_STATUS=$(sudo nitro-cli describe-enclaves 2>/dev/null || echo "[]")
if [ "$ENCLAVE_STATUS" = "[]" ]; then
    echo -e "${YELLOW}  No enclave running. Deploying...${NC}"
    make deploy
else
    echo -e "${GREEN}  ✓ Enclave already running${NC}"
    sudo nitro-cli describe-enclaves | jq -r '.[0] | "    ID: \(.EnclaveID)\n    CID: \(.EnclaveCID)"'
fi

# Step 2: Start vsock-proxy
echo ""
echo -e "${YELLOW}[2/3]${NC} Starting vsock-proxy..."

# Kill any existing vsock-proxy
sudo pkill -f vsock-proxy 2>/dev/null || true

# Start vsock-proxy in background
sudo vsock-proxy 8080 rest.iad-01.braze.com 443 &
PROXY_PID=$!
echo -e "${GREEN}  ✓ vsock-proxy started (PID: $PROXY_PID)${NC}"
echo "    Forwarding: localhost:8080 → rest.iad-01.braze.com:443"

# Step 3: Run enclave worker
echo ""
echo -e "${YELLOW}[3/3]${NC} Starting enclave worker..."
echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}                    SYSTEM READY!                            ${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "${BLUE}Monitor commands:${NC}"
echo "  • View enclave console: ${YELLOW}make console${NC}"
echo "  • Check status: ${YELLOW}make status${NC}"
echo "  • Stop everything: ${YELLOW}make kill && sudo pkill vsock-proxy${NC}"
echo ""
echo -e "${BLUE}Starting worker now...${NC}"
echo ""

# Export required environment variables
export BRAZE_API_ENDPOINT="${BRAZE_API_ENDPOINT:-https://rest.iad-01.braze.com}"
export ENCLAVE_CID=16
export ENCLAVE_PORT=5000
export PROXY_HOST="127.0.0.1"
export PROXY_PORT=8080
export RUST_LOG="${RUST_LOG:-debug}"

# Run the worker (this will run in foreground)
exec cargo run --bin enclave-worker
