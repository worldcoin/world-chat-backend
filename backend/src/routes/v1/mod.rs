pub mod attestation;
pub mod auth;
pub mod group_invites;
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
            "/group-invites",
            post(group_invites::create_group_invite).get(group_invites::get_group_invites_by_topic),
        )
        .api_route(
            "/group-invites/:id",
            get(group_invites::get_group_invite).delete(group_invites::delete_group_invite),
        )
        .layer(middleware::from_fn(auth_middleware));

    public_routes.merge(protected_routes)
}
