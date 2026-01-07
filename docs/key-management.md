# Enclave Key Management

## Overview

This document describes how cryptographic keys are managed and distributed across AWS Nitro Enclaves in our system.

## Enclave Tracks

An **enclave track** is a group of enclaves that share the same cryptographic keys. This design enables:

- **Horizontal scaling** - Multiple enclaves can handle requests for the same data
- **High availability** - If one enclave fails, others in the track continue operating
- **Security isolation** - Different tracks have different keys, limiting blast radius

```mermaid
graph LR
    subgraph "Track: Production-v1"
        E1[Enclave 1]
        E2[Enclave 2]
        E3[Enclave 3]
    end

    subgraph "Track: Production-v2"
        E4[Enclave 4]
        E5[Enclave 5]
    end

    E1 -.-> E2
    E2 -.-> E3
    E4 -.-> E5

    style E1 fill:#e3f2fd
    style E2 fill:#e3f2fd
    style E3 fill:#e3f2fd
    style E4 fill:#fff3e0
    style E5 fill:#fff3e0
```

All enclaves within a track can decrypt the same push notification identifiers, but enclaves in different tracks cannot decrypt each other's data.

## Genesis: Creating a New Track

When a new track needs to be created, the first enclave to start becomes the **genesis enclave** and generates the cryptographic keys for that track.

```mermaid
sequenceDiagram
    participant E1 as First Enclave
    participant Redis as Redis Lock
    participant E2 as Second Enclave

    Note over E1,E2: Starting new track "prod-v1"

    E1->>Redis: Acquire lock for "prod-v1"
    Redis-->>E1: Lock acquired ✓

    Note over E1: I'm first!<br/>Generate keys

    E1->>E1: Generate X25519 keypair<br/>using hardware RNG

    E1->>Redis: Release lock

    Note over E1: Ready with keys

    E2->>Redis: Acquire lock for "prod-v1"
    Redis-->>E2: Lock unavailable

    Note over E2: Track exists!<br/>Need to get keys<br/>from E1
```

**Key Points:**

- Redis distributed lock ensures only one genesis enclave per track
- Keys are generated using hardware random number generator (RNG)
- Keys exist only in the enclave's memory - never written to disk

## Joining an Existing Track

When a new enclave starts and finds that its track already exists, it must obtain the keys from an existing member of that track.

```mermaid
sequenceDiagram
    participant New as New Enclave
    participant Existing as Existing Enclave

    Note over New: Starting in track "prod-v1"

    New->>New: Check Redis lock
    Note over New: Track exists!

    New->>Existing: Hello, I need the track keys

    rect rgb(230, 245, 255)
        Note over New,Existing: Secure Key Exchange
        New->>Existing: Here's my attestation
        Existing->>Existing: Verify attestation
        Existing->>New: Here are the encrypted keys<br/>and my attestation
        New->>New: Verify & decrypt
    end

    Note over New: Now I have the keys!
    Note over New,Existing: Both can serve requests
```

**Key Points:**

- New enclave discovers track already exists via Redis
- Keys are obtained directly from a running enclave
- Transfer is secured through attestation and encryption

## Secure Key Exchange

The secure key exchange is the critical security mechanism that ensures keys are only shared between legitimate enclaves running the same code.

### How It Works

```mermaid
sequenceDiagram
    participant New as New Enclave
    participant NSM1 as Hardware (New)
    participant Existing as Existing Enclave
    participant NSM2 as Hardware (Existing)

    Note over New: Need track keys

    New->>New: Generate ephemeral keypair

    New->>NSM1: Create attestation
    Note right of NSM1: Attestation contains:<br/>• Enclave measurements<br/>• Public key<br/>• Timestamp
    NSM1-->>New: Signed attestation

    New->>Existing: Request keys<br/>+ my attestation

    Existing->>Existing: Verify attestation:<br/>✓ Valid signature<br/>✓ Same code version<br/>✓ Fresh timestamp

    alt Verification Success
        Existing->>NSM2: Create attestation
        NSM2-->>Existing: Signed attestation

        Existing->>Existing: Encrypt track keys<br/>for New's public key

        Existing-->>New: Encrypted keys<br/>+ my attestation

        New->>New: Verify attestation<br/>& decrypt keys

        Note over New: Success!
    else Verification Failed
        Existing-->>New: Rejected:<br/>Different code version
        Note over New: Cannot join track
    end
```

### Why This Is Secure

**Key Security Properties:**

- **Code Identity**: PCR values ensure only enclaves running identical code can exchange keys
- **Hardware Root of Trust**: AWS Nitro hardware cannot be tampered with
- **Forward Secrecy**: Ephemeral keypairs are used once and destroyed
- **Memory-Only**: Keys never touch disk, only exist in protected RAM
- **Mutual Verification**: Both enclaves verify each other

## Failure Recovery

The system is designed to handle various failure scenarios:

```mermaid
graph TB
    F1[All enclaves<br/>in track die] --> R1[New enclave<br/>performs genesis]

    F2[Network issues<br/>during exchange] --> R2[Try different<br/>enclave]

    F3[Code version<br/>mismatch] --> R3[Cannot join -<br/>deploy matching version]

    F4[One enclave<br/>crashes] --> R4[Others continue<br/>serving]

    style R1 fill:#fff3e0
    style R2 fill:#fff3e0
    style R3 fill:#ffcdd2
    style R4 fill:#c8e6c9
```

## Summary

The key management system ensures that:

1. Each track has unique keys generated by the first enclave
2. New enclaves can securely join tracks by getting keys from peers
3. Only enclaves running identical, verified code can share keys
4. Keys never leave the secure enclave environment
5. The system continues operating even if individual enclaves fail

This design provides both security and scalability for managing encrypted push notification identifiers across a fleet of enclaves.

## Implementation Details

For developers looking to understand the implementation:

- [ **`secure-enclave-init`** ](../secure-enclave-init/src/main.rs) - Parent instance service that manages enclave lifecycle, handles Redis locking for genesis detection, and coordinates key distribution between enclaves
- [ **`/initialize` endpoint** ](../secure-enclave/src/pontifex_server/initialize.rs) - Enclave's initialization handler that receives configuration from parent, generates or retrieves track keys, and establishes the enclave's cryptographic identity
- [ **`/secret_key` endpoint** ](../secure-enclave/src/pontifex_server/secret_key.rs) - Returns the enclave's public key for client-side encryption and facilitates secure key exchange between enclaves in the same track
