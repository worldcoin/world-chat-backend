use anyhow::Context;
use axum::{Extension, Json};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclaveAttestationDocRequest;
use tracing::info;

use crate::cache::CacheManager;
use crate::types::AppError;

const EXPIRATION_TIME_SECS: u64 = 60 * 60 * 3; // 3 hours
const REFRESH_THRESHOLD_SECS: u64 = 60 * 30; // 30 minutes before expiration
const CACHE_KEY: &str = "enclave-worker:attestation-document";

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Extension(cache_manager): Extension<CacheManager>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let attestation_doc = cache_manager
        .cache_with_refresh(
            CACHE_KEY,
            EXPIRATION_TIME_SECS,
            REFRESH_THRESHOLD_SECS,
            move || async move {
                let request = EnclaveAttestationDocRequest {};
                let response = pontifex::client::send::<EnclaveAttestationDocRequest>(
                    pontifex_connection_details,
                    &request,
                )
                .await
                .context("Pontifex error")?
                .context("Failed to fetch attestation document")?;

                info!(attestation = %STANDARD.encode(response.attestation.clone()), "Refreshed attestation document");

                Ok(response.attestation)
            },
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get attestation document: {e:?}");
            AppError::internal_server_error()
        })?;

    Ok(Json(AttestationDocumentResponse {
        attestation_doc_base64: STANDARD.encode(attestation_doc),
    }))
}
