//! JWT-related error types for our hand-rolled JWS (ES256) implementation.
//!
//! These errors intentionally avoid dependencies on JWT libraries so that the
//! rest of the codebase deals with a small, well-defined set of cases.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JwtError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Signing input build error: {0}")]
    SigningInput(String),

    #[error("AWS KMS error: {0}")]
    Kms(#[from] Box<aws_sdk_kms::Error>),

    #[error("Other: {0}")]
    Other(#[from] anyhow::Error),
}
