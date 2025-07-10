use crate::types::Environment;
use aide::{axum::ApiRouter, openapi::OpenApi, scalar::Scalar};
use axum::http::StatusCode;
use axum::{response::IntoResponse, routing::get, Extension, Json};

pub fn handler() -> ApiRouter {
    let scalar = Scalar::new("/openapi.json").with_title("World Chat Backend Docs");

    ApiRouter::new()
        .route("/docs", scalar.axum_route())
        .route("/openapi.json", get(openapi_schema))
}

#[allow(clippy::unused_async)]
async fn openapi_schema(
    Extension(environment): Extension<Environment>,
    Extension(openapi): Extension<OpenApi>,
) -> impl IntoResponse {
    if !environment.show_api_docs() {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(openapi).into_response()
}
