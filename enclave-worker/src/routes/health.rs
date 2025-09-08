use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use enclave_types::HealthCheckRequest;
use schemars::JsonSchema;
use serde::Serialize;

use crate::{pontifex_client::PontifexClient, types::AppError};

#[derive(Debug, Serialize, JsonSchema)]
pub struct HealthResponse {
    status: String,
    /// Current version of the application
    semver: String,
    /// Commit hash of the current build (if available)
    rev: Option<String>,
    /// Whether the enclave is initialized
    enclave_initialized: bool,
}

/// Health check endpoint
///
/// Returns the current status and version information of the service.
/// This endpoint can be used for monitoring and deployment verification.
pub async fn handler(
    Extension(pontifex_client): Extension<Arc<PontifexClient>>,
) -> Result<Json<HealthResponse>, AppError> {
    // Now directly returns a bool (initialized status)
    let initialized = pontifex_client
        .send(HealthCheckRequest)
        .await?;

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        semver: env!("CARGO_PKG_VERSION").to_string(),
        rev: option_env!("GIT_REV").map(ToString::to_string),
        enclave_initialized: initialized,
    }))
}
