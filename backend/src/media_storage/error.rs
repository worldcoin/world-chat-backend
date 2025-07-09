//! Error types for bucket operations

use aws_sdk_s3::{
    error::SdkError,
    operation::{head_object::HeadObjectError, put_object::PutObjectError},
};
use thiserror::Error;

/// Result type for bucket operations
pub type BucketResult<T> = Result<T, BucketError>;

/// Errors that can occur during bucket operations
#[derive(Error, Debug)]
pub enum BucketError {
    /// S3 service error
    #[error("S3 service error: {0}")]
    S3Error(String),

    /// Object already exists in bucket
    #[error("Object already exists: {0}")]
    ObjectExists(String),

    /// AWS SDK error
    #[error("AWS SDK error: {0}")]
    AwsError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Upstream service error (5xx from S3)
    #[error("Upstream service error: {0}")]
    UpstreamError(String),

    /// Invalid input provided
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl From<aws_sdk_s3::Error> for BucketError {
    fn from(error: aws_sdk_s3::Error) -> Self {
        Self::S3Error(error.to_string())
    }
}

impl From<SdkError<HeadObjectError>> for BucketError {
    fn from(error: SdkError<HeadObjectError>) -> Self {
        match error {
            SdkError::ServiceError(err) => match err.err() {
                HeadObjectError::NotFound(_) => {
                    // Not found is expected for deduplication check
                    Self::S3Error("Object not found".to_string())
                }
                _ => Self::S3Error(format!("{:?}", err.err())),
            },
            _ => Self::AwsError(error.to_string()),
        }
    }
}

impl From<SdkError<PutObjectError>> for BucketError {
    fn from(error: SdkError<PutObjectError>) -> Self {
        Self::S3Error(error.to_string())
    }
}
