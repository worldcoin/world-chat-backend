//! Notification service for World Chat
//!
//! This service handles sending notifications to users.

/// HTTP handlers module
pub mod http;

use axum::Router;

/// Builds the HTTP router for the notification service
pub async fn build_http_router() -> Router {
    http::routes()
}
