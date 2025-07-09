#!/bin/bash
# Create S3 bucket for media storage
awslocal s3 mb s3://world-chat-media

echo "AWS LocalStack resources initialized successfully!"
