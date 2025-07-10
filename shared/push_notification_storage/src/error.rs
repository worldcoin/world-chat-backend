//! Error types for push notification storage operations

use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, get_item::GetItemError, put_item::PutItemError,
    query::QueryError,
};
use thiserror::Error;

/// Result type for push notification storage operations
pub type PushNotificationStorageResult<T> = Result<T, PushNotificationStorageError>;

/// Errors that can occur during push notification storage operations
#[derive(Error, Debug)]
pub enum PushNotificationStorageError {
    /// Failed to insert subscription into DynamoDB
    #[error("Failed to insert subscription into DynamoDB: {0}")]
    DynamoDbPutError(#[from] SdkError<PutItemError>),

    /// Failed to delete subscription from DynamoDB
    #[error("Failed to delete subscription from DynamoDB: {0}")]
    DynamoDbDeleteError(#[from] SdkError<DeleteItemError>),

    /// Failed to get subscription from DynamoDB
    #[error("Failed to get subscription from DynamoDB: {0}")]
    DynamoDbGetError(#[from] SdkError<GetItemError>),

    /// Failed to query subscriptions from DynamoDB
    #[error("Failed to query subscriptions from DynamoDB: {0}")]
    DynamoDbQueryError(#[from] SdkError<QueryError>),

    /// Failed to parse subscription from DynamoDB item
    #[error("Failed to parse subscription: {0}")]
    ParseSubscriptionError(String),

    /// Invalid TTL timestamp
    #[error("Invalid TTL timestamp")]
    InvalidTtlError,
}