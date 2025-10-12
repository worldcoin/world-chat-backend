use std::sync::Arc;

use axum::{Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{enclave_worker_api::EnclaveWorkerApi, types::AppError};

#[derive(Debug, Serialize, JsonSchema)]
pub struct AttestationDocumentResponse {
    /// Base-64 encoded attestation document.
    pub attestation_doc_base64: String,
}

/// Get the attestation document from the enclave
///
/// # Errors
///
/// If the enclave worker API returns an error, it will be returned.
pub async fn handler(
    Extension(enclave_worker_api): Extension<Arc<dyn EnclaveWorkerApi>>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let response = enclave_worker_api.get_attestation_document().await?;

    Ok(Json(AttestationDocumentResponse {
        attestation_doc_base64: response.attestation_doc_base64,
    }))
}
