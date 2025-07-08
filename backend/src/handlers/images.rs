use axum::{extract::State, Json};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use validator::Validate;

use crate::{
    bucket::BucketError,
    state::AppState,
    types::{error::AppError, extractors::ValidatedJson},
};

static IMAGE_ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[0-9a-f]{64}$").expect("Invalid regex"));

#[derive(Debug, Deserialize, Validate)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (Blake3 of encrypted blob)
    #[validate(regex(path = "*IMAGE_ID_REGEX", message = "invalid_image_id"))]
    pub image_id: String,
    /// Size in bytes - max 15 MiB
    #[validate(range(min = 1, max = 15_728_640, message = "payload_too_large"))]
    pub content_length: i64,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub presigned_url: String,
    pub expires_at: String, // ISO-8601 UTC
}

#[instrument(skip(app_state, payload))]
pub async fn upload_image(
    State(app_state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<UploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    // Log request details
    tracing::Span::current()
        .record("image_id", &payload.image_id)
        .record("content_length", payload.content_length);
    info!("Received upload request");

    // TODO: Add auth validation when auth is implemented

    // Step 2: De-duplication Probe
    debug!("Checking if object already exists");
    let exists = app_state
        .bucket_client
        .check_object_exists(&payload.image_id)
        .await?;

    if exists {
        return Err(BucketError::ObjectExists(payload.image_id).into());
    }

    debug!("Object does not exist, proceeding with presigned URL generation");

    // Step 3: Generate Presigned URL
    debug!("Generating presigned URL");
    let presigned_url = app_state
        .bucket_client
        .generate_presigned_put_url(&payload.image_id, payload.content_length)
        .await?;

    info!(
        "Successfully generated presigned URL for image_id: {}",
        payload.image_id
    );

    Ok(Json(UploadResponse {
        presigned_url: presigned_url.url,
        expires_at: presigned_url.expires_at.to_rfc3339(),
    }))
}
