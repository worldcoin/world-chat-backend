#!/bin/bash

# Script to build and run the secure enclave on EC2 Nitro instance
# This builds the EIF and runs it in the Nitro Enclave

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${CYAN}         🔐 SECURE ENCLAVE DEPLOYMENT SCRIPT                ${NC}"
echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
echo ""

# ============================================================================
# CONFIGURATION - MODIFY THESE AS NEEDED
# ============================================================================

# Enclave Resources
ENCLAVE_CPU_COUNT="${ENCLAVE_CPU_COUNT:-2}"  # Number of CPUs for enclave
ENCLAVE_MEMORY_MB="${ENCLAVE_MEMORY_MB:-2048}"  # Memory in MB for enclave
ENCLAVE_CID="${ENCLAVE_CID:-16}"  # Context ID for vsock communication

# Debug Mode (set to false for production)
DEBUG_MODE="${DEBUG_MODE:-true}"  # Enable debug mode for console access

# Docker image name
DOCKER_IMAGE_NAME="secure-enclave"
DOCKER_IMAGE_TAG="${DOCKER_IMAGE_TAG:-latest}"
EIF_FILE="secure-enclave.eif"

# ============================================================================
# FUNCTIONS
# ============================================================================

print_status() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[i]${NC} $1"
}

# ============================================================================
# PRE-FLIGHT CHECKS
# ============================================================================

echo -e "${YELLOW}[CHECK]${NC} Running pre-flight checks..."

# Check if running on EC2 with Nitro support
if ! command -v nitro-cli &> /dev/null; then
    print_error "Nitro CLI not found. Please install aws-nitro-enclaves-cli"
    echo "  Run: sudo yum install -y aws-nitro-enclaves-cli"
    exit 1
fi

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    print_error "Docker not found. Please install Docker"
    exit 1
fi

# Check if user has permissions for Docker
if ! docker ps &> /dev/null; then
    if ! sudo docker ps &> /dev/null; then
        print_error "Cannot access Docker. Please check permissions"
        exit 1
    else
        print_info "Using sudo for Docker commands"
        DOCKER_CMD="sudo docker"
    fi
else
    DOCKER_CMD="docker"
fi

# Check allocator configuration
print_info "Checking Nitro Enclaves allocator..."
ALLOCATOR_YAML="/etc/nitro_enclaves/allocator.yaml"
if [ -f "$ALLOCATOR_YAML" ]; then
    ALLOCATED_CPUS=$(grep "^cpu_count:" $ALLOCATOR_YAML 2>/dev/null | awk '{print $2}' || echo "0")
    ALLOCATED_MEMORY=$(grep "^memory_mib:" $ALLOCATOR_YAML 2>/dev/null | awk '{print $2}' || echo "0")
    
    if [ "$ALLOCATED_CPUS" -lt "$ENCLAVE_CPU_COUNT" ] || [ "$ALLOCATED_MEMORY" -lt "$ENCLAVE_MEMORY_MB" ]; then
        print_warning "Allocator may not have enough resources allocated"
        echo "  Current: CPUs=$ALLOCATED_CPUS, Memory=${ALLOCATED_MEMORY}MB"
        echo "  Needed:  CPUs=$ENCLAVE_CPU_COUNT, Memory=${ENCLAVE_MEMORY_MB}MB"
        echo ""
        echo "  To fix, edit $ALLOCATOR_YAML and run:"
        echo "  sudo systemctl restart nitro-enclaves-allocator.service"
    else
        print_status "Allocator has sufficient resources"
    fi
else
    print_warning "Cannot read allocator configuration"
fi

# ============================================================================
# TERMINATE EXISTING ENCLAVES
# ============================================================================

print_info "Checking for existing enclaves..."
EXISTING_ENCLAVES=$(nitro-cli describe-enclaves 2>/dev/null || echo "[]")

if [ "$EXISTING_ENCLAVES" != "[]" ]; then
    print_warning "Found existing enclave(s), terminating..."
    nitro-cli terminate-enclave --all 2>/dev/null || true
    sleep 2
fi

# ============================================================================
# BUILD DOCKER IMAGE
# ============================================================================

echo ""
echo -e "${BLUE}[BUILD]${NC} Building Docker image..."
cd /home/ec2-user/repos/world-chat-backend

# Build with detailed output for debugging
print_info "Building Docker image (this may take a few minutes)..."
$DOCKER_CMD build \
    --progress=plain \
    -t ${DOCKER_IMAGE_NAME}:${DOCKER_IMAGE_TAG} \
    -f secure-enclave/Dockerfile \
    . > /tmp/docker-build.log 2>&1

DOCKER_BUILD_EXIT=$?

# Show last few lines of output
if [ $DOCKER_BUILD_EXIT -eq 0 ]; then
    print_status "Docker build completed successfully"
    echo "Last few lines of build output:"
    tail -5 /tmp/docker-build.log
else
    print_error "Docker build failed! Exit code: $DOCKER_BUILD_EXIT"
    echo "Error details:"
    tail -20 /tmp/docker-build.log
    exit 1
fi

print_status "Docker image built successfully"

# ============================================================================
# BUILD ENCLAVE IMAGE FILE (EIF)
# ============================================================================

echo ""
echo -e "${BLUE}[BUILD]${NC} Building Enclave Image File (EIF)..."

nitro-cli build-enclave \
    --docker-uri ${DOCKER_IMAGE_NAME}:${DOCKER_IMAGE_TAG} \
    --output-file ${EIF_FILE} > /tmp/eif-build.log 2>&1

EIF_BUILD_EXIT=$?

if [ $EIF_BUILD_EXIT -eq 0 ]; then
    print_status "EIF build completed successfully"
    # Show the important info from the build
    grep -E "Enclave Image successfully created|PCR" /tmp/eif-build.log || true
else
    print_error "EIF build failed! Exit code: $EIF_BUILD_EXIT"
    echo "Error details:"
    tail -20 /tmp/eif-build.log
    exit 1
fi

# Extract PCR values for attestation
print_info "Extracting PCR measurements for attestation..."
PCR_INFO=$(nitro-cli describe-eif --eif-path ${EIF_FILE} 2>/dev/null || echo "{}")
PCR0=$(echo "$PCR_INFO" | jq -r '.Measurements.PCR0 // "N/A"')
PCR1=$(echo "$PCR_INFO" | jq -r '.Measurements.PCR1 // "N/A"')
PCR2=$(echo "$PCR_INFO" | jq -r '.Measurements.PCR2 // "N/A"')

echo -e "${CYAN}  PCR0:${NC} $PCR0"
echo -e "${CYAN}  PCR1:${NC} $PCR1"
echo -e "${CYAN}  PCR2:${NC} $PCR2"

print_status "EIF built successfully: ${EIF_FILE}"

# ============================================================================
# RUN THE ENCLAVE
# ============================================================================

echo ""
echo -e "${BLUE}[DEPLOY]${NC} Starting Nitro Enclave..."

# Build run command based on debug mode
RUN_CMD="nitro-cli run-enclave \
    --cpu-count ${ENCLAVE_CPU_COUNT} \
    --memory ${ENCLAVE_MEMORY_MB} \
    --enclave-cid ${ENCLAVE_CID} \
    --eif-path ${EIF_FILE}"

if [ "$DEBUG_MODE" = "true" ]; then
    RUN_CMD="$RUN_CMD --debug-mode"
    print_warning "Running in DEBUG MODE (not for production!)"
else
    print_info "Running in PRODUCTION MODE (no console access)"
fi

# Run the enclave
print_info "Starting enclave..."
$RUN_CMD > /tmp/enclave-run.log 2>&1

RUN_EXIT=$?

if [ $RUN_EXIT -eq 0 ]; then
    print_status "Enclave started successfully"
    # Show the output from the run command
    cat /tmp/enclave-run.log
else
    print_error "Failed to start enclave! Exit code: $RUN_EXIT"
    echo "Error details:"
    cat /tmp/enclave-run.log
    exit 1
fi

# ============================================================================
# VERIFY ENCLAVE IS RUNNING
# ============================================================================

sleep 3
echo ""
print_info "Verifying enclave status..."

ENCLAVE_INFO=$(nitro-cli describe-enclaves 2>/dev/null || echo "[]")
ENCLAVE_ID=$(echo "$ENCLAVE_INFO" | jq -r '.[0].EnclaveID // "unknown"')
ENCLAVE_STATE=$(echo "$ENCLAVE_INFO" | jq -r '.[0].State // "unknown"')

if [ "$ENCLAVE_STATE" = "RUNNING" ]; then
    print_status "Enclave is running successfully!"
else
    print_error "Enclave is not in RUNNING state. Current state: $ENCLAVE_STATE"
    echo "$ENCLAVE_INFO" | jq '.'
    exit 1
fi

# ============================================================================
# DISPLAY SUMMARY
# ============================================================================

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}         ✅ SECURE ENCLAVE DEPLOYED SUCCESSFULLY            ${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "${CYAN}Enclave Details:${NC}"
echo -e "  • Enclave ID: ${YELLOW}$ENCLAVE_ID${NC}"
echo -e "  • CID: ${YELLOW}$ENCLAVE_CID${NC}"
echo -e "  • CPUs: ${YELLOW}$ENCLAVE_CPU_COUNT${NC}"
echo -e "  • Memory: ${YELLOW}${ENCLAVE_MEMORY_MB}MB${NC}"
echo -e "  • State: ${GREEN}$ENCLAVE_STATE${NC}"
echo ""

if [ "$DEBUG_MODE" = "true" ]; then
    echo -e "${CYAN}Debug Commands:${NC}"
    echo -e "  • View console: ${YELLOW}nitro-cli console --enclave-id $ENCLAVE_ID${NC}"
    echo -e "  • Describe: ${YELLOW}nitro-cli describe-enclaves${NC}"
    echo -e "  • Terminate: ${YELLOW}nitro-cli terminate-enclave --enclave-id $ENCLAVE_ID${NC}"
    echo ""
    echo -e "${BLUE}[TIP]${NC} Opening console in a new terminal..."
    echo -e "      Run: ${YELLOW}nitro-cli console --enclave-id $ENCLAVE_ID${NC}"
fi

echo ""
echo -e "${GREEN}[NEXT]${NC} Now run the enclave worker: ${YELLOW}./scripts/run-enclave-worker.sh${NC}"
echo ""

# ============================================================================
# OPTIONAL: Start vsock-proxy
# ============================================================================

if [ -f "./scripts/setup-vsock-proxy.sh" ]; then
    echo -e "${YELLOW}[OPTIONAL]${NC} Start vsock-proxy for network access?"
    echo "  This is required for the enclave to access external APIs (Braze)"
    echo "  Press Enter to start vsock-proxy, or Ctrl+C to skip..."
    read -r
    
    ./scripts/setup-vsock-proxy.sh
fi
