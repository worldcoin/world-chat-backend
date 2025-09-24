mod attestation;
mod docs;
mod health;
mod push_id_challenge;

use aide::axum::{
    routing::{get, post},
    ApiRouter,
};

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .api_route("/health", get(health::handler))
        .api_route("/v1/push-id-challenge", post(push_id_challenge::handler))
        .api_route("/v1/attestation-document", get(attestation::handler))
}
