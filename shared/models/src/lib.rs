//! Shared models for World Chat services
//!
//! This crate contains common data structures used across all services.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// The response data if successful
    pub data: Option<T>,
    /// Error message if the request failed
    pub error: Option<String>,
}

/// Common API errors used across services
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Internal server error")]
    InternalError,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found")]
    NotFound,
}
