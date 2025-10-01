use axum::Extension;
use axum_jsonschema::Json;
use enclave_types::EnclaveHealthCheckRequest;
use schemars::JsonSchema;
use serde::Serialize;

use crate::types::AppError;

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
pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
) -> Result<Json<HealthResponse>, AppError> {
    // Verify we can reach the enclave and it's healthy
    pontifex::client::send::<EnclaveHealthCheckRequest>(
        pontifex_connection_details,
        &EnclaveHealthCheckRequest,
    )
    .await??;

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        semver: env!("CARGO_PKG_VERSION").to_string(),
        rev: option_env!("GIT_REV").map(ToString::to_string),
    }))
}
