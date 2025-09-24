use axum::Extension;
use axum_jsonschema::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common_types::AttestationDocumentResponse;
use enclave_types::EnclavePublicKeyRequest;

use crate::types::AppError;

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
) -> Result<Json<AttestationDocumentResponse>, AppError> {
    let request = EnclavePublicKeyRequest {};

    let response =
        pontifex::client::send::<EnclavePublicKeyRequest>(pontifex_connection_details, &request)
            .await??;

    Ok(Json(AttestationDocumentResponse {
        attestation: STANDARD.encode(response.attestation),
    }))
}
