# AWS Nitro Enclave Notification System

This is a prototype implementation of a secure notification system using AWS Nitro Enclaves. The system consists of two main components:

1. **Enclave Worker** - Runs on the EC2 parent instance and sends notification requests
2. **Secure Enclave** - Runs inside the Nitro Enclave and securely handles Braze API calls

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                   EC2 Nitro Instance                     │
│                                                          │
│  ┌──────────────────┐        ┌─────────────────────┐   │
│  │  Enclave Worker  │        │   Secure Enclave    │   │
│  │                  │ vsock  │                     │   │
│  │  - Receives      │───────▶│  - Stores API Key   │   │
│  │    notifications │  :5000 │  - Calls Braze API  │   │
│  │  - Uses Pontifex │        │  - Uses Pontifex    │   │
│  └──────────────────┘        └──────────┬──────────┘   │
│                                         │               │
│  ┌──────────────────────────────────────▼──────────┐   │
│  │              vsock-proxy (Port 8080)             │   │
│  │         Forwards traffic to Braze API            │   │
│  └──────────────────────────────────────────────────┘   │
│                           │                              │
└───────────────────────────┼──────────────────────────────┘
                           │
                           ▼
                    Braze API (HTTPS)
```

## Prerequisites

1. **EC2 Instance Requirements:**
   - Instance type with Nitro Enclave support (e.g., m5.xlarge, c5.xlarge)
   - Amazon Linux 2 or Amazon Linux 2023
   - At least 4 vCPUs (2 for parent, 2 for enclave)
   - At least 4GB RAM

2. **Software Requirements:**
   - AWS Nitro Enclaves CLI
   - Docker
   - Rust toolchain (for development)

## Setup Instructions

### 1. Launch EC2 Instance

```bash
# Launch an EC2 instance with Nitro Enclave support
aws ec2 run-instances \
  --image-id ami-0c02fb55731490381 \
  --instance-type m5.xlarge \
  --key-name your-key-pair \
  --enclave-options 'Enabled=true' \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=nitro-enclave-demo}]'
```

### 2. Connect to Instance and Install Dependencies

```bash
# SSH into the instance
ssh -i your-key.pem ec2-user@<instance-ip>

# Install Nitro CLI
sudo yum install -y aws-nitro-enclaves-cli aws-nitro-enclaves-cli-devel

# Enable Docker and Nitro Enclaves
sudo usermod -aG ne ec2-user
sudo usermod -aG docker ec2-user

# Configure allocator (dedicates resources to enclaves)
sudo sed -i 's/^#.*cpu_count.*/cpu_count: 2/' /etc/nitro_enclaves/allocator.yaml
sudo sed -i 's/^#.*memory_mib.*/memory_mib: 2048/' /etc/nitro_enclaves/allocator.yaml

# Restart services
sudo systemctl restart nitro-enclaves-allocator.service
sudo systemctl restart docker

# Re-login for group changes to take effect
exit
# SSH back in
```

### 3. Clone and Build the Project

```bash
# Clone the repository
git clone <your-repo-url>
cd world-chat-backend

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Build the project
cargo build --release
```

### 4. Build the Secure Enclave Image

```bash
# Build the Docker image for the secure enclave
docker build -t secure-enclave:latest -f secure-enclave/Dockerfile .

# Convert to Enclave Image File (EIF)
nitro-cli build-enclave \
  --docker-uri secure-enclave:latest \
  --output-file secure-enclave.eif

# Note the PCR values and enclave measurements for attestation
```

### 5. Start the Enclave

```bash
# Run the enclave in debug mode (for development)
nitro-cli run-enclave \
  --cpu-count 2 \
  --memory 2048 \
  --enclave-cid 16 \
  --eif-path secure-enclave.eif \
  --debug-mode

# Get the enclave ID
ENCLAVE_ID=$(nitro-cli describe-enclaves | jq -r '.[0].EnclaveID')
echo "Enclave ID: $ENCLAVE_ID"

# View enclave console output (debug mode only)
nitro-cli console --enclave-id $ENCLAVE_ID
```

### 6. Setup vsock-proxy for Network Access

```bash
# Make the script executable
chmod +x scripts/setup-vsock-proxy.sh

# Run the vsock-proxy setup
# This allows the enclave to access the Braze API
./scripts/setup-vsock-proxy.sh
```

### 7. Start the Enclave Worker

```bash
# Set environment variables
export BRAZE_API_KEY="your-braze-api-key"
export BRAZE_API_ENDPOINT="https://rest.iad-01.braze.com"
export ENCLAVE_CID=16
export ENCLAVE_PORT=5000
export PROXY_HOST="127.0.0.1"
export PROXY_PORT=8080

# Run the enclave worker
cargo run --release --bin enclave-worker

# Or use the Docker image
docker build -t enclave-worker:latest -f enclave-worker/Dockerfile .
docker run --network host \
  -e BRAZE_API_KEY \
  -e BRAZE_API_ENDPOINT \
  -e ENCLAVE_CID \
  -e ENCLAVE_PORT \
  -e PROXY_HOST \
  -e PROXY_PORT \
  enclave-worker:latest
```

## Testing the Implementation

### 1. Check Enclave Status

```bash
# List running enclaves
nitro-cli describe-enclaves

# Expected output:
# [
#   {
#     "EnclaveID": "i-xxx-enc-xxx",
#     "ProcessID": 12345,
#     "EnclaveCID": 16,
#     "NumberOfCPUs": 2,
#     "CPUIDs": [2, 3],
#     "MemoryMiB": 2048,
#     "State": "RUNNING",
#     "Flags": "DEBUG_MODE"
#   }
# ]
```

### 2. Monitor Logs

```bash
# Terminal 1: Enclave console (debug mode only)
nitro-cli console --enclave-id $ENCLAVE_ID

# Terminal 2: Enclave worker logs
# The worker will output detailed logs showing:
# - Initialization status
# - Health check results
# - Notification send attempts
# - Response details
```

### 3. Verify Communication

The enclave worker will automatically:
1. Initialize the secure enclave with Braze configuration
2. Perform a health check
3. Send a test notification
4. Continue sending periodic test notifications every 30 seconds

Look for these log messages:

```
🚀 Starting Enclave Worker
📡 Connecting to enclave at CID: 16, Port: 5000
🔐 Initializing secure enclave with Braze configuration...
✅ Enclave initialized successfully
🏥 Performing health check...
✅ Health check passed. Enclave initialized: true
📨 Sending test notification...
✅ Notification sent successfully!
```

### 4. Test Custom Notifications

You can modify the enclave-worker code to send custom notifications:

```rust
let notification = NotificationRequest {
    request_id: uuid::Uuid::new_v4().to_string(),
    external_user_id: "user123".to_string(),
    title: "Custom Alert".to_string(),
    message: "This is a custom notification".to_string(),
    custom_data: Some(HashMap::from([
        ("priority".to_string(), "high".to_string()),
        ("category".to_string(), "alert".to_string()),
    ])),
    trigger_properties: None,
};
```

## Debugging Tips

### 1. Enable Debug Logging

```bash
# Set log level for both components
export RUST_LOG=debug

# For more detailed Pontifex communication logs
export RUST_LOG=pontifex=trace,enclave_worker=debug,secure_enclave=debug
```

### 2. Common Issues and Solutions

**Issue: "Failed to connect to enclave"**
- Solution: Verify enclave is running with `nitro-cli describe-enclaves`
- Check CID and port match between worker and enclave

**Issue: "Network error in enclave"**
- Solution: Ensure vsock-proxy is running
- Check proxy configuration matches in both components

**Issue: "Enclave not initialized"**
- Solution: Worker must call Initialize before sending notifications
- Check Braze API key is correctly set

**Issue: "Failed to build enclave"**
- Solution: Ensure Docker image builds successfully first
- Check sufficient memory/CPU allocated in allocator.yaml

### 3. Testing Without Enclave (Development)

For development, you can test the components outside of an enclave:

```bash
# Terminal 1: Run secure-enclave directly
RUST_LOG=debug cargo run --release --bin secure-enclave

# Terminal 2: Run enclave-worker
# Set ENCLAVE_CID to 3 (local CID) for testing
export ENCLAVE_CID=3
RUST_LOG=debug cargo run --release --bin enclave-worker
```

### 4. Monitor vsock-proxy

```bash
# Check if vsock-proxy is running
ps aux | grep vsock-proxy

# View proxy logs (if configured)
sudo journalctl -u vsock-proxy -f

# Test proxy connectivity
curl -v http://127.0.0.1:8080/test
```

## Production Considerations

### 1. Security Best Practices

- **Never run enclaves in debug mode in production**
  ```bash
  # Production enclave launch (without --debug-mode)
  nitro-cli run-enclave \
    --cpu-count 2 \
    --memory 2048 \
    --enclave-cid 16 \
    --eif-path secure-enclave.eif
  ```

- **Implement attestation verification** to ensure enclave integrity
- **Use AWS KMS** for key management instead of environment variables
- **Rotate API keys regularly** and store them securely

### 2. Performance Optimization

- Increase CPU and memory allocation for better performance
- Use connection pooling for Braze API calls
- Implement request batching for high-volume notifications
- Add caching for frequently used data

### 3. Monitoring and Observability

- Use CloudWatch for metrics and logs
- Implement health checks and alarms
- Track notification success/failure rates
- Monitor enclave resource utilization

### 4. High Availability

- Run multiple enclave instances for redundancy
- Implement load balancing between workers
- Use SQS or similar for reliable message queuing
- Add retry logic with exponential backoff

## Troubleshooting Commands

```bash
# Stop all enclaves
nitro-cli terminate-enclave --all

# Clean up enclave resources
sudo nitro-cli clean

# Check allocator status
systemctl status nitro-enclaves-allocator.service

# View allocator configuration
cat /etc/nitro_enclaves/allocator.yaml

# Check kernel support for enclaves
dmesg | grep -i nitro

# Verify vsock module is loaded
lsmod | grep vsock

# Test vsock connectivity (requires socat)
sudo yum install -y socat
socat - VSOCK-CONNECT:16:5000
```

## Additional Resources

- [AWS Nitro Enclaves Documentation](https://docs.aws.amazon.com/enclaves/)
- [Pontifex Library Documentation](https://docs.rs/pontifex)
- [Braze API Documentation](https://www.braze.com/docs/api/basics/)
- [vsock-proxy Documentation](https://github.com/aws/aws-nitro-enclaves-sdk-c)

## License

This prototype is provided as-is for demonstration purposes.
