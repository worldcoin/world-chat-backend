//! JWT-related error types

use josekit::JoseError;
use thiserror::Error;

/// Errors that can occur during JWT operations
#[derive(Error, Debug)]
pub enum JwtError {
    /// JWT validation failed (signature invalid or claims rejected)
    #[error("Invalid or expired token")]
    ValidationError,

    /// Failed to join the blocking task
    #[error("Failed to join the blocking task: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    /// `JOSEKit` error
    #[error("`JOSEKit` error: {0}")]
    JoseKitError(#[from] JoseError),

    /// Aggregate error for JWKS retrieval/construction
    #[error("Failed to retrieve JWKS: {0}")]
    JwksRetrievalError(#[from] anyhow::Error),
}
