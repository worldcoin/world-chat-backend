//! Enclave service for secure operations

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

/// HTTP handlers module
pub mod http;

use axum::Router;

/// Builds the HTTP router for the enclave service
pub async fn build_http_router() -> Router {
    http::routes()
}
