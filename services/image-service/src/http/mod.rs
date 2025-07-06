mod handlers;

use axum::{routing::get, Router};

/// Creates the routes for this service
pub fn routes() -> Router {
    Router::new().route("/hello", get(handlers::hello))
}
