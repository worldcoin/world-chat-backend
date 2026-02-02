# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

World Chat Backend provides push notification and image services for World Chat Native. It's a Rust workspace with multiple crates running in AWS Nitro Enclaves for secure message processing.

## Build Commands

```bash
make build          # Build all crates (debug)
make build-release  # Build all crates (release)
make fmt            # Format code
make lint           # Run clippy lints
make check          # Run fmt + lint
make test           # Run all tests
make audit          # Run cargo-deny (security, license, bans)
```

### Running Services

```bash
make run-backend         # HTTP API server (port 3000)
make run-enclave-worker  # Enclave communication worker
make run-secure-enclave  # Secure enclave service
```

### Running Tests

Tests require LocalStack and Redis:
```bash
docker compose up -d --wait localstack redis
cargo test -- --nocapture
cargo test <test_name>              # Single test
cargo test --package backend        # Tests for specific crate
```

## Architecture

### Workspace Crates

- **backend/** - Main HTTP API server. Handles media uploads (S3), auth proofs, push subscriptions (DynamoDB), JWT via KMS, World ID verification
- **notification-worker/** - Connects to XMTP node, processes incoming messages, queues notifications to SQS
- **enclave-worker/** - Runs outside enclave, processes notification queue, communicates with secure-enclave via Pontifex
- **secure-enclave/** - Runs inside AWS Nitro Enclave, handles encryption operations, uses NSM hardware RNG
- **shared/backend_storage/** - DynamoDB storage (auth proofs, push subscriptions, group invites) and SQS queue operations
- **shared/enclave-types/** - Types shared between enclave-worker and secure-enclave
- **shared/common-types/** - Common types across all crates
- **shared/attestation-verifier/** - AWS Nitro attestation document verification

### Key Technologies

- **Pontifex** - Communication between enclave-worker and secure-enclave over vsock
- **AWS Services** - S3 (media), DynamoDB (storage), SQS (queues), KMS (JWT signing)
- **XMTP** - Decentralized messaging protocol (proto files in notification-worker)
- **World ID** - Identity verification via semaphore proofs

## Authentication Flow

Authentication uses World ID zero-knowledge proofs to verify users without revealing identity.

1. **Client Request**: Client sends `/v1/authorize` with:
   - `encrypted_push_id`: Push notification identifier encrypted with enclave's public key
   - `timestamp`: Current time (proof valid for 5-minute window)
   - `proof`, `nullifier_hash`, `merkle_root`, `credential_type`: World ID ZK proof components

2. **Signal Validation**: Backend combines `encrypted_push_id:timestamp` as the signal, ensuring the proof is fresh and bound to this specific push ID

3. **World ID Verification**: Proof is verified against World ID's on-chain merkle tree

4. **Auth Proof Storage** (DynamoDB):
   - Nullifier hash is the primary key (unique per World ID user)
   - Stores the user's `encrypted_push_id` and `push_id_rotated_at` timestamp
   - TTL of 6-8 months (random for privacy), refreshed on activity

5. **Push ID Rotation Logic**:
   - If push IDs match: Issue JWT with stored encrypted push ID
   - If mismatch but within 6-month cooldown: Reject (prevents impersonation)
   - If mismatch and cooldown expired: Rotate push ID and issue new JWT

6. **JWT Issuance**: ES256 JWT signed via AWS KMS, contains encrypted push ID as subject

7. **Protected Endpoints**: Auth middleware validates JWT and extracts `AuthenticatedUser` with `encrypted_push_id`

**Source:** `backend/src/routes/v1/auth.rs`, `backend/src/middleware/auth.rs`, `backend/src/jwt/mod.rs`, `shared/backend_storage/src/auth_proof/mod.rs`

## Push Subscription Flow

Push subscriptions link XMTP conversation topics to users for notification delivery.

### Storage Model (DynamoDB)
- **Primary Key**: `topic` (XMTP conversation ID)
- **Sort Key**: `hmac_key` (84 hex chars, rotates every 30-day XMTP epoch)
- **Attributes**: `encrypted_push_id`, `ttl` (max 40 days), `deletion_request` (set of push IDs requesting deletion)

### Subscribe (`POST /v1/subscriptions`)
- Authenticated endpoint - uses JWT's `encrypted_push_id`
- Accepts array of `{topic, hmac_key, ttl}` objects
- Upserts subscriptions (idempotent) - same topic+hmac_key overwrites
- TTL gets random 1-minute to 24-hour offset to prevent timing analysis

### Unsubscribe (`DELETE /v1/subscriptions` or `POST /v1/subscriptions/delete`)
- **If requester owns subscription** (encrypted_push_id matches): Immediate deletion
- **If requester doesn't own**: Adds their encrypted_push_id to `deletion_request` set (tombstone)
- Tombstones enable lazy deletion when plaintext push IDs are compared in the enclave

### Batch Operations
- Batch unsubscribe fetches all subscriptions, partitions by ownership, executes deletions and tombstones concurrently
- DynamoDB batch limits: 25 items per request

**Source:** `backend/src/routes/v1/subscriptions.rs`, `shared/backend_storage/src/push_subscription/mod.rs`

## Notification Delivery Flow

Notification delivery is asynchronous with three components working together.

### 1. notification-worker (XMTP Listener)
- Connects to XMTP node via gRPC, streams all messages
- Filters: Only V3 topics, only messages where `should_push != false`
- For each message:
  - Queries DynamoDB for all subscriptions on the topic
  - Filters out self-notifications (sender's HMAC key matches subscription)
  - Collects unique `encrypted_push_id`s
  - Publishes `Notification` to SQS FIFO queue with topic, recipients, and base64-encoded message

### 2. enclave-worker (Queue Processor)
- Polls SQS queue for `Notification` messages
- Splits recipients into batches (configurable `recipients_per_batch`)
- Sends each batch to secure-enclave via Pontifex (vsock)
- Handles partial failures: Acknowledges message if at least one batch succeeds
- Metrics: `notification_queued`, `notification_delivered`

### 3. secure-enclave (Encryption & Delivery)
- Receives `EnclaveNotificationRequest` with encrypted push IDs
- Decrypts push IDs using X25519 private key (only enclave has this)
- Delivers notifications to Braze API
- All cryptographic operations use NSM hardware RNG

### Data Flow Summary
```
XMTP Message → notification-worker → SQS Queue → enclave-worker → secure-enclave → Braze → User Device
```

**Source:** `notification-worker/src/worker/mod.rs`, `notification-worker/src/worker/message_processor.rs`, `enclave-worker/src/notification_processor/mod.rs`, `shared/backend_storage/src/queue/`

## Secure Enclave Key Management

The secure-enclave runs inside AWS Nitro Enclaves with cryptographic isolation.

### Enclave Tracks
An **enclave track** is a group of enclaves sharing the same cryptographic keys, enabling horizontal scaling and high availability. Different tracks have different keys for security isolation.

### Genesis (New Track)
When creating a new track, the first enclave becomes the **genesis enclave**:
1. Acquires Redis distributed lock for the track name
2. Generates X25519 keypair using hardware RNG
3. Keys exist only in enclave memory - never written to disk

### Joining an Existing Track
When an enclave finds its track already exists:
1. Discovers track via Redis lock (lock unavailable = track exists)
2. Requests keys from an existing enclave in the track
3. Secure key exchange via mutual attestation verification

### Secure Key Exchange Protocol
1. New enclave generates ephemeral keypair
2. New enclave creates attestation document (contains measurements, public key, timestamp)
3. Sends attestation to existing enclave
4. Existing enclave verifies: valid signature, same code version (PCR values), fresh timestamp
5. If valid: existing enclave encrypts track keys for new enclave's public key, sends with its own attestation
6. New enclave verifies and decrypts keys

**Security Properties:**
- PCR values ensure only identical code can exchange keys
- Hardware root of trust (AWS Nitro) cannot be tampered with
- Ephemeral keypairs provide forward secrecy
- Keys never touch disk, only protected RAM
- Mutual verification between enclaves

### Failure Recovery
- All enclaves die → New enclave performs genesis
- Network issues → Try different enclave
- Code version mismatch → Cannot join (deploy matching version)
- Single enclave crash → Others continue serving

### Implementation
- **`secure-enclave-init`** - Parent instance service managing enclave lifecycle, Redis locking, key distribution coordination
- **`/initialize` endpoint** - Receives config, generates or retrieves track keys
- **`/secret_key` endpoint** - Returns public key, facilitates key exchange between track members

**Source:** `secure-enclave-init/src/main.rs`, `secure-enclave/src/state.rs`, `secure-enclave/src/encryption.rs`, `secure-enclave/src/pontifex_server/`

## Development Environment

Copy `backend/.env.example` to `backend/.env` for local development.

Docker Compose provides:
- LocalStack (S3, DynamoDB, SQS, KMS) on port 4566
- XMTP node on ports 5555/5556
- Redis on port 6379
- PostgreSQL for XMTP (ports 25432)

## Code Conventions

- Rust 1.86.0 (specified in rust-toolchain.toml)
- Max line width: 100 chars
- OpenSSL is banned - use rustls
- Strict clippy lints on shared crates (`clippy::pedantic`, `clippy::nursery`)

## Development Practices

1. **Before committing and before finishing work**: Run `make check` to ensure code is formatted and passes lints

2. **Running integration tests**: Spin up the required Docker Compose services first. Check the CI workflow for the exact services needed:
   ```bash
   # For most tests
   docker compose up -d --wait localstack redis
   cargo test -- --nocapture

   # For tests requiring XMTP
   docker compose up -d --wait localstack xmtp-node
   ```
