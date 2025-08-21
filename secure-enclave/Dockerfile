
########################################################
#### Builder
########################################################

FROM clux/muslrust:stable AS chef
USER root
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json --bin secure-enclave

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=secret,id=GITHUB_TOKEN \
    GITHUB_TOKEN=$(cat /run/secrets/GITHUB_TOKEN) && \
    git config --global url."https://${GITHUB_TOKEN}@github.com/worldcoin/".insteadOf "https://github.com/worldcoin/"

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json --package secure-enclave --bin secure-enclave
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl --package secure-enclave

########################################################
#### Runner
########################################################
FROM alpine AS runtime
RUN addgroup -S rust && adduser -S rust -G rust
WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/secure-enclave /usr/local/bin/

USER rust
CMD ["/usr/local/bin/secure-enclave"]
