#!/bin/bash
# Create S3 bucket for image storage
awslocal s3 mb s3://world-chat-images
awslocal s3api put-bucket-versioning --bucket world-chat-images --versioning-configuration Status=Enabled

echo "AWS LocalStack resources initialized successfully!"