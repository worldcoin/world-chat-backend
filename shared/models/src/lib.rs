use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Internal server error")]
    InternalError,
    
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    #[error("Not found")]
    NotFound,
}