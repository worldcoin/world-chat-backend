use crate::state::AppState;
use axum::{routing::post, Router};

mod images;

/// Creates the router with all handler routes
pub fn routes() -> Router<AppState> {
    Router::new().route("/v1/images/upload", post(images::upload_image))
}
