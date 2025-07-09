mod docs;
pub mod media;
use aide::axum::{routing::post, ApiRouter};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new().merge(docs::handler()).api_route(
        "/v1/media/presigned-urls",
        post(media::create_presigned_upload_url),
    )
}
