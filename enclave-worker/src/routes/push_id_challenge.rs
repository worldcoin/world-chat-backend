use axum::{Extension, Json};
use common_types::{PushIdChallengeRequest, PushIdChallengeResponse};
use enclave_types::EnclavePushIdChallengeRequest;

use crate::types::AppError;

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Json(payload): Json<PushIdChallengeRequest>,
) -> Result<Json<PushIdChallengeResponse>, AppError> {
    let encrypted_push_id_1 = hex::decode(payload.encrypted_push_id_1).map_err(|e| {
        AppError::bad_request("invalid_encrypted_push_id_1", "Invalid encrypted push ID 1")
    })?;
    let encrypted_push_id_2 = hex::decode(payload.encrypted_push_id_2).map_err(|e| {
        AppError::bad_request("invalid_encrypted_push_id_2", "Invalid encrypted push ID 2")
    })?;

    let pontifex_request = EnclavePushIdChallengeRequest {
        encrypted_push_id_1,
        encrypted_push_id_2,
    };

    let response = pontifex::client::send::<EnclavePushIdChallengeRequest>(
        pontifex_connection_details,
        &pontifex_request,
    )
    .await??;

    Ok(Json(PushIdChallengeResponse {
        push_ids_match: response,
    }))
}
