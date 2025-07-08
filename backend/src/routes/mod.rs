mod docs;
mod images;
use aide::axum::{routing::post, ApiRouter};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/v1/images/upload", post(images::upload_image))
}
