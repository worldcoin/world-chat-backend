# World Chat Backend

Push notification and image services for World Chat Native.

## Structure

```
world-chat-backend/
├── servers/                    # HTTP servers
│   ├── backend-server/         # Main server (port 3000)
│   └── enclave-server/         # Enclave server (port 3001)
├── services/                   # Business logic
│   ├── notification-service/
│   ├── image-service/
│   └── enclave-service/
└── shared/
    └── models/                 # Common types
```

## Quick Start

```bash
# Install development tools
make install-dev-tools

# Build
make build

# Run backend server
make run-backend

# Run enclave server
make run-enclave
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

# Pre-commit checks
make pre-commit

# See all commands
make help
```

### Code Quality

- **Formatting**: Enforced via `rustfmt`
- **Linting**: Strict clippy rules with pedantic and nursery lints
- **CI**: Automated checks on every PR
- **Safety**: No unsafe code allowed
