//! Error types for group join request storage operations

use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::{
    batch_write_item::BatchWriteItemError, delete_item::DeleteItemError, get_item::GetItemError,
    put_item::PutItemError, query::QueryError, scan::ScanError,
};
use thiserror::Error;

/// Result type alias for storage operations
pub type GroupJoinRequestStorageResult<T> = Result<T, GroupJoinRequestStorageError>;

/// Storage error types for group join request operations
#[derive(Debug, Error)]
pub enum GroupJoinRequestStorageError {
    /// Failed to insert group join request into `DynamoDB`
    #[error("Failed to insert group join request into DynamoDB: {0:?}")]
    DynamoDbPutError(#[from] SdkError<PutItemError>),

    /// Failed to get group join request from `DynamoDB`
    #[error("Failed to get group join request from DynamoDB: {0:?}")]
    DynamoDbGetError(#[from] SdkError<GetItemError>),

    /// Failed to query group join requests from `DynamoDB`
    #[error("Failed to query group join requests from DynamoDB: {0:?}")]
    DynamoDbQueryError(#[from] SdkError<QueryError>),

    /// Failed to delete group join request from `DynamoDB`
    #[error("Failed to delete group join request from DynamoDB: {0:?}")]
    DynamoDbDeleteError(#[from] SdkError<DeleteItemError>),

    /// Failed to batch write group join requests to `DynamoDB`
    #[error("Failed to batch write group join requests to DynamoDB: {0:?}")]
    DynamoDbBatchWriteError(#[from] SdkError<BatchWriteItemError>),

    /// Failed to parse group join request from `DynamoDB` item
    #[error("Failed to parse group join request: {0}")]
    SerializationError(String),
}

impl From<serde_dynamo::Error> for GroupJoinRequestStorageError {
    fn from(err: serde_dynamo::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}
