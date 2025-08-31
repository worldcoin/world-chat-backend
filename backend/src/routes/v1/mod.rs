pub mod auth;
pub mod media;
pub mod notifications;

use aide::axum::{routing::post, ApiRouter};
use axum::{middleware, routing::post as axum_post};

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    // Public routes (no auth required)
    let public_routes = ApiRouter::new().api_route("/authorize", post(auth::authorize_handler));

    // Protected routes (auth required) - use regular axum routing for middleware compatibility
    let protected_routes = axum::Router::new()
        .route(
            "/media/presigned-urls",
            axum_post(media::create_presigned_upload_url),
        )
        .route(
            "/notifications/subscribe",
            axum_post(notifications::subscribe),
        )
        .route(
            "/notifications/unsubscribe",
            axum_post(notifications::unsubscribe),
        )
        .layer(middleware::from_fn(auth_middleware));

    // Combine public and protected routes
    public_routes.merge(protected_routes)
}
