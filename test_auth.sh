#!/bin/bash
# Test authentication endpoint with a single user from CSV

# Extract first user data from CSV
DATA=$(head -2 user_data.csv | tail -1)

encrypted_push_id=$(echo "$DATA" | cut -d',' -f1)
timestamp=$(echo "$DATA" | cut -d',' -f2)
proof=$(echo "$DATA" | cut -d',' -f3)
nullifier_hash=$(echo "$DATA" | cut -d',' -f4)
merkle_root=$(echo "$DATA" | cut -d',' -f5)
credential_type=$(echo "$DATA" | cut -d',' -f6)

echo "Testing with user data:"
echo "  encrypted_push_id: $encrypted_push_id"
echo "  timestamp: $timestamp"
echo "  credential_type: $credential_type"
echo ""
echo "Sending request to https://chat-staging.toolsforhumanity.com/v1/authorize"
echo ""

# Make the request with verbose output
curl -v -X POST \
  https://chat-staging.toolsforhumanity.com/v1/authorize \
  -H "Content-Type: application/json" \
  -d "{
    \"encrypted_push_id\": \"$encrypted_push_id\",
    \"timestamp\": $timestamp,
    \"proof\": \"$proof\",
    \"nullifier_hash\": \"$nullifier_hash\",
    \"merkle_root\": \"$merkle_root\",
    \"credential_type\": \"$credential_type\"
  }" 

echo ""
echo "If you see a 422 error, check the response body above for details."
