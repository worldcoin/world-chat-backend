# Use cargo-chef to cache dependencies
FROM rust:1.86.0-slim AS chef
WORKDIR /app

# Install dependencies for building
RUN apt-get update && apt-get install -y \
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

# Copy source code and build application
COPY . .
RUN cargo build --package enclave-worker --locked --release

# Runtime stage - Amazon Linux 2023
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal

# Install runtime dependencies using microdnf (available in minimal image)
RUN microdnf update -y && \
    microdnf install -y \
    ca-certificates \
    && microdnf clean all

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/enclave-worker /usr/local/bin/enclave-worker

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/enclave-worker"]
