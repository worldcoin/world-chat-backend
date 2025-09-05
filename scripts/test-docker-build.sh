#!/bin/bash

# Quick test script to verify Docker build works without hanging

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Testing Docker build for secure-enclave...${NC}"
echo ""

# Check Docker access
if ! docker ps &> /dev/null; then
    if ! sudo docker ps &> /dev/null; then
        echo -e "${RED}Cannot access Docker${NC}"
        exit 1
    else
        DOCKER_CMD="sudo docker"
    fi
else
    DOCKER_CMD="docker"
fi

echo -e "${YELLOW}[1/3]${NC} Starting Docker build..."
echo "      Output is being saved to /tmp/docker-build-test.log"

# Run the build with timeout to prevent hanging
timeout 300 $DOCKER_CMD build \
    --progress=plain \
    -t secure-enclave:test \
    -f secure-enclave/Dockerfile \
    . > /tmp/docker-build-test.log 2>&1 &

BUILD_PID=$!

# Show progress dots while building
echo -n "      Building"
while kill -0 $BUILD_PID 2>/dev/null; do
    echo -n "."
    sleep 2
done
echo ""

wait $BUILD_PID
BUILD_EXIT=$?

if [ $BUILD_EXIT -eq 0 ]; then
    echo -e "${GREEN}[✓]${NC} Docker build completed successfully!"
    echo ""
    echo "Image details:"
    $DOCKER_CMD images secure-enclave:test --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}"
elif [ $BUILD_EXIT -eq 124 ]; then
    echo -e "${RED}[✗]${NC} Docker build timed out after 5 minutes"
    echo "Last 20 lines of output:"
    tail -20 /tmp/docker-build-test.log
else
    echo -e "${RED}[✗]${NC} Docker build failed with exit code: $BUILD_EXIT"
    echo "Last 20 lines of output:"
    tail -20 /tmp/docker-build-test.log
fi

echo ""
echo -e "${BLUE}[2/3]${NC} Checking build logs..."
if [ -f /tmp/docker-build-test.log ]; then
    LOG_SIZE=$(wc -l < /tmp/docker-build-test.log)
    echo "      Log file has $LOG_SIZE lines"
    
    # Check for common issues
    if grep -q "cargo-chef" /tmp/docker-build-test.log; then
        echo -e "      ${GREEN}✓${NC} cargo-chef found in build"
    fi
    
    if grep -q "error" /tmp/docker-build-test.log; then
        echo -e "      ${YELLOW}⚠${NC} Errors found in build log:"
        grep -i "error" /tmp/docker-build-test.log | head -5
    fi
fi

echo ""
echo -e "${BLUE}[3/3]${NC} Cleanup..."
echo "      Removing test image..."
$DOCKER_CMD rmi secure-enclave:test 2>/dev/null || true

echo ""
echo -e "${GREEN}Test complete!${NC}"
echo ""
echo "To view full build log: cat /tmp/docker-build-test.log"
echo "To run the full enclave deployment: ./scripts/run-secure-enclave.sh"
