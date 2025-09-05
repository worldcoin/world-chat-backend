#!/bin/bash

# Ultra-fast development cycle for enclave iteration
# Watch for changes and auto-deploy

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Function to build and deploy
deploy_enclave() {
    echo -e "${CYAN}═══════════════════════════════════════${NC}"
    echo -e "${CYAN}  🚀 Building and deploying...${NC}"
    echo -e "${CYAN}═══════════════════════════════════════${NC}"
    
    START_TIME=$(date +%s)
    
    # Build binary
    echo -e "${YELLOW}[1/5]${NC} Compiling binary..."
    if cargo build --release --bin secure-enclave 2>&1 | tail -5; then
        echo -e "${GREEN}✓${NC} Binary compiled"
    else
        echo -e "${RED}✗${NC} Compilation failed"
        return 1
    fi
    
    # Build Docker image
    echo -e "${YELLOW}[2/5]${NC} Building Docker image..."
    if sudo docker build -t secure-enclave:fast -f secure-enclave/Dockerfile.fast . > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} Docker image built"
    else
        echo -e "${RED}✗${NC} Docker build failed"
        return 1
    fi
    
    # Build EIF
    echo -e "${YELLOW}[3/5]${NC} Building EIF..."
    if nitro-cli build-enclave --docker-uri secure-enclave:fast --output-file secure-enclave.eif > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} EIF built"
    else
        echo -e "${RED}✗${NC} EIF build failed"
        return 1
    fi
    
    # Terminate old enclave
    echo -e "${YELLOW}[4/5]${NC} Terminating old enclave..."
    nitro-cli terminate-enclave --all > /dev/null 2>&1 || true
    
    # Start new enclave
    echo -e "${YELLOW}[5/5]${NC} Starting enclave..."
    if nitro-cli run-enclave \
        --cpu-count 2 \
        --memory 2048 \
        --enclave-cid 16 \
        --eif-path secure-enclave.eif \
        --debug-mode > /tmp/enclave-run.log 2>&1; then
        
        ENCLAVE_ID=$(nitro-cli describe-enclaves | jq -r '.[0].EnclaveID')
        END_TIME=$(date +%s)
        DURATION=$((END_TIME - START_TIME))
        
        echo ""
        echo -e "${GREEN}═══════════════════════════════════════${NC}"
        echo -e "${GREEN}  ✅ DEPLOYED IN ${DURATION} SECONDS${NC}"
        echo -e "${GREEN}═══════════════════════════════════════${NC}"
        echo -e "  Enclave ID: ${YELLOW}$ENCLAVE_ID${NC}"
        echo -e "  Console: ${YELLOW}nitro-cli console --enclave-id $ENCLAVE_ID${NC}"
        echo ""
    else
        echo -e "${RED}✗${NC} Failed to start enclave"
        cat /tmp/enclave-run.log
        return 1
    fi
}

# Main execution
if [ "$1" == "watch" ]; then
    echo -e "${CYAN}👁️  Watching for changes in secure-enclave/src/${NC}"
    echo -e "${CYAN}Press Ctrl+C to stop${NC}"
    echo ""
    
    # Initial deployment
    deploy_enclave
    
    # Watch for changes
    while inotifywait -r -e modify secure-enclave/src/ 2>/dev/null; do
        echo -e "${YELLOW}📝 Changes detected!${NC}"
        deploy_enclave
    done
else
    # Single deployment
    deploy_enclave
fi
