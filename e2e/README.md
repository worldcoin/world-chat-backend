# E2E Testing & Load Testing

This directory contains end-to-end testing utilities and load testing tools for the World Chat Backend.

## Load Testing with Drill

### Prerequisites

1. Install drill (Rust HTTP load testing tool):
   ```bash
   cargo install drill
   ```

2. Make sure your backend server is running:
   ```bash
   cargo run --bin backend
   ```

### Generate Test Data

Generate World ID proofs for 100 test users:

```bash
# From the project root
cargo run --bin generate-test-data
```

This will:
- Generate 100 unique users with hardcoded World ID secrets (user_1 uses secret byte 1, user_2 uses 2, etc.)
- Create World ID proofs for each user using the staging environment
- Output a `user_data.csv` file with all authentication data

**Note:** Proof generation is CPU-intensive and may take several minutes to complete.

### Run Load Tests

Once `user_data.csv` is generated, run the load test:

```bash
drill --benchmark drill.yml --stats
```

### Configuration

Edit `drill.yml` to customize the load test:

- **concurrency**: Number of concurrent virtual users (default: 10)
- **iterations**: Number of requests each user makes (default: 100)
- **ramp_up**: Time in seconds to reach full concurrency (default: 10)
- **base**: Backend API base URL (default: http://localhost:8080)

### Output

Drill provides statistics including:
- Total requests
- Successful/failed requests
- Response time percentiles (p50, p90, p95, p99)
- Requests per second
- Average response time

### Example Output

```
Total requests            10000
Successful requests       10000
Failed requests           0
Median response time      45ms
Average response time     52ms
Sample std deviation      12ms
99.0'th response time     89ms
Slowest response time     156ms
Fastest response time     23ms
Requests per second       192.31
```

## CSV Data Format

The generated `user_data.csv` contains:

| Column | Description |
|--------|-------------|
| encrypted_push_id | Encrypted push notification ID |
| timestamp | Unix timestamp for proof signal |
| proof | Zero-knowledge proof string |
| nullifier_hash | Unique user identifier |
| merkle_root | World ID merkle tree root |
| credential_type | Type of World ID credential (Device) |

## Troubleshooting

### Proof generation fails
- Ensure you have network access (proofs are generated against World ID staging)
- Check that `walletkit-core` is properly configured

### Load test fails with 401/403 errors
- Regenerate test data (proofs expire after 5 minutes)
- Verify backend is running and accessible

### Rate limiting
- Reduce `concurrency` in `drill.yml`
- Increase `ramp_up` time for gradual load increase
