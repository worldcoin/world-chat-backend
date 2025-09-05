# Solving vsock-proxy Allowlist Error

The error "The given address and port are not allowed" means vsock-proxy is blocking connections to Braze. Here are multiple solutions:

## Solution 1: Use Environment Variable (Recommended)

Some versions of vsock-proxy accept an environment variable to bypass allowlist:

```bash
# Try with VSOCK_PROXY_ALLOW_ALL environment variable
sudo VSOCK_PROXY_ALLOW_ALL=1 vsock-proxy 8080 rest.iad-05.braze.com 443
```

## Solution 2: Use Different Proxy Approach

Instead of proxying directly to Braze, proxy to localhost and use socat or nginx:

```bash
# Step 1: vsock-proxy to localhost (usually allowed)
sudo vsock-proxy 8080 127.0.0.1 8081

# Step 2: Use socat to forward from localhost to Braze
socat TCP-LISTEN:8081,fork,reuseaddr TCP:rest.iad-05.braze.com:443
```

## Solution 3: Modify vsock-proxy Configuration

The vsock-proxy might have a config file at `/etc/nitro_enclaves/vsock-proxy.yaml`:

```bash
# Check if config exists
ls -la /etc/nitro_enclaves/

# If it exists, edit it:
sudo nano /etc/nitro_enclaves/vsock-proxy.yaml
```

Add Braze to the allowlist:
```yaml
allowlist:
  - host: rest.iad-05.braze.com
    port: 443
```

## Solution 4: Use Built-in AWS Endpoints

AWS Nitro instances often have pre-allowed endpoints. You can use these as a proxy:

```bash
# These are typically allowed by default
sudo vsock-proxy 8080 kms.us-east-1.amazonaws.com 443
# OR
sudo vsock-proxy 8080 s3.amazonaws.com 443
```

Then modify your enclave code to use a proxy service running on the parent instance.

## Solution 5: Run Without Restrictions (Development Only)

For development, you might be able to run vsock-proxy with less restrictions:

```bash
# Try running with different parameters
sudo vsock-proxy --any 8080 rest.iad-05.braze.com 443

# Or try binding to all interfaces
sudo vsock-proxy -b 0.0.0.0 8080 rest.iad-05.braze.com 443
```

## Solution 6: Use Parent Instance as Full Proxy

Instead of using vsock-proxy directly to Braze, set up a full HTTP proxy on the parent:

```bash
# Install squid or tinyproxy
sudo yum install -y squid

# Configure squid to allow Braze
echo "acl braze dstdomain .braze.com" | sudo tee -a /etc/squid/squid.conf
echo "http_access allow braze" | sudo tee -a /etc/squid/squid.conf

# Start squid
sudo systemctl start squid

# Then use vsock-proxy to squid
sudo vsock-proxy 8080 127.0.0.1 3128
```

## Quick Test Script

Here's a script to try multiple approaches:

```bash
#!/bin/bash

echo "Testing vsock-proxy configurations..."

# Test 1: With environment variable
echo "Test 1: VSOCK_PROXY_ALLOW_ALL=1"
sudo VSOCK_PROXY_ALLOW_ALL=1 vsock-proxy 8080 rest.iad-05.braze.com 443 2>&1 &
PID=$!
sleep 2
if ps -p $PID > /dev/null; then
    echo "✓ Success with VSOCK_PROXY_ALLOW_ALL"
    exit 0
else
    echo "✗ Failed"
fi

# Test 2: Local proxy
echo "Test 2: Local proxy approach"
sudo vsock-proxy 8080 127.0.0.1 8081 2>&1 &
if [ $? -eq 0 ]; then
    echo "✓ Local proxy works - need to set up socat"
    echo "Run: socat TCP-LISTEN:8081,fork,reuseaddr TCP:rest.iad-05.braze.com:443"
    exit 0
fi

# Test 3: Check for config file
echo "Test 3: Checking for config files"
if [ -f /etc/nitro_enclaves/vsock-proxy.yaml ]; then
    echo "Config file exists at /etc/nitro_enclaves/vsock-proxy.yaml"
    echo "Edit it to add Braze to allowlist"
else
    echo "No config file found"
fi
```

## Recommended Approach for Your Setup

Given your use case, I recommend:

1. **First, try the environment variable approach** (Solution 1)
2. **If that fails, use the localhost proxy approach** (Solution 2) 
3. **For production, properly configure the allowlist** (Solution 3)

The localhost proxy approach is often the most reliable for development.
