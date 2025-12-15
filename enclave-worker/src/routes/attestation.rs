use anyhow::Context;
use attestation_verifier::extract_certificate_validity;
use axum::{Extension, Json};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclaveAttestationDocRequest;
use tracing::{info, warn};

use crate::cache::CacheManager;
use crate::types::AppError;

const MAX_TTL_SECS: u64 = 60 * 2; // 2 minutes
const REFRESH_THRESHOLD_SECS: u64 = 20; // 20 seconds before expiration
const CACHE_KEY: &str = "enclave-worker:attestation-document";

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Extension(cache_manager): Extension<CacheManager>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let attestation_doc = cache_manager
        .cache_with_refresh(
            CACHE_KEY,
            MAX_TTL_SECS,
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

                // Log certificate validity for debugging
                match extract_certificate_validity(&response.attestation) {
                    Ok(validity) => {
                        info!(
                            cert_not_before = validity.not_before_secs,
                            cert_not_after = validity.not_after_secs,
                            remaining_validity_secs = validity.remaining_validity_secs(),
                            "Refreshed attestation document"
                        );
                    }
                    Err(e) => {
                        warn!("Failed to extract certificate validity: {e:?}");
                    }
                }

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
