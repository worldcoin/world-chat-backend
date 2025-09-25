use axum::Extension;
use axum_jsonschema::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclaveAttestationDocRequest;

use crate::types::AppError;

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let request = EnclaveAttestationDocRequest {};

    let response = pontifex::client::send::<EnclaveAttestationDocRequest>(
        pontifex_connection_details,
        &request,
    )
    .await??;

    Ok(Json(AttestationDocumentResponse {
        attestation_doc_base64: STANDARD.encode(response.attestation),
    }))
}
