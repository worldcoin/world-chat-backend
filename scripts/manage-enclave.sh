#!/bin/bash

# Script to manage Nitro Enclaves (start, stop, status, console)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

function show_usage() {
    echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}              NITRO ENCLAVE MANAGEMENT TOOL                  ${NC}"
    echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Usage: $0 [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  status    - Show status of all running enclaves"
    echo "  stop      - Terminate a specific enclave by ID"
    echo "  stop-all  - Terminate all running enclaves"
    echo "  console   - Connect to enclave console (debug mode only)"
    echo "  clean     - Clean up all enclave resources"
    echo "  logs      - Show recent enclave logs"
    echo ""
    echo "Examples:"
    echo "  $0 status"
    echo "  $0 stop i-xxx-enc-xxx"
    echo "  $0 stop-all"
    echo "  $0 console"
    echo ""
}

function show_status() {
    echo -e "${BLUE}[INFO]${NC} Checking enclave status..."
    echo ""
    
    ENCLAVES=$(nitro-cli describe-enclaves)
    
    if [ "$ENCLAVES" = "[]" ]; then
        echo -e "${YELLOW}[!]${NC} No running enclaves found."
        echo ""
        echo "To start an enclave, run: ./scripts/run-secure-enclave.sh"
    else
        echo -e "${GREEN}[✓]${NC} Found running enclave(s):"
        echo ""
        echo "$ENCLAVES" | jq -r '.[] | 
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n" +
            "Enclave ID:  \(.EnclaveID)\n" +
            "State:       \(.State)\n" +
            "CID:         \(.EnclaveCID)\n" +
            "CPUs:        \(.NumberOfCPUs) (IDs: \(.CPUIDs | join(", ")))\n" +
            "Memory:      \(.MemoryMiB) MB\n" +
            "Debug Mode:  \(if .Flags == "DEBUG_MODE" then "Yes" else "No" end)"'
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    fi
}

function stop_enclave() {
    if [ -z "$1" ]; then
        echo -e "${RED}[ERROR]${NC} Please provide an enclave ID"
        echo "Usage: $0 stop <enclave-id>"
        echo ""
        echo "Current enclaves:"
        nitro-cli describe-enclaves | jq -r '.[] | "  • \(.EnclaveID)"'
        exit 1
    fi
    
    ENCLAVE_ID=$1
    echo -e "${YELLOW}[!]${NC} Terminating enclave: $ENCLAVE_ID"
    
    nitro-cli terminate-enclave --enclave-id "$ENCLAVE_ID"
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}[✓]${NC} Enclave terminated successfully"
    else
        echo -e "${RED}[ERROR]${NC} Failed to terminate enclave"
        echo "The enclave might not exist or you don't have permissions"
    fi
}

function stop_all_enclaves() {
    echo -e "${YELLOW}[!]${NC} Terminating all running enclaves..."
    
    # Get list of enclaves before termination
    ENCLAVES=$(nitro-cli describe-enclaves)
    
    if [ "$ENCLAVES" = "[]" ]; then
        echo -e "${YELLOW}[!]${NC} No running enclaves to terminate"
        return
    fi
    
    # Show which enclaves will be terminated
    echo "$ENCLAVES" | jq -r '.[] | "  • Terminating: \(.EnclaveID)"'
    echo ""
    
    nitro-cli terminate-enclave --all
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}[✓]${NC} All enclaves terminated successfully"
    else
        echo -e "${RED}[ERROR]${NC} Failed to terminate some enclaves"
    fi
    
    # Verify all are terminated
    sleep 2
    REMAINING=$(nitro-cli describe-enclaves)
    if [ "$REMAINING" != "[]" ]; then
        echo -e "${YELLOW}[WARN]${NC} Some enclaves may still be running:"
        echo "$REMAINING" | jq -r '.[] | "  • \(.EnclaveID) - \(.State)"'
    fi
}

function connect_console() {
    ENCLAVES=$(nitro-cli describe-enclaves)
    
    if [ "$ENCLAVES" = "[]" ]; then
        echo -e "${RED}[ERROR]${NC} No running enclaves found"
        exit 1
    fi
    
    # Get enclave ID
    ENCLAVE_ID=$(echo "$ENCLAVES" | jq -r '.[0].EnclaveID')
    DEBUG_MODE=$(echo "$ENCLAVES" | jq -r '.[0].Flags')
    
    if [ "$DEBUG_MODE" != "DEBUG_MODE" ]; then
        echo -e "${RED}[ERROR]${NC} Enclave is not running in debug mode"
        echo "Console access is only available when enclave is started with --debug-mode"
        exit 1
    fi
    
    echo -e "${GREEN}[✓]${NC} Connecting to enclave console: $ENCLAVE_ID"
    echo -e "${YELLOW}[!]${NC} Press Ctrl+C to exit console"
    echo ""
    
    nitro-cli console --enclave-id "$ENCLAVE_ID"
}

function clean_resources() {
    echo -e "${YELLOW}[!]${NC} Cleaning up enclave resources..."
    
    # Terminate any running enclaves first
    nitro-cli terminate-enclave --all 2>/dev/null
    
    # Clean up resources
    sudo nitro-cli clean
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}[✓]${NC} Enclave resources cleaned successfully"
    else
        echo -e "${RED}[ERROR]${NC} Failed to clean some resources"
    fi
    
    # Remove any leftover EIF files
    if [ -f "secure-enclave.eif" ]; then
        echo -e "${BLUE}[INFO]${NC} Removing EIF file..."
        rm -f secure-enclave.eif
    fi
}

function show_logs() {
    echo -e "${BLUE}[INFO]${NC} Recent enclave-related logs:"
    echo ""
    
    # Check systemd logs for allocator
    echo -e "${CYAN}Allocator Service Logs:${NC}"
    sudo journalctl -u nitro-enclaves-allocator.service -n 20 --no-pager
    
    echo ""
    echo -e "${CYAN}Kernel Messages (dmesg):${NC}"
    sudo dmesg | grep -i nitro | tail -20
    
    # Check for any build logs
    if [ -f "/tmp/eif-build.log" ]; then
        echo ""
        echo -e "${CYAN}Last EIF Build Log:${NC}"
        tail -20 /tmp/eif-build.log
    fi
    
    if [ -f "/tmp/enclave-run.log" ]; then
        echo ""
        echo -e "${CYAN}Last Enclave Run Log:${NC}"
        tail -20 /tmp/enclave-run.log
    fi
}

# Main logic
case "${1:-}" in
    status)
        show_status
        ;;
    stop)
        stop_enclave "$2"
        ;;
    stop-all)
        stop_all_enclaves
        ;;
    console)
        connect_console
        ;;
    clean)
        clean_resources
        ;;
    logs)
        show_logs
        ;;
    *)
        show_usage
        ;;
esac
