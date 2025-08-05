FROM rust:1.85.0-slim AS builder

ARG GIT_REV
ENV GIT_REV=${GIT_REV}

WORKDIR /app

RUN apt-get update && apt-get install -y \
    musl-tools \
    clang \
    pkg-config \
    build-essential \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

# Copy workspace files first
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY shared/backend_storage/Cargo.toml shared/backend_storage/Cargo.toml
COPY enclave-worker/Cargo.toml enclave-worker/Cargo.toml
COPY secure-enclave/Cargo.toml secure-enclave/Cargo.toml
COPY notification-worker/Cargo.toml notification-worker/Cargo.toml

# Cache dependencies
RUN mkdir -p backend/src && \
    echo "fn main() {}" > backend/src/main.rs && \
    mkdir -p shared/backend_storage/src && \
    echo "fn main() {}" > shared/backend_storage/src/lib.rs && \
    mkdir -p enclave-worker/src && \
    echo "fn main() {}" > enclave-worker/src/main.rs && \
    mkdir -p secure-enclave/src && \
    echo "fn main() {}" > secure-enclave/src/main.rs && \
    mkdir -p notification-worker/src && \
    echo "fn main() {}" > notification-worker/src/main.rs && \
    cargo build --package backend --locked --release && \
    rm -rf backend/src shared/backend_storage/src enclave-worker/src secure-enclave/src notification-worker/src

# Copy source code
COPY . .

# Build the backend application
RUN cargo build --package backend --locked --release --target x86_64-unknown-linux-musl

# Runtime stage
FROM scratch

WORKDIR /app

# Copy SSL certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/backend /app/backend

# Set the entrypoint
USER 100
EXPOSE 8000
ENTRYPOINT ["/app/backend"]
