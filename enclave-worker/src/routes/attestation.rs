use anyhow::Context;
use attestation_verifier::EnclaveAttestationVerifier;
use axum::{Extension, Json};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclaveAttestationDocRequest;
use tracing::{error, info};

use crate::cache::CacheManager;
use crate::types::AppError;

const MAX_TTL_SECS: u64 = 60 * 60 * 3; // 3 hours
const CACHE_KEY: &str = "enclave-worker:attestation-document";

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Extension(cache_manager): Extension<CacheManager>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let attestation_doc = cache_manager
        .cache_with_refresh(
            CACHE_KEY,
            MAX_TTL_SECS,
            move || async move {
                let attestation_document =
                    fetch_attestation_document(pontifex_connection_details.clone()).await?;

                info!(attestation = %STANDARD.encode(attestation_document.clone()), "Refreshed attestation document");

                Ok(attestation_document)
            },
        )
        .await
        .map_err(|e| {
            error!("Failed to get attestation document: {e:?}");
            AppError::internal_server_error()
        })?;

    // If the attestation document fails verification
    // - Fetch a fresh one and update cache
    let attestation_verifier = EnclaveAttestationVerifier::new(vec![]);
    if let Err(e) =
        attestation_verifier.verify_attestation_document_without_pcr_check(&attestation_doc)
    {
        error!("Attestation document verification failed: {e:?}");

        let attestation_doc = fetch_attestation_document(pontifex_connection_details.clone())
            .await
            .map_err(|e| {
                error!("Failed to get attestation document: {e:?}");
                AppError::internal_server_error()
            })?;
        cache_manager
            .set_with_ttl_safely(CACHE_KEY, &attestation_doc, MAX_TTL_SECS)
            .await;

        info!(attestation = %STANDARD.encode(attestation_doc.clone()), "Refreshed attestation document after failed verification");

        return Ok(Json(AttestationDocumentResponse {
            attestation_doc_base64: STANDARD.encode(attestation_doc),
        }));
    } else {
        return Ok(Json(AttestationDocumentResponse {
            attestation_doc_base64: STANDARD.encode(attestation_doc),
        }));
    }
}

async fn fetch_attestation_document(
    pontifex_connection_details: pontifex::client::ConnectionDetails,
) -> anyhow::Result<Vec<u8>> {
    let request: EnclaveAttestationDocRequest = EnclaveAttestationDocRequest {};
    let response = pontifex::client::send::<EnclaveAttestationDocRequest>(
        pontifex_connection_details,
        &request,
    )
    .await
    .context("Pontifex error")?
    .context("Failed to fetch attestation document")?;

    Ok(response.attestation)
}
