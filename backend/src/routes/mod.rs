mod docs;
mod health;
mod jwks;
pub mod v1;

use aide::axum::{routing::get, ApiRouter};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/.well-known/jwks.json", get(jwks::jwks_wellknown))
        .api_route("/health", get(health::handler))
        .nest("/v1", v1::handler())
}
