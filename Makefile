# Makefile for fast enclave development

# Variables
ENCLAVE_CID ?= 16
DEBUG_MODE ?= true

# Colors for output
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m

.PHONY: help build deploy clean console status kill all

help:
	@echo "Fast Enclave Development Commands:"
	@echo "  make build   - Build secure-enclave binary locally"
	@echo "  make deploy  - Build and deploy enclave (fast method)"
	@echo "  make all     - Clean, build, and deploy"
	@echo "  make console - Connect to enclave console"
	@echo "  make status  - Show enclave status"
	@echo "  make kill    - Terminate all enclaves"
	@echo "  make clean   - Clean build artifacts"

# Build the binary locally (uses cargo cache - FAST!)
build:
	@echo "$(YELLOW)Building secure-enclave...$(NC)"
	@cargo build --release --bin secure-enclave
	@echo "$(GREEN)✓ Build complete$(NC)"

# Deploy using fast method
deploy: build
	@echo "$(YELLOW)Copying binary...$(NC)"
	@cp target/release/secure-enclave secure-enclave/secure-enclave-binary
	@echo "$(YELLOW)Building Docker image...$(NC)"
	@sudo docker build -t secure-enclave:fast -f secure-enclave/Dockerfile.fast . 
	@echo "$(YELLOW)Building EIF...$(NC)"
	@sudo nitro-cli build-enclave --docker-uri secure-enclave:fast --output-file secure-enclave.eif 
	@echo "$(YELLOW)Deploying enclave...$(NC)"
	@sudo nitro-cli terminate-enclave --all 2>/dev/null || true
	@sleep 1
	@sudo nitro-cli run-enclave \
		--cpu-count 2 \
		--memory 2048 \
		--enclave-cid $(ENCLAVE_CID) \
		--attach-console \
		--eif-path secure-enclave.eif \
		$(if $(filter true,$(DEBUG_MODE)),--debug-mode,)
	@echo "$(GREEN)✓ Enclave deployed!$(NC)"
	@sudo nitro-cli describe-enclaves | jq -r '.[0] | "ID: \(.EnclaveID)\nCID: \(.EnclaveCID)"'

# Full cycle: clean, build, deploy
all: kill clean deploy

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

# Clean build artifacts
clean:
	@echo "$(YELLOW)Cleaning...$(NC)"
	@rm -f secure-enclave.eif secure-enclave-fast.eif
	@cargo clean 2>/dev/null || true
	@echo "$(GREEN)✓ Clean complete$(NC)"

# Quick rebuild and deploy (most common during development)
quick: deploy

http-proxy:
	@sudo docker run -d -p 5000:5000  --privileged --device=/dev/vsock --name socat alpine/socat tcp-listen:5000,fork,reuseaddr vsock-connect:16:5000

kill-http-proxy:
	@sudo docker rm -f socat

status-http-proxy:
	@sudo docker network inspect bridge | jq -r '.[0].Containers[] | select(.Name == "socat") | .IPv4Address'
