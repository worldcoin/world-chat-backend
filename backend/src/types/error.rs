//! Universal error handling for the API

use aide::OperationOutput;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::media_storage::BucketError;

/// API error response envelope that matches mobile client expectations
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorResponse {
    /// Whether the client should retry the request
    pub allow_retry: bool,
    /// Error details
    pub error: ErrorBody,
}

/// Error body containing code and message
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Machine-readable error code
    pub code: &'static str,
    /// Human-readable error message
    pub message: &'static str,
}

/// Application error type that wraps the API error response
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    inner: ApiErrorResponse,
}

impl AppError {
    /// Create a new application error
    #[must_use]
    pub const fn new(
        status: StatusCode,
        code: &'static str,
        msg: &'static str,
        retry: bool,
    ) -> Self {
        Self {
            status,
            inner: ApiErrorResponse {
                allow_retry: retry,
                error: ErrorBody { code, message: msg },
            },
        }
    }

    /// Create a validation error from field and code string
    #[must_use]
    pub fn validation_from_str(_field: &str, code: &str) -> Self {
        let (status, error_code, message) = match code {
            "invalid_image_id" => (
                StatusCode::BAD_REQUEST,
                "invalid_image_id",
                "Image ID must be a 64-character hexadecimal string",
            ),
            "payload_too_large" => (
                StatusCode::BAD_REQUEST,
                "payload_too_large",
                "Content length exceeds maximum allowed size",
            ),
            _ => (
                StatusCode::BAD_REQUEST,
                "validation_error",
                "Invalid request data",
            ),
        };

        Self::new(status, error_code, message, false)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Log the error based on status code
        match self.status.as_u16() {
            400..=499 => tracing::warn!(
                "Client error: {} - {}",
                self.inner.error.code,
                self.inner.error.message
            ),
            500..=599 => tracing::error!(
                "Server error: {} - {}",
                self.inner.error.code,
                self.inner.error.message
            ),
            _ => {}
        }

        (self.status, Json(self.inner)).into_response()
    }
}

/// Convert bucket errors to application errors
impl From<BucketError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: BucketError) -> Self {
        use BucketError::{AwsError, ConfigError, ObjectExists, S3Error, UpstreamError};

        match &err {
            ObjectExists(id) => {
                tracing::info!("Object already exists: {id}");
                Self::new(
                    StatusCode::CONFLICT,
                    "already_exists",
                    "Image with this ID already exists",
                    false,
                )
            }
            UpstreamError(msg) => {
                tracing::error!("S3 upstream error: {msg}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "upstream_error",
                    "S3 service temporarily unavailable",
                    true,
                )
            }
            S3Error(msg) | AwsError(msg) => {
                tracing::error!("S3/AWS error: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    true,
                )
            }
            ConfigError(msg) => {
                tracing::error!("Configuration error: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
        }
    }
}

impl OperationOutput for AppError {
    type Inner = ApiErrorResponse;

    fn operation_response(
        ctx: &mut aide::generate::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        Json::<ApiErrorResponse>::operation_response(ctx, operation)
    }
}
