pub mod attestation;
pub mod auth;
pub mod group_invites;
pub mod media;
pub mod subscriptions;

use aide::axum::{
    routing::{delete, get, post},
    ApiRouter,
};
use axum::middleware;

use crate::middleware::auth::auth_middleware;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    let public_routes = ApiRouter::new()
        // Get group invite by ID is a public URL called by the landing page
        .api_route("/group-invites/{id}", get(group_invites::get_group_invite))
        .api_route("/attestation-document", get(attestation::handler))
        .api_route("/authorize", post(auth::authorize_handler));

    let protected_routes = ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        .api_route("/media/config", get(media::get_media_config))
        .api_route(
            "/subscriptions",
            post(subscriptions::subscribe).delete(subscriptions::unsubscribe),
        )
        .api_route("/group-invites", post(group_invites::create_group_invite))
        .api_route(
            "/group-invites/latest",
            get(group_invites::get_latest_group_invite_by_topic),
        )
        .api_route(
            "/group-invites/{id}",
            delete(group_invites::delete_group_invite),
        )
        .layer(middleware::from_fn(auth_middleware));

    public_routes.merge(protected_routes)
}
