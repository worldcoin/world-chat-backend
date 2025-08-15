//! JWT-related error types

use thiserror::Error;

/// Errors that can occur during JWT operations
#[derive(Error, Debug)]
pub enum JwtError {
    /// JWT encoding failed
    #[error("Failed to encode JWT token")]
    EncodingError(#[from] jsonwebtoken::errors::Error),

    /// JWT validation failed
    #[error("Invalid or expired token")]
    ValidationError,

    /// Secret loading failed
    #[error("Failed to load JWT secret: {0}")]
    SecretLoadError(String),
}
