//! Error types for group invite storage operations

use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, get_item::GetItemError, put_item::PutItemError, query::QueryError,
};
use thiserror::Error;

/// Result type alias for storage operations
pub type GroupInviteStorageResult<T> = Result<T, GroupInviteStorageError>;

/// Storage error types for group invite operations
#[derive(Debug, Error)]
pub enum GroupInviteStorageError {
    /// Failed to insert group invite into `DynamoDB`
    #[error("Failed to insert group invite into DynamoDB: {0:?}")]
    DynamoDbPutError(#[from] SdkError<PutItemError>),

    /// Failed to get group invite from `DynamoDB`
    #[error("Failed to get group invite from DynamoDB: {0:?}")]
    DynamoDbGetError(#[from] SdkError<GetItemError>),

    /// Failed to query group invites from `DynamoDB`
    #[error("Failed to query group invites from DynamoDB: {0:?}")]
    DynamoDbQueryError(#[from] SdkError<QueryError>),

    /// Failed to delete group invite from `DynamoDB`
    #[error("Failed to delete group invite from DynamoDB: {0:?}")]
    DynamoDbDeleteError(#[from] SdkError<DeleteItemError>),

    /// Failed to parse group invite from `DynamoDB` item
    #[error("Failed to parse group invite: {0}")]
    SerializationError(String),
}

impl From<serde_dynamo::Error> for GroupInviteStorageError {
    fn from(err: serde_dynamo::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}
