#!/bin/bash

# Script to run the enclave worker on EC2 instance
# This runs directly with cargo run for development/testing

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}         🚀 ENCLAVE WORKER STARTUP SCRIPT                   ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# ============================================================================
# ENVIRONMENT VARIABLES - CONFIGURE THESE!
# ============================================================================

# Required: Braze API Configuration
# export BRAZE_API_KEY="${BRAZE_API_KEY:-YOUR_BRAZE_API_KEY_HERE}"  # REQUIRED: Your Braze API key
# export BRAZE_API_ENDPOINT="${BRAZE_API_ENDPOINT:-https://rest.iad-01.braze.com}"  # Braze REST endpoint

# # Enclave Connection Configuration
# export ENCLAVE_CID="${ENCLAVE_CID:-16}"  # Default CID for first enclave
# export ENCLAVE_PORT="${ENCLAVE_PORT:-5000}"  # Port where secure enclave listens

# # Proxy Configuration (for enclave network access)
# export PROXY_HOST="${PROXY_HOST:-127.0.0.1}"  # vsock-proxy host
# export PROXY_PORT="${PROXY_PORT:-8080}"  # vsock-proxy port

# # Logging Configuration
# export RUST_LOG="${RUST_LOG:-debug,pontifex=trace}"  # Set to 'trace' for maximum verbosity
# export RUST_BACKTRACE="${RUST_BACKTRACE:-full}"  # Enable full backtraces on errors
export BRAZE_API_ENDPOINT=https://rest.iad-05.braze.com
export BRAZE_API_KEY=d793c128-603e-4014-b3e5-a9d3fe864f93
export ENCLAVE_CID=16
export ENCLAVE_PORT=5000
export PROXY_HOST="127.0.0.1"
export PROXY_PORT=8080
export RUST_LOG=debug

# ============================================================================
# VALIDATION
# ============================================================================

echo -e "${YELLOW}[CHECK]${NC} Validating environment variables..."

if [ "$BRAZE_API_KEY" == "YOUR_BRAZE_API_KEY_HERE" ]; then
    echo -e "${RED}[ERROR]${NC} BRAZE_API_KEY is not set! Please set it before running."
    echo "  Example: export BRAZE_API_KEY='your-actual-api-key'"
    exit 1
fi

# ============================================================================
# DISPLAY CONFIGURATION
# ============================================================================

echo -e "${GREEN}[INFO]${NC} Configuration:"
echo -e "  ${BLUE}Braze API:${NC}"
echo -e "    • API Key: ${YELLOW}[REDACTED - ${#BRAZE_API_KEY} chars]${NC}"
echo -e "    • Endpoint: ${YELLOW}$BRAZE_API_ENDPOINT${NC}"
echo ""
echo -e "  ${BLUE}Enclave Connection:${NC}"
echo -e "    • CID: ${YELLOW}$ENCLAVE_CID${NC}"
echo -e "    • Port: ${YELLOW}$ENCLAVE_PORT${NC}"
echo ""
echo -e "  ${BLUE}Proxy Configuration:${NC}"
echo -e "    • Host: ${YELLOW}$PROXY_HOST${NC}"
echo -e "    • Port: ${YELLOW}$PROXY_PORT${NC}"
echo ""
echo -e "  ${BLUE}Logging:${NC}"
echo -e "    • Log Level: ${YELLOW}$RUST_LOG${NC}"
echo -e "    • Backtrace: ${YELLOW}$RUST_BACKTRACE${NC}"
echo ""

# ============================================================================
# CHECK ENCLAVE STATUS
# ============================================================================

echo -e "${YELLOW}[CHECK]${NC} Checking if secure enclave is running..."

if command -v nitro-cli &> /dev/null; then
    ENCLAVE_STATUS=$(nitro-cli describe-enclaves 2>/dev/null || echo "[]")
    if [ "$ENCLAVE_STATUS" != "[]" ]; then
        echo -e "${GREEN}[OK]${NC} Found running enclave(s):"
        echo "$ENCLAVE_STATUS" | jq -r '.[] | "  • Enclave ID: \(.EnclaveID) | CID: \(.EnclaveCID) | State: \(.State)"'
    else
        echo -e "${YELLOW}[WARN]${NC} No running enclaves detected. Make sure to run ./run-secure-enclave.sh first!"
    fi
else
    echo -e "${YELLOW}[INFO]${NC} nitro-cli not available - assuming development mode"
fi

echo ""

# ============================================================================
# RUN ENCLAVE WORKER
# ============================================================================

echo -e "${GREEN}[START]${NC} Starting Enclave Worker..."
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Change to workspace directory
cd /home/ec2-user/repos/world-chat-backend

# Run with cargo in debug mode for rich error details
exec cargo run --bin enclave-worker

# Note: exec replaces the shell process, so the script ends here
# Use Ctrl+C to stop the worker
