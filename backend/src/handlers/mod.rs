use axum::{routing::get, Router};

mod hello;

/// Creates the router with all handler routes
pub fn routes() -> Router {
    Router::new().route("/v1/hello", get(hello::handler))
}
