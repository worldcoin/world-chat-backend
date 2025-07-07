.PHONY: help fmt lint check build test clean run-backend run-enclave audit

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

fmt: ## Format Rust code using rustfmt
	cargo fmt --all

lint: ## Run clippy lints
	cargo clippy --all-targets --all-features -- -D warnings

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
	cargo run --bin backend-server

run-enclave: ## Run the enclave server
	cargo run --bin enclave-server

audit: ## Run security audit
	cargo audit

watch-backend: ## Run backend server with auto-reload (requires cargo-watch)
	cargo watch -x 'run --bin backend-server'

watch-enclave: ## Run enclave server with auto-reload (requires cargo-watch)
	cargo watch -x 'run --bin enclave-server'

install-dev-tools: ## Install development tools
	rustup component add rustfmt clippy
	cargo install cargo-audit cargo-watch