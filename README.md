# World Chat Backend

Push notification and image services for World Chat Native.

## Structure

```
world-chat-backend/
├── backend/                # Main HTTP server (port 3000)
├── enclave-worker/         # Worker for enclave operations
├── secure-enclave/         # Secure enclave service
└── shared/                 # Shared crates
    └──  backend_storage/   # Common SQS and DynamoDB structs
```

## Quick Start

```bash
# Install development tools
make install-dev-tools

# Build
make build

# Run backend server
make run-backend

# Run enclave worker
make run-enclave-worker

# Run secure enclave
make run-secure-enclave
```

## Development

```bash
# Format code
make fmt

# Run lints
make lint

# Run all checks
make check

# Run tests
make test

# See all commands
make help
```

### Code Quality

- **Formatting**: Enforced via `rustfmt`
- **Linting**: Default clippy rules
- **CI**: Automated checks on every PR
