pub mod attestation;
pub mod auth;
pub mod group_join_requests;
pub mod media;
pub mod subscriptions;

use aide::axum::{
    routing::{get, post, put},
    ApiRouter,
};
use axum::middleware;

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    let public_routes = ApiRouter::new()
        // TODO: TEMPORARY: Remove this once we finish mobile dev testing
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        .api_route("/attestation-document", get(attestation::handler))
        .api_route("/media/config", get(media::get_media_config))
        .api_route("/authorize", post(auth::authorize_handler));

    let protected_routes = ApiRouter::new()
        .api_route(
            "/subscriptions",
            post(subscriptions::subscribe).delete(subscriptions::unsubscribe),
        )
        .api_route(
            "/group-join-requests",
            post(group_join_requests::create_join_request),
        )
        .api_route(
            "/group-join-requests/{id}",
            get(group_join_requests::get_join_request),
        )
        .api_route(
            "/group-join-requests/{id}/approve",
            put(group_join_requests::approve_join_request),
        )
        .layer(middleware::from_fn(auth_middleware));

    public_routes.merge(protected_routes)
}
