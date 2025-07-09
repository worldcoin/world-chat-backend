use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    media_storage::{BucketError, MediaStorage},
    types::AppError,
};

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (Blake3 of encrypted blob)
    #[schemars(length(equal = 64), regex(pattern = r"^[a-f0-9]{64}$"))]
    pub image_id: String,
    /// Size in bytes - max 15 MiB
    #[schemars(range(min = 1, max = 15728640))]
    pub content_length: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UploadResponse {
    pub presigned_url: String,
    pub expires_at: String, // ISO-8601 UTC
}

#[instrument(skip(media_storage, payload))]
pub async fn create_presigned_upload_url(
    Extension(media_storage): Extension<Arc<MediaStorage>>,
    Json(payload): Json<UploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    // TODO: Step 1:Add auth validation when auth is implemented

    let lower_case_image_id = payload.image_id.to_lowercase();

    // Step 2: De-duplication Probe
    let exists = media_storage
        .check_object_exists(&lower_case_image_id)
        .await?;

    if exists {
        return Err(BucketError::ObjectExists(lower_case_image_id).into());
    }

    // Step 3: Generate Presigned URL
    let presigned_url = media_storage
        .generate_presigned_put_url(&lower_case_image_id, payload.content_length)
        .await?;

    Ok(Json(UploadResponse {
        presigned_url: presigned_url.url,
        expires_at: presigned_url.expires_at.to_rfc3339(),
    }))
}
