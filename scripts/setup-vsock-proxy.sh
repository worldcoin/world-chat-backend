#!/bin/bash

# Script to setup vsock-proxy for enclave network access
# This allows the secure enclave to access external APIs through the parent instance

set -e

echo "🔧 Setting up vsock-proxy for enclave network access..."

# Install vsock-proxy if not already installed
if ! command -v vsock-proxy &> /dev/null; then
    echo "📦 Installing vsock-proxy..."
    
    # Download and install vsock-proxy
    sudo yum install -y aws-nitro-enclaves-cli-devel
    
    # Build vsock-proxy from source if not available
    if ! command -v vsock-proxy &> /dev/null; then
        echo "Building vsock-proxy from source..."
        git clone https://github.com/aws/aws-nitro-enclaves-sdk-c.git /tmp/nitro-sdk
        cd /tmp/nitro-sdk
        cmake -DCMAKE_BUILD_TYPE=Release .
        make
        sudo make install
        cd -
    fi
fi

# Default configuration
BRAZE_ENDPOINT="${BRAZE_ENDPOINT:-rest.iad-01.braze.com}"
BRAZE_PORT="${BRAZE_PORT:-443}"
LOCAL_PORT="${LOCAL_PORT:-8080}"
ENCLAVE_CID="${ENCLAVE_CID:-16}"

echo "📡 Starting vsock-proxy..."
echo "   Local port: $LOCAL_PORT"
echo "   Target: $BRAZE_ENDPOINT:$BRAZE_PORT"
echo "   Enclave CID: $ENCLAVE_CID"

# Kill any existing vsock-proxy process
sudo pkill -f vsock-proxy || true

# Start vsock-proxy in the background
# This forwards traffic from the enclave to the Braze API
sudo vsock-proxy $LOCAL_PORT $BRAZE_ENDPOINT $BRAZE_PORT &

echo "✅ vsock-proxy started with PID: $!"
echo ""
echo "The enclave can now access Braze API at: http://127.0.0.1:$LOCAL_PORT"
echo ""
echo "To stop the proxy, run: sudo pkill -f vsock-proxy"
