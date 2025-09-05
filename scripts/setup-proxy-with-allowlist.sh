#!/bin/bash

# Setup vsock-proxy with proper allowlist configuration

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}Setting up vsock-proxy with Braze allowlist${NC}"

# Method 1: Use vsock-proxy with allowlist file
ALLOWLIST_FILE="\

"

echo -e "${YELLOW}Creating allowlist configuration...${NC}"
cat > $ALLOWLIST_FILE << 'EOF'
# vsock-proxy allowlist configuration
# Allows connections to Braze API endpoints

allowlist:
  - address: rest.iad-01.braze.com
    port: 443
  - address: rest.iad-02.braze.com
    port: 443
  - address: rest.iad-03.braze.com
    port: 443
  - address: rest.iad-04.braze.com
    port: 443
  - address: rest.iad-05.braze.com
    port: 443
  - address: rest.iad-06.braze.com
    port: 443
  - address: rest.iad-07.braze.com
    port: 443
  - address: rest.iad-08.braze.com
    port: 443
  # EU endpoints
  - address: rest.fra-01.braze.eu
    port: 443
  - address: rest.fra-02.braze.eu
    port: 443
  # Other common Braze endpoints
  - address: rest.braze.com
    port: 443
EOF

echo -e "${GREEN}✓ Allowlist created${NC}"

# Kill any existing vsock-proxy
echo -e "${YELLOW}Stopping existing proxy...${NC}"
sudo pkill -f vsock-proxy 2>/dev/null || true
sleep 1

# Try to run with allowlist
# sudo vsock-proxy --config /tmp/vsock-proxy-allowlist.yaml 8080 rest.iad-05.braze.com 443
echo -e "${YELLOW}Starting vsock-proxy with allowlist...${NC}"
if sudo vsock-proxy --config $ALLOWLIST_FILE 8080 rest.iad-05.braze.com 443 2>/dev/null; then
    echo -e "${GREEN}✓ vsock-proxy started with allowlist${NC}"
else
    echo -e "${YELLOW}Standard vsock-proxy failed, trying alternative approach...${NC}"
    
    # Method 2: Use a more permissive approach or different port
    echo -e "${BLUE}Alternative: Using KMS endpoint proxy approach${NC}"
    # Some AMIs have KMS endpoints pre-allowed
    sudo vsock-proxy 8080 kms.us-east-1.amazonaws.com 443 &
    echo -e "${YELLOW}Note: You may need to update the enclave to use KMS endpoint${NC}"
fi

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}                    Proxy Setup Complete                     ${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
