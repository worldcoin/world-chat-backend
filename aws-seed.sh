#!/bin/bash
# Create S3 bucket for media storage
awslocal s3 mb s3://world-chat-media

# Create DynamoDB table for push subscriptions
awslocal dynamodb create-table \
    --table-name world-chat-push-subscriptions \
    --attribute-definitions \
        AttributeName=hmac,AttributeType=S \
        AttributeName=topic,AttributeType=S \
    --key-schema \
        AttributeName=hmac,KeyType=HASH \
    --global-secondary-indexes \
        'IndexName=topic-index,Keys=[{AttributeName=topic,KeyType=HASH}],Projection={ProjectionType=ALL},BillingMode=PAY_PER_REQUEST' \
    --billing-mode PAY_PER_REQUEST

# Enable TTL on the push subscriptions table
awslocal dynamodb update-time-to-live \
    --table-name world-chat-push-subscriptions \
    --time-to-live-specification "Enabled=true,AttributeName=ttl"

awslocal sqs create-queue --queue-name world-chat-notification-queue --attributes '{"FifoQueue": "true"}'
awslocal sqs create-queue --queue-name world-chat-subscription-request-queue --attributes '{"FifoQueue": "true"}'

echo "AWS LocalStack resources initialized successfully!"
