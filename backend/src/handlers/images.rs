use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};

use crate::{
    bucket::BucketError,
    state::AppState,
};

const MAX_UPLOAD_BYTES: i64 = 15 * 1024 * 1024; // 15 MiB

#[derive(Debug, Deserialize)]
pub struct UploadRequest {
    /// 64-character lowercase hex string (Blake3 of encrypted blob)
    pub image_id: String,
    /// Size in bytes - max 15 MiB
    pub content_length: i64,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub presigned_url: String,
    pub expires_at: String, // ISO-8601 UTC
}

#[derive(Debug, Serialize)]
pub struct ValidationErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct AlreadyExistsResponse {
    pub error: String,
    pub message: String,
    pub image_id: String,
}

#[instrument(skip(app_state), fields(image_id = %payload.image_id, content_length = %payload.content_length))]
pub async fn upload_image(
    State(app_state): State<AppState>,
    Json(payload): Json<UploadRequest>,
) -> impl IntoResponse {
    info!("Received upload request");

    // Step 1: Request Validation
    if !validate_image_id(&payload.image_id) {
        debug!("Invalid image_id format");
        return (
            StatusCode::BAD_REQUEST,
            Json(ValidationErrorResponse {
                error: "invalid_image_id".to_string(),
                message: "Image ID must be a 64-character lowercase hexadecimal string".to_string(),
            }),
        )
            .into_response();
    }

    if !validate_content_length(payload.content_length) {
        debug!("Invalid content_length: {}", payload.content_length);
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ValidationErrorResponse {
                error: "payload_too_large".to_string(),
                message: format!(
                    "Content length {} exceeds maximum of {} bytes",
                    payload.content_length, MAX_UPLOAD_BYTES
                ),
            }),
        )
            .into_response();
    }

    // TODO: Add auth validation when auth is implemented

    // Step 2: De-duplication Probe
    debug!("Checking if object already exists");
    match app_state.bucket_client.check_object_exists(&payload.image_id).await {
        Ok(true) => {
            info!("Object already exists: {}", payload.image_id);
            return (
                StatusCode::CONFLICT,
                Json(AlreadyExistsResponse {
                    error: "already_exists".to_string(),
                    message: "Image with this ID already exists".to_string(),
                    image_id: payload.image_id,
                }),
            )
                .into_response();
        }
        Ok(false) => {
            debug!("Object does not exist, proceeding with presigned URL generation");
        }
        Err(BucketError::UpstreamError(msg)) => {
            error!("Upstream error during deduplication check: {}", msg);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "upstream_error".to_string(),
                    message: "S3 service temporarily unavailable".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            error!("Error checking object existence: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to check object existence".to_string(),
                }),
            )
                .into_response();
        }
    }

    // Step 3: Generate Presigned URL
    debug!("Generating presigned URL");
    match app_state.bucket_client
        .generate_presigned_put_url(&payload.image_id, payload.content_length)
        .await
    {
        Ok(presigned_url) => {
            info!("Successfully generated presigned URL for image_id: {}", payload.image_id);
            Json(UploadResponse {
                presigned_url: presigned_url.url,
                expires_at: presigned_url.expires_at.to_rfc3339(),
            })
            .into_response()
        }
        Err(e) => {
            error!("Failed to generate presigned URL: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to generate presigned URL".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// Helper function to validate image_id format
fn validate_image_id(image_id: &str) -> bool {
    image_id.len() == 64
        && image_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && c.is_lowercase())
}

// Helper function to validate content length
fn validate_content_length(content_length: i64) -> bool {
    content_length > 0 && content_length <= MAX_UPLOAD_BYTES
}