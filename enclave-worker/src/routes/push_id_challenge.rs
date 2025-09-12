use axum::Extension;
use axum_jsonschema::Json;
use enclave_types::EnclavePushIdChallengeRequest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::AppError;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PushIdChallengeRequest {
    pub encrypted_push_id_1: String,
    pub encrypted_push_id_2: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PushIdChallengeResponse {
    pub is_match: bool,
}

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

    Ok(Json(PushIdChallengeResponse { is_match: response }))
}
