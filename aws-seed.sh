#!/bin/bash
# Create S3 bucket for media storage
awslocal s3 mb s3://world-chat-media

# Create JWT secret in Secrets Manager
awslocal secretsmanager create-secret \
    --name world-chat-jwt-secret \
    --secret-string '{"jwt_secret":"SECRET_KEY"}'

# Create DynamoDB table for push subscriptions
awslocal dynamodb create-table \
    --table-name world-chat-push-subscriptions \
    --attribute-definitions \
        AttributeName=hmac,AttributeType=S \
        AttributeName=topic,AttributeType=S \
    --key-schema \
        AttributeName=hmac,KeyType=HASH \
    --global-secondary-indexes \
        'IndexName=topic-index,KeySchema=[{AttributeName=topic,KeyType=HASH}],Projection={ProjectionType=ALL}' \
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

awslocal sqs create-queue --queue-name notification-queue.fifo --attributes '{"FifoQueue": "true", "ContentBasedDeduplication": "true"}'
awslocal sqs create-queue --queue-name subscription-request-queue.fifo --attributes '{"FifoQueue": "true", "ContentBasedDeduplication": "true"}'

echo "AWS LocalStack resources initialized successfully!"
