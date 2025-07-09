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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (SHA-256 of encrypted blob)
    #[schemars(length(equal = 64), regex(pattern = r"^[a-f0-9]{64}$"))]
    pub content_digest_sha256: String,
    /// Size in bytes - max 15 MiB
    #[schemars(range(min = 1, max = 15728640))]
    pub content_length: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UploadResponse {
    /// S3 key of the asset, used in XMTP media message
    pub asset_id: String,
    /// Presigned URL to upload the asset to S3
    pub presigned_url: String,
    /// ISO-8601 UTC timestamp when the presigned URL expires
    pub expires_at: String,
}

#[instrument(skip(media_storage, payload))]
pub async fn create_presigned_upload_url(
    Extension(media_storage): Extension<Arc<MediaStorage>>,
    Json(payload): Json<UploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    // TODO: Step 1:Add auth validation when auth is implemented
    let s3_key = MediaStorage::map_sha256_to_s3_key(&payload.content_digest_sha256);

    // Step 2: De-duplication Probe
    let exists = media_storage.check_object_exists(&s3_key).await?;

    if exists {
        // TODO: don't map to bucket error here, instead an app error maybe (?)
        return Err(BucketError::ObjectExists(payload.content_digest_sha256).into());
    }

    // Step 3: Generate Presigned URL
    let presigned_url = media_storage
        .generate_presigned_put_url(&payload.content_digest_sha256, payload.content_length)
        .await?;

    Ok(Json(UploadResponse {
        asset_id: s3_key,
        presigned_url: presigned_url.url,
        expires_at: presigned_url.expires_at.to_rfc3339(),
    }))
}
