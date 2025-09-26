use axum::Extension;
use axum_jsonschema::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclaveAttestationDocRequest;

use crate::cache::CacheManager;
use crate::types::AppError;

const EXPIRATION_TIME: u64 = 60 * 60 * 3; // 3 hours
const REFRESH_THRESHOLD: u64 = 60 * 10; // 10 minutes
const CACHE_KEY: &str = "enclave-worker:attestation-document";

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Extension(cache_manager): Extension<CacheManager>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let request = EnclaveAttestationDocRequest {};

    let attestation_doc = cache_manager
        .cache_with_refresh(CACHE_KEY, EXPIRATION_TIME, REFRESH_THRESHOLD, || async {
            pontifex::client::send::<EnclaveAttestationDocRequest>(
                pontifex_connection_details,
                &request,
            )
            .await
            .context("Pontifex error")
            .context("Failed to fetch attestation document")
            .map(|response| response.attestation)
        })
        .await?;

    Ok(Json(AttestationDocumentResponse {
        attestation_doc_base64: STANDARD.encode(attestation_doc),
    }))
}
