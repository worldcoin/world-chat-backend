#!/bin/bash
# Create S3 bucket for media storage
awslocal s3 mb s3://world-chat-media

# Create KMS key for JWT signing and alias
KMS_KEY_ID=$(awslocal kms create-key --key-usage SIGN_VERIFY --key-spec ECC_NIST_P256 --query 'KeyMetadata.KeyId' --output text)
awslocal kms create-alias --alias-name alias/world-chat-jwt --target-key-id "$KMS_KEY_ID"

# Create DynamoDB table for push subscriptions
awslocal dynamodb create-table \
    --table-name world-chat-push-subscriptions \
    --attribute-definitions \
        AttributeName=topic,AttributeType=S \
        AttributeName=hmac_key,AttributeType=S \
    --key-schema \
        AttributeName=topic,KeyType=HASH \
        AttributeName=hmac_key,KeyType=RANGE \
    --billing-mode PAY_PER_REQUEST

# Enable TTL on the push subscriptions table
awslocal dynamodb update-time-to-live \
    --table-name world-chat-push-subscriptions \
    --time-to-live-specification "Enabled=true,AttributeName=ttl"


# Create DynamoDB table for auth proofs
awslocal dynamodb create-table \
    --table-name world-chat-auth-proofs \
    --attribute-definitions \
        AttributeName=nullifier,AttributeType=S \
    --key-schema \
        AttributeName=nullifier,KeyType=HASH \
    --billing-mode PAY_PER_REQUEST

# Enable TTL on the auth proofs table
awslocal dynamodb update-time-to-live \
    --table-name world-chat-auth-proofs \
    --time-to-live-specification "Enabled=true,AttributeName=ttl"

awslocal sqs create-queue --queue-name notification-queue.fifo --attributes '{
  "FifoQueue": "true",
  "ContentBasedDeduplication": "true",
  "DeduplicationScope": "messageGroup",
  "FifoThroughputLimit": "perMessageGroupId"
}'
awslocal sqs create-queue --queue-name subscription-request-queue.fifo --attributes '{"FifoQueue": "true", "ContentBasedDeduplication": "true"}'

echo "AWS LocalStack resources initialized successfully!"
