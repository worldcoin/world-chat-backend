use axum::Extension;
use axum_jsonschema::Json;
use common_types::{PushIdChallengeRequest, PushIdChallengeResponse};
use enclave_types::EnclavePushIdChallengeRequest;

use crate::types::AppError;

pub async fn handler(
    Extension(pontifex_connection_details): Extension<pontifex::client::ConnectionDetails>,
    Json(payload): Json<PushIdChallengeRequest>,
) -> Result<Json<PushIdChallengeResponse>, AppError> {
    let pontifex_request = EnclavePushIdChallengeRequest {
        encrypted_push_id_1: payload.encrypted_push_id_1,
        encrypted_push_id_2: payload.encrypted_push_id_2,
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
