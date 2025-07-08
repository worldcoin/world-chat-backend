use std::sync::Arc;

use axum::{Extension, Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use validator::Validate;

use crate::{
    image_storage::{BucketError, ImageStorage},
    types::{extractors::ValidatedJson, AppError},
};

fn validate_image_id(image_id: &str) -> Result<(), validator::ValidationError> {
    let re = regex::Regex::new(r"^[0-9a-f]{64}$").expect("Invalid regex");
    if re.is_match(image_id) {
        Ok(())
    } else {
        Err(validator::ValidationError::new("invalid_image_id"))
    }
}

#[derive(Debug, Deserialize, Validate, JsonSchema)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (Blake3 of encrypted blob)
    #[validate(custom(function = "validate_image_id"))]
    pub image_id: String,
    /// Size in bytes - max 15 MiB
    #[validate(range(min = 1, max = 15_728_640, message = "payload_too_large"))]
    pub content_length: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UploadResponse {
    pub presigned_url: String,
    pub expires_at: String, // ISO-8601 UTC
}

#[instrument(skip(image_storage, payload))]
pub async fn upload_image(
    Extension(image_storage): Extension<Arc<ImageStorage>>,
    ValidatedJson(payload): ValidatedJson<UploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    // TODO: Step 1:Add auth validation when auth is implemented

    // Step 2: De-duplication Probe
    let exists = image_storage.check_object_exists(&payload.image_id).await?;

    if exists {
        return Err(BucketError::ObjectExists(payload.image_id).into());
    }

    // Step 3: Generate Presigned URL
    let presigned_url = image_storage
        .generate_presigned_put_url(&payload.image_id, payload.content_length)
        .await?;

    Ok(Json(UploadResponse {
        presigned_url: presigned_url.url,
        expires_at: presigned_url.expires_at.to_rfc3339(),
    }))
}
