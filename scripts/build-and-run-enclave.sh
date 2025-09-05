#!/bin/bash

# Script to build and run the secure enclave
set -e

echo "🔨 Building and deploying secure enclave..."

# Configuration
ENCLAVE_CID="${ENCLAVE_CID:-16}"
ENCLAVE_CPU="${ENCLAVE_CPU:-2}"
ENCLAVE_MEMORY="${ENCLAVE_MEMORY:-2048}"
DEBUG_MODE="${DEBUG_MODE:-true}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

# Check if running on EC2 with Nitro support
if ! command -v nitro-cli &> /dev/null; then
    print_error "Nitro CLI not found. Please install aws-nitro-enclaves-cli"
    exit 1
fi

# Terminate any existing enclaves
print_status "Terminating existing enclaves..."
nitro-cli terminate-enclave --all 2>/dev/null || true

# Build the Docker image
print_status "Building Docker image for secure-enclave..."
docker build -t secure-enclave:latest -f secure-enclave/Dockerfile .

if [ $? -ne 0 ]; then
    print_error "Failed to build Docker image"
    exit 1
fi

# Build the Enclave Image File (EIF)
print_status "Building Enclave Image File (EIF)..."
nitro-cli build-enclave \
    --docker-uri secure-enclave:latest \
    --output-file secure-enclave.eif

if [ $? -ne 0 ]; then
    print_error "Failed to build EIF"
    exit 1
fi

# Extract PCR values for attestation
print_status "Extracting PCR values..."
PCR0=$(nitro-cli describe-eif --eif-path secure-enclave.eif | jq -r '.Measurements.PCR0' 2>/dev/null || echo "N/A")
PCR1=$(nitro-cli describe-eif --eif-path secure-enclave.eif | jq -r '.Measurements.PCR1' 2>/dev/null || echo "N/A")
PCR2=$(nitro-cli describe-eif --eif-path secure-enclave.eif | jq -r '.Measurements.PCR2' 2>/dev/null || echo "N/A")

echo "  PCR0: $PCR0"
echo "  PCR1: $PCR1"
echo "  PCR2: $PCR2"

# Run the enclave
if [ "$DEBUG_MODE" = "true" ]; then
    print_warning "Starting enclave in DEBUG MODE (not for production)"
    nitro-cli run-enclave \
        --cpu-count $ENCLAVE_CPU \
        --memory $ENCLAVE_MEMORY \
        --enclave-cid $ENCLAVE_CID \
        --eif-path secure-enclave.eif \
        --debug-mode
else
    print_status "Starting enclave in PRODUCTION MODE"
    nitro-cli run-enclave \
        --cpu-count $ENCLAVE_CPU \
        --memory $ENCLAVE_MEMORY \
        --enclave-cid $ENCLAVE_CID \
        --eif-path secure-enclave.eif
fi

if [ $? -ne 0 ]; then
    print_error "Failed to start enclave"
    exit 1
fi

# Get enclave information
sleep 2
ENCLAVE_INFO=$(nitro-cli describe-enclaves)
ENCLAVE_ID=$(echo $ENCLAVE_INFO | jq -r '.[0].EnclaveID' 2>/dev/null)

if [ -z "$ENCLAVE_ID" ] || [ "$ENCLAVE_ID" = "null" ]; then
    print_error "Failed to get enclave ID"
    exit 1
fi

print_status "Enclave started successfully!"
echo ""
echo "Enclave Details:"
echo "  Enclave ID: $ENCLAVE_ID"
echo "  CID: $ENCLAVE_CID"
echo "  CPUs: $ENCLAVE_CPU"
echo "  Memory: ${ENCLAVE_MEMORY}MB"
echo ""

if [ "$DEBUG_MODE" = "true" ]; then
    echo "To view enclave console output:"
    echo "  nitro-cli console --enclave-id $ENCLAVE_ID"
    echo ""
    echo "Opening console in 3 seconds..."
    sleep 3
    nitro-cli console --enclave-id $ENCLAVE_ID
else
    echo "Enclave is running in production mode (no console access)"
fi
