//! Universal error handling for the API

use aide::OperationOutput;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_jsonschema::Json;
use backend_storage::push_subscription::PushSubscriptionStorageError;
use schemars::JsonSchema;
use serde::Serialize;

/// API error response envelope that matches mobile client expectations
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorResponse {
    /// Whether the client should retry the request
    pub allow_retry: bool,
    /// Error details
    error: ErrorBody,
}

/// Error body containing code and message
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
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

    /// Create a new internal server error with no retry
    #[must_use]
    pub const fn internal_server_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            false,
        )
    }

    #[must_use]
    pub const fn bad_request(code: &'static str, msg: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, msg, false)
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

impl OperationOutput for AppError {
    type Inner = ApiErrorResponse;

    fn operation_response(
        ctx: &mut aide::gen::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        Json::<ApiErrorResponse>::operation_response(ctx, operation)
    }
}

impl From<PushSubscriptionStorageError> for AppError {
    fn from(err: PushSubscriptionStorageError) -> Self {
        use PushSubscriptionStorageError::{
            DynamoDbBatchWriteError, DynamoDbDeleteError, DynamoDbGetError, DynamoDbPutError,
            DynamoDbQueryError, DynamoDbUpdateError, ParseSubscriptionError,
            PushSubscriptionExists, SerializationError,
        };

        match &err {
            // This path is not relevant to enclave-worker,
            // but we need to handle it to avoid compile errors
            PushSubscriptionExists => {
                tracing::debug!("Push subscription already exists");
                Self::new(
                    StatusCode::CONFLICT,
                    "already_exists",
                    "Push subscription already exists",
                    false,
                )
            }
            DynamoDbPutError(_)
            | DynamoDbDeleteError(_)
            | DynamoDbGetError(_)
            | DynamoDbQueryError(_)
            | DynamoDbUpdateError(_)
            | DynamoDbBatchWriteError(_) => {
                tracing::error!("DynamoDB error: {err}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "database_error",
                    "Database service temporarily unavailable",
                    true,
                )
            }
            SerializationError(msg) | ParseSubscriptionError(msg) => {
                tracing::error!("Serialization/Parse error: {msg}");
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

impl From<pontifex::client::Error> for AppError {
    fn from(err: pontifex::client::Error) -> Self {
        tracing::error!("Pontifex error: {err:?}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            false,
        )
    }
}

impl From<enclave_types::EnclaveError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: enclave_types::EnclaveError) -> Self {
        use enclave_types::EnclaveError::{
            AttestationFailed, BrazeRequestFailed, DecryptPushIdFailed, KeyPairCreationFailed,
            NotInitialized, PontifexError, SecureModuleNotInitialized,
        };

        match &err {
            NotInitialized => {
                tracing::error!("Enclave not initialized");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            SecureModuleNotInitialized => {
                tracing::error!("Secure module not initialized");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            AttestationFailed() => {
                tracing::error!("Attestation failed");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            BrazeRequestFailed(msg) => {
                tracing::error!("Braze request failed: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            DecryptPushIdFailed(msg) => {
                tracing::error!("Decrypt push ID failed: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            PontifexError(msg) => {
                tracing::error!("Pontifex error: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            KeyPairCreationFailed => {
                tracing::error!("Key pair creation failed");
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
