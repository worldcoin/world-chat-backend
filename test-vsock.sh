#!/bin/bash

# Test script for vsock-enabled enclave

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}           TESTING VSOCK ENCLAVE COMMUNICATION              ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Check enclave is running
echo -e "${YELLOW}[1/4]${NC} Checking enclave status..."
ENCLAVE_STATUS=$(sudo nitro-cli describe-enclaves | jq -r '.[0].State' 2>/dev/null || echo "NONE")
if [ "$ENCLAVE_STATUS" = "RUNNING" ]; then
    echo -e "${GREEN}  ✓ Enclave is running${NC}"
else
    echo -e "${RED}  ✗ Enclave not running. Run: make deploy${NC}"
    exit 1
fi

# Check vsock-proxy
echo -e "${YELLOW}[2/4]${NC} Checking vsock-proxy..."
if ps aux | grep -q "[v]sock-proxy.*8080.*braze"; then
    echo -e "${GREEN}  ✓ vsock-proxy is running on port 8080${NC}"
else
    echo -e "${YELLOW}  Starting vsock-proxy...${NC}"
    sudo pkill -f vsock-proxy 2>/dev/null || true
    sudo vsock-proxy 8080 rest.iad-05.braze.com 443 &
    sleep 2
    echo -e "${GREEN}  ✓ vsock-proxy started${NC}"
fi

# Check if BRAZE_API_KEY is set
echo -e "${YELLOW}[3/4]${NC} Checking Braze API key..."
if [ -z "$BRAZE_API_KEY" ]; then
    echo -e "${RED}  ✗ BRAZE_API_KEY not set${NC}"
    echo "  Please run: export BRAZE_API_KEY='your-api-key'"
    exit 1
else
    echo -e "${GREEN}  ✓ BRAZE_API_KEY is set${NC}"
fi

# Monitor enclave console in background
echo -e "${YELLOW}[4/4]${NC} Starting enclave console monitor..."
ENCLAVE_ID=$(sudo nitro-cli describe-enclaves | jq -r '.[0].EnclaveID')
echo "  Monitoring enclave: $ENCLAVE_ID"
echo ""

# Start console in background to a file
sudo nitro-cli console --enclave-id $ENCLAVE_ID > /tmp/enclave-console.log 2>&1 &
CONSOLE_PID=$!

# Run the enclave worker
echo -e "${BLUE}Starting enclave worker...${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

export BRAZE_API_ENDPOINT="${BRAZE_API_ENDPOINT:-https://rest.iad-05.braze.com}"
export ENCLAVE_CID=16
export ENCLAVE_PORT=5000
export PROXY_HOST="127.0.0.1"
export PROXY_PORT=8080
export RUST_LOG="${RUST_LOG:-debug}"

# Run for a short time to test
timeout 10 cargo run --bin enclave-worker 2>&1 | tee /tmp/worker.log || true

# Kill console monitor
kill $CONSOLE_PID 2>/dev/null || true

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                    TEST RESULTS                             ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Check for success in logs
if grep -q "Notification sent successfully" /tmp/worker.log; then
    echo -e "${GREEN}✅ SUCCESS: Notification was sent through vsock!${NC}"
else
    echo -e "${RED}❌ FAILED: Notification was not sent${NC}"
    echo ""
    echo "Worker errors:"
    grep -i error /tmp/worker.log | tail -5
    echo ""
    echo "Enclave errors:"
    grep -i error /tmp/enclave-console.log | tail -5
fi

echo ""
echo "Full logs available at:"
echo "  Worker: /tmp/worker.log"
echo "  Enclave: /tmp/enclave-console.log"
