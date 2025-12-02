#!/usr/bin/env bash
# Builds the secure enclave on a Nitro-enabled EC2 and outputs PCR0, PCR1, PCR2.
# Usage: ./verify-enclave-pcrs.sh <commit>

set -euo pipefail

COMMIT="${1:?Usage: $0 <commit>}"
REPO_URL="https://github.com/worldcoin/world-chat-backend.git"
WORK_DIR=$(mktemp -d)
trap "rm -rf $WORK_DIR" EXIT

git clone "$REPO_URL" "$WORK_DIR"
cd "$WORK_DIR"
git checkout "$COMMIT"

docker build -t world-chat-secure-enclave:local -f secure-enclave/Dockerfile .
nitro-cli build-enclave --docker-uri world-chat-secure-enclave:local --output-file enclave.eif > measurements.json

echo "Commit: $(git rev-parse HEAD)"
jq '.Measurements | {PCR0, PCR1, PCR2}' measurements.json
