mod docs;
mod health;

use aide::axum::{routing::get, ApiRouter};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/health", get(health::handler))
}
