//! Enclave service for World Chat
//!
//! This service provides secure enclave operations for sensitive data processing.

/// HTTP handlers module
pub mod http;

use axum::Router;

/// Builds the HTTP router for the enclave service
pub async fn build_http_router() -> Router {
    http::routes()
}
