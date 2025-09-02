pub mod auth;
pub mod media;
pub mod notifications;

use aide::axum::{
    routing::{get, post},
    ApiRouter,
};
use axum::middleware;

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    // Public routes (no auth required)
    let public_routes = ApiRouter::new().api_route("/authorize", post(auth::authorize_handler));

    // Protected routes (auth required) - use regular axum routing for middleware compatibility
    let protected_routes = ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        .api_route("/media/config", get(media::get_media_config))
        .api_route(
            "/notifications",
            post(notifications::subscribe).delete(notifications::unsubscribe),
        )
        .layer(middleware::from_fn(auth_middleware));

    // Combine public and protected routes
    public_routes.merge(protected_routes)
}
