pub mod auth;
pub mod media;
pub mod notifications;

use aide::axum::{
    routing::{get, post},
    routing::{get, post},
    ApiRouter,
};
use axum::middleware;

use crate::middleware::auth::auth_middleware;
use axum::middleware;

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    let public_routes = ApiRouter::new().api_route("/authorize", post(auth::authorize_handler));

    let protected_routes = ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
            post(media::create_presigned_upload_url),
        )
        .api_route("/media/config", get(media::get_media_config))
        .layer(middleware::from_fn(auth_middleware));

    public_routes.merge(protected_routes)
}
