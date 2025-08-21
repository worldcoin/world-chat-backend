//! JWT-related error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JwtError {
    #[error("Invalid or malformed token")]
    InvalidToken,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Signing input build error: {0}")]
    SigningInput(String),

    #[error("AWS KMS error: {0}")]
    Kms(#[from] Box<aws_sdk_kms::Error>),

    #[error("Other: {0}")]
    Other(#[from] anyhow::Error),
}
