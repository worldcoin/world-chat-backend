//! Error types for push notification storage operations

use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, get_item::GetItemError, put_item::PutItemError, query::QueryError,
};
use thiserror::Error;

/// Result type for auth proof storage operations
pub type AuthProofStorageResult<T> = Result<T, AuthProofStorageError>;

/// Errors that can occur during auth proof storage operations
#[derive(Error, Debug)]
pub enum AuthProofStorageError {
    /// Failed to insert auth proof into Dynamo DB
    #[error("Failed to insert auth proof into DynamoDB: {0}")]
    DynamoDbPutError(#[from] SdkError<PutItemError>),

    /// Failed to delete auth proof from Dynamo DB
    #[error("Failed to delete auth proof from DynamoDB: {0}")]
    DynamoDbDeleteError(#[from] SdkError<DeleteItemError>),

    /// Failed to get auth proof from Dynamo DB
    #[error("Failed to get auth proof from DynamoDB: {0}")]
    DynamoDbGetError(#[from] SdkError<GetItemError>),

    /// Failed to query auth proofs from Dynamo DB
    #[error("Failed to query auth proofs from DynamoDB: {0}")]
    DynamoDbQueryError(#[from] SdkError<QueryError>),

    /// Failed to update auth proof in Dynamo DB
    #[error("Failed to update auth proof in DynamoDB: {0}")]
    DynamoDbUpdateError(#[from] SdkError<UpdateItemError>),

    /// Auth proof already exists
    #[error("Auth proof already exists")]
    AuthProofExists,

    /// Serialization error for `serde_dynamo`
    #[error("Serialization error: {0}")]
    SerializationError(String),
}
