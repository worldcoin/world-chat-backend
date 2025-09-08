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


ENCLAVE_CID ?= 16
DEBUG_MODE ?= true


# Colors for output
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m

build-secure-enclave:
	@echo "$(YELLOW)Building secure-enclave...$(NC)"
	@cargo build --release --bin secure-enclave
	@echo "$(GREEN)✓ Build complete$(NC)"

# Deploy using fast method
deploy-secure-enclave: build-secure-enclave
	@echo "$(YELLOW)Copying binary...$(NC)"
	@cp target/release/secure-enclave secure-enclave/secure-enclave-binary
	@echo "$(YELLOW)Building Docker image...$(NC)"
	@sudo docker build -t secure-enclave:fast -f secure-enclave/Dockerfile.fast . 
	@echo "$(YELLOW)Building EIF...$(NC)"
	@sudo nitro-cli build-enclave --docker-uri secure-enclave:fast --output-file secure-enclave.eif 
	@echo "$(YELLOW)Deploying enclave...$(NC)"
	@sudo nitro-cli terminate-enclave --all 2>/dev/null || true
	@sleep 1
	@echo "$(YELLOW)Running enclave... in debug mode $(DEBUG_MODE)$(NC)"
	@sudo nitro-cli run-enclave \
		--cpu-count 2 \
		--memory 2048 \
		--enclave-cid $(ENCLAVE_CID) \
		--eif-path secure-enclave.eif \
		$(if $(filter true,$(DEBUG_MODE)),--debug-mode,)
	@echo "$(GREEN)✓ Enclave deployed!$(NC)"
	@sudo nitro-cli describe-enclaves | jq -r '.[0] | "ID: \(.EnclaveID)\nCID: \(.EnclaveCID)"'


# Connect to console
console:
	@ENCLAVE_ID=$$(sudo nitro-cli describe-enclaves | jq -r '.[0].EnclaveID'); \
	if [ "$$ENCLAVE_ID" != "null" ]; then \
		sudo nitro-cli console --enclave-id $$ENCLAVE_ID; \
	else \
		echo "No running enclave found"; \
	fi

# Show status
status:
	@sudo nitro-cli describe-enclaves | jq '.'

# Kill all enclaves
kill:
	@echo "$(YELLOW)Terminating all enclaves...$(NC)"
	@sudo nitro-cli terminate-enclave --all 2>/dev/null || true
	@echo "$(GREEN)✓ Done$(NC)"
