# Secure Enclave - Privacy-Preserving Notification Service

## Overview

This AWS Nitro Enclave service enables push notifications while preserving user privacy through cryptographic isolation. The system stores encrypted push identifiers that can only be decrypted within the Trusted Execution Environment (TEE), preventing metadata leakage about user communications.

## Privacy Architecture

### The Problem

Traditional notification services require storing push tokens in plaintext, revealing:

- Which users are registered
- When users are active
- Potential correlation of user identities across services

### Our Solution

- **Encrypted Storage**: Push identifiers are encrypted before storage - only the enclave holds the decryption keys
- **Zero Knowledge**: The system doesn't know which users are communicating with whom
- **Hardware Isolation**: Decryption only happens within the attestable AWS Nitro Enclave
- **No Metadata Leakage**: Even system administrators cannot decrypt push identifiers or determine user relationships

## How It Works

1. **Client Registration**: Apps encrypt push tokens with the enclave's public key before sending
2. **Secure Storage**: Only encrypted push identifiers are stored in the database
3. **Notification Delivery**: When a notification is needed:
   - The encrypted push ID is sent to the enclave
   - The enclave decrypts it within the TEE
   - The notification is forwarded to the push service (e.g., Braze)
   - The decrypted identifier is never logged or persisted

### Security Features

- **Attestation**: Clients verify the enclave is running unmodified code
- **Key Management**: Sophisticated key distribution system for horizontal scaling (see [Key Management](../docs/key-management.md))
- **Memory-Only Operations**: Keys and decrypted data never touch disk
- **Hardware RNG**: Cryptographic operations use Nitro Security Module

## Deployment Architecture

```
┌─────────────────┐         ┌─────────────────┐
│   Client App    │         │   Database      │
│                 │         │                 │
│ Encrypts push   │         │ Stores only     │
│ ID with enclave │         │ encrypted IDs   │
│ public key      │         │                 │
└────────┬────────┘         └────────┬────────┘
         │                           │
         │                           │
         ▼                           ▼
┌─────────────────────────────────────────────┐
│            Parent EC2 Instance              │
│                                             │
│  ┌─────────────────────────────────────┐    │
│  │      Nitro Enclave (TEE)            │    │
│  │                                     │    │
│  │  • Holds decryption keys            │    │
│  │  • Decrypts push IDs in memory      │    │
│  │  • Forwards to notification service │    │
│  │  • Keys never leave enclave         │    │
│  │                                     │    │
│  └─────────────────────────────────────┘    │
│                                             │
└─────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────┐
              │  Push Service    │
              │  (e.g., Braze)   │
              └──────────────────┘
```
