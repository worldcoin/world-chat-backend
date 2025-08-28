pub mod auth;
pub mod media;

use aide::axum::{
    routing::{get, post},
    ApiRouter,
};

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        .api_route("/media/config", get(media::get_media_config))
        .api_route("/authorize", post(auth::authorize_handler))
}
