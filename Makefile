.PHONY: help fmt lint check build test clean run-backend run-enclave-worker run-secure-enclave audit

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

fmt: ## Format Rust code using rustfmt
	cargo fmt --all

lint: ## Run clippy lints
	cargo clippy --all-targets --all-features --

check: fmt lint ## Run all checks (format + lint)
	@echo "All checks passed!"

build: ## Build the project in debug mode
	cargo build

build-release: ## Build the project in release mode
	cargo build --release

test: ## Run all tests
	cargo test --all

clean: ## Clean build artifacts
	cargo clean

run-backend: ## Run the backend server
	cargo run --bin backend

run-enclave-worker: ## Run the enclave worker
	cargo run --bin enclave-worker

run-secure-enclave: ## Run the secure enclave
	cargo run --bin secure-enclave

audit: ## Run security, license, and ban checks
	cargo deny check

watch-backend: ## Run backend server with auto-reload (requires cargo-watch)
	cargo watch -x 'run --bin backend'

watch-enclave-worker: ## Run enclave worker with auto-reload (requires cargo-watch)
	cargo watch -x 'run --bin enclave-worker'

install-dev-tools: ## Install development tools
	rustup component add rustfmt clippy
	cargo install cargo-deny cargo-watch
