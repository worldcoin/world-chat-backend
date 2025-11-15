use std::sync::Arc;

use aide::OperationIo;
use axum::Json;
use axum::{http::StatusCode, response::IntoResponse, Extension};
use axum_valid::Valid;
use mime::Mime;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::LazyLock;
use validator::Validate;

use crate::{
    media_storage::MediaStorage,
    types::{AppError, Environment},
};

/// 5 MB Image size limit
pub const MAX_IMAGE_SIZE_BYTES: i64 = 5 * 1024 * 1024;
/// 15 MB Video size limit
pub const MAX_VIDEO_SIZE_BYTES: i64 = 15 * 1024 * 1024;
/// Maximum count of assets per message
pub const MAX_ASSETS_PER_MESSAGE: usize = 10;
/// Regex for lowercase SHA-256 digest
static DIGEST_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-f0-9]{64}$").unwrap());

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (SHA-256 of encrypted blob)
    #[validate(regex(path = *DIGEST_REGEX))]
    pub content_digest_sha256: String,
    /// Size in bytes - max 15 MiB
    #[validate(range(min = 1, max = 15_728_640))]
    pub content_length: i64,
    /// Only Image and Video MIME types are allowed
    #[serde(deserialize_with = "deserialize_allowed_mime")]
    #[schemars(with = "String", description = "Mime type must be image/* or video/*")]
    pub content_type: Mime,
}

fn deserialize_allowed_mime<'de, D>(d: D) -> Result<Mime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    let m: Mime = s.parse().map_err(serde::de::Error::custom)?;
    if matches!(m.type_(), mime::IMAGE | mime::VIDEO) {
        Ok(m)
    } else {
        Err(serde::de::Error::custom("mime must be image/* or video/*"))
    }
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct SuccessResponse {
    /// Presigned URL to upload the asset to S3
    pub presigned_url: String,
    /// Base64-encoded SHA-256 content digest
    ///
    /// Used in the `x-amz-checksum-sha256` header of the presigned URL
    ///
    /// Read more about it [here](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity.html)
    pub content_digest_base64: String,
    /// CDN URL of the asset
    ///
    /// Used in XMTP [Remote Attachment message](https://docs.xmtp.org/inboxes/content-types/attachments#send-a-remote-attachment)
    pub asset_url: String,
}

/// Conflict response type
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConflictResponse {
    /// CDN URL of the existing asset
    pub asset_url: String,
}

#[derive(Debug, Serialize, JsonSchema, OperationIo)]
#[serde(untagged)]
pub enum MediaUploadResponse {
    /// Successful response with presigned URL for upload
    Success(SuccessResponse),
    /// Asset already exists, returning existing asset URL
    Conflict(ConflictResponse),
}

impl IntoResponse for MediaUploadResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Success(resp) => (StatusCode::OK, Json(resp)).into_response(),
            Self::Conflict(resp) => (StatusCode::CONFLICT, Json(resp)).into_response(),
        }
    }
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
pub async fn create_presigned_upload_url(
    Extension(media_storage): Extension<Arc<MediaStorage>>,
    Extension(environment): Extension<Environment>,
    Valid(Json(payload)): Valid<Json<UploadRequest>>,
) -> Result<MediaUploadResponse, AppError> {
    let s3_key = MediaStorage::map_sha256_to_s3_key(&payload.content_digest_sha256);
    validate_asset_size(&payload.content_type, payload.content_length)?;

    // Step 2: De-duplication Probe
    let exists = media_storage.check_object_exists(&s3_key).await?;
    if exists {
        let asset_url = format!("{}/{}", environment.cdn_url(), s3_key);
        return Ok(MediaUploadResponse::Conflict(ConflictResponse {
            asset_url,
        }));
    }

    // Step 3: Generate Presigned URL
    let presigned_url = media_storage
        .generate_presigned_put_url(
            &payload.content_digest_sha256,
            payload.content_length,
            payload.content_type.to_string().as_str(),
        )
        .await?;

    let asset_url = format!("{}/{}", environment.cdn_url(), s3_key);
    let content_digest_base64 = MediaStorage::map_sha256_to_b64(&payload.content_digest_sha256)?;

    Ok(MediaUploadResponse::Success(SuccessResponse {
        presigned_url: presigned_url.url,
        asset_url,
        content_digest_base64,
    }))
}

fn validate_asset_size(content_type: &Mime, content_length: i64) -> Result<(), AppError> {
    match content_type.type_() {
        mime::VIDEO if content_length > MAX_VIDEO_SIZE_BYTES => Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_asset_size",
            "Video asset size is too large",
            false,
        )),
        mime::IMAGE if content_length > MAX_IMAGE_SIZE_BYTES => Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_asset_size",
            "Image asset size is too large",
            false,
        )),
        _ => Ok(()),
    }
}

#[derive(Serialize, JsonSchema)]
pub struct MediaConfigResponse {
    /// Maximum count of assets per message
    max_assets_per_message: usize,
    /// Maximum image size in bytes
    max_image_size_bytes: i64,
    /// Maximum video size in bytes
    max_video_size_bytes: i64,
    /// Trusted CDN URL
    trusted_cdn_url: String,
}

pub async fn get_media_config(
    Extension(environment): Extension<Environment>,
) -> Json<MediaConfigResponse> {
    Json(MediaConfigResponse {
        max_assets_per_message: MAX_ASSETS_PER_MESSAGE,
        max_image_size_bytes: MAX_IMAGE_SIZE_BYTES,
        max_video_size_bytes: MAX_VIDEO_SIZE_BYTES,
        trusted_cdn_url: environment.cdn_url(),
    })
}
