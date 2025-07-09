mod docs;
pub mod v1;

use aide::axum::ApiRouter;

/// Creates the router with all handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .merge(docs::handler())
        .nest("/v1", v1::handler())
}
