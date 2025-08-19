//! JWT-related error types

use thiserror::Error;

/// Errors that can occur during JWT operations
#[derive(Error, Debug)]
pub enum JwtError {
    /// JWT validation failed (signature invalid or claims rejected)
    #[error("Invalid or expired token")]
    ValidationError,

    /// Failed to sign with KMS
    #[error("KMS signing failed: {0}")]
    SigningError(String),

    /// Failed to fetch or parse the public key
    #[error("Failed to load public key: {0}")]
    PublicKeyLoadError(String),

    /// Malformed or unexpected header
    #[error("Invalid JWT header: {0}")]
    HeaderError(String),
}
