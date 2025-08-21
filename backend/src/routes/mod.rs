mod docs;
mod health;
pub mod v1;

use aide::axum::{routing::get, ApiRouter};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/health", get(health::handler))
        .nest("/v1", v1::handler())
}
