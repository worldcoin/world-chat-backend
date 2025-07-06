//! Image service for World Chat
//!
//! This service handles image upload, processing, and storage.

/// HTTP handlers module
pub mod http;

use axum::Router;

/// Builds the HTTP router for the image service
pub async fn build_http_router() -> Router {
    http::routes()
}
