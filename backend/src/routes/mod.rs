mod auth;
mod docs;
mod health;
pub mod v1;

use aide::axum::{
    routing::{get, post},
    ApiRouter,
};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/health", get(health::handler))
        .api_route("/authorize", post(auth::authorize_handler))
        .nest("/v1", v1::handler())
}
