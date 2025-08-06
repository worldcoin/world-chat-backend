use aide::axum::IntoApiResponse;
use axum_jsonschema::Json;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct HealthResponse {
    status: String,
    /// Current version of the application
    semver: String,
    /// Commit hash of the current build (if available)
    rev: Option<String>,
}

/// Health check endpoint
///
/// Returns the current status and version information of the service.
/// This endpoint can be used for monitoring and deployment verification.
pub async fn handler() -> impl IntoApiResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        semver: env!("CARGO_PKG_VERSION").to_string(),
        rev: option_env!("GIT_REV").map(ToString::to_string),
    })
}
