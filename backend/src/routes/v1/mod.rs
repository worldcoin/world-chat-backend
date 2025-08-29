pub mod auth;
pub mod media;

use aide::axum::{
    routing::{get, post, post_with},
    ApiRouter,
};
use axum_jsonschema::Json;

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post_with(media::create_presigned_upload_url, |op| {
                op.response::<200, Json<media::SuccessResponse>>()
                    .response::<409, Json<media::ConflictResponse>>()
            }),
        )
        .api_route("/media/config", get(media::get_media_config))
        .api_route("/authorize", post(auth::authorize_handler))
}
