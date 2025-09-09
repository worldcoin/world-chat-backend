# Use cargo-chef to cache dependencies
FROM rust:1.86.0-slim AS chef
WORKDIR /app

# Install dependencies for building
RUN apt-get update && apt-get install -y \
    musl-tools \
    clang \
    pkg-config \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef

# Plan stage - creates recipe.json for dependency caching
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build stage - cooks dependencies then builds application
FROM chef AS builder

# Copy recipe and build dependencies (cached if unchanged)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

ARG GIT_REV
ENV GIT_REV=${GIT_REV}

# Copy source code and build application
COPY . .
RUN cargo build --package enclave-worker --locked --release --target x86_64-unknown-linux-musl

# Runtime stage
FROM scratch

WORKDIR /app

# Copy SSL certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/enclave-worker /app/enclave-worker

# Set the entrypoint
USER 100
EXPOSE 8002
ENTRYPOINT ["/app/enclave-worker"]
