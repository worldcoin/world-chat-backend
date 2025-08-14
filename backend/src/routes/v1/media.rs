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
    #[schemars(range(min = 1, max = 15_728_640))]
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

/// Creates a presigned URL for uploading media content to S3
///
/// This function implements a secure media upload workflow with deduplication:
/// 1. Maps the SHA-256 content digest to an S3 key
/// 2. Checks if the object already exists in S3 (deduplication)
/// 3. Generates a presigned PUT URL for the upload if object doesn't exist
///
/// # Arguments
///
/// * `media_storage` - The media storage service instance
/// * `payload` - Upload request containing content digest and length
///
/// # Returns
///
/// Returns `Ok(Json<UploadResponse>)` containing:
/// - `asset_id`: S3 key for the uploaded asset
/// - `presigned_url`: Temporary URL for uploading the content
/// - `expires_at`: ISO-8601 timestamp when the URL expires
///
/// # Errors
///
/// This function can return the following errors:
/// - `BucketError::ObjectExists` - Object with the same SHA-256 already exists in S3
/// - `BucketError::S3Error` - S3 service error during object existence check or presigned URL generation
/// - `BucketError::UpstreamError` - 5xx errors from S3 service during object existence check
/// - `BucketError::ConfigError` - Failed to create presigning configuration
/// - `BucketError::InvalidInput` - Invalid SHA-256 format (not 64-character hex string)
#[instrument(skip(media_storage, payload))]
pub async fn create_presigned_upload_url(
    Extension(media_storage): Extension<Arc<MediaStorage>>,
    Json(payload): Json<UploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    let s3_key = MediaStorage::map_sha256_to_s3_key(&payload.content_digest_sha256);

    tracing::info!("s3_key: {}", s3_key);

    // Step 2: De-duplication Probe
    let exists = media_storage.check_object_exists(&s3_key).await?;

    tracing::info!("exists: {}", exists);

    if exists {
        return Err(BucketError::ObjectExists(payload.content_digest_sha256).into());
    }

    // Step 3: Generate Presigned URL
    let presigned_url = media_storage
        .generate_presigned_put_url(&payload.content_digest_sha256, payload.content_length)
        .await?;

    tracing::info!("presigned_url: {}", presigned_url.url);

    Ok(Json(UploadResponse {
        asset_id: s3_key,
        presigned_url: presigned_url.url,
        expires_at: presigned_url.expires_at.to_rfc3339(),
    }))
}
