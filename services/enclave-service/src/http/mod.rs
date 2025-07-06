mod handlers;

use axum::{routing::get, Router};

pub fn routes() -> Router {
    Router::new().route("/hello", get(handlers::hello))
}
