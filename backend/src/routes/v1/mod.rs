pub mod attestation;
pub mod auth;
pub mod config;
pub mod media;
pub mod subscriptions;

use aide::axum::{
    routing::{get, post},
    ApiRouter,
};
use axum::middleware;

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    let public_routes = ApiRouter::new()
        .api_route("/attestation-document", get(attestation::handler))
        .api_route("/authorize", post(auth::authorize_handler))
        .api_route("/config", get(config::get_config));

    let protected_routes = ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        // TODO: This endpoint is deprecated, replaced by /config
        .api_route("/media/config", get(media::get_media_config))
        .api_route(
            "/subscriptions",
            post(subscriptions::subscribe).delete(subscriptions::unsubscribe),
        )
        .layer(middleware::from_fn(auth_middleware));

    public_routes.merge(protected_routes)
}
