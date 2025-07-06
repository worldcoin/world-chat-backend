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
# Build
cargo build

# Run backend server
cargo run --bin backend-server

# Run enclave server  
cargo run --bin enclave-server
```
