//! Universal error handling for the API

use aide::OperationOutput;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_jsonschema::Json;
use backend_storage::auth_proof::AuthProofStorageError;
use schemars::JsonSchema;
use serde::Serialize;

use crate::media_storage::BucketError;
use crate::zkp::ZkpError;

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

/// Convert JSON schema validation errors to application errors
impl From<axum_jsonschema::JsonSchemaRejection> for AppError {
    fn from(err: axum_jsonschema::JsonSchemaRejection) -> Self {
        tracing::warn!("JSON schema validation error: {:?}", err);
        Self::new(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "Request validation failed",
            false,
        )
    }
}

/// Convert bucket errors to application errors
impl From<BucketError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: BucketError) -> Self {
        use BucketError::{
            AwsError, ConfigError, InvalidInput, ObjectExists, S3Error, UpstreamError,
        };

        match &err {
            ObjectExists(id) => {
                tracing::debug!("Object already exists: {id}");
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
            InvalidInput(msg) => {
                tracing::warn!("Invalid input: {msg}");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Invalid input provided",
                    false,
                )
            }
        }
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

impl From<AuthProofStorageError> for AppError {
    fn from(err: AuthProofStorageError) -> Self {
        use AuthProofStorageError::{
            AuthProofExists, DynamoDbDeleteError, DynamoDbGetError, DynamoDbPutError,
            DynamoDbQueryError, DynamoDbUpdateError, SerializationError,
        };

        match &err {
            AuthProofExists => {
                tracing::debug!("Auth proof already exists");
                Self::new(
                    StatusCode::CONFLICT,
                    "already_exists",
                    "Auth proof already exists",
                    false,
                )
            }
            DynamoDbPutError(_)
            | DynamoDbDeleteError(_)
            | DynamoDbGetError(_)
            | DynamoDbQueryError(_)
            | DynamoDbUpdateError(_) => {
                tracing::error!("DynamoDB error: {err}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "database_error",
                    "Database service temporarily unavailable",
                    true,
                )
            }
            SerializationError(msg) => {
                tracing::error!("Serialization error: {msg}");
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

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        tracing::error!("JWT error: {:?}", err);
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            false,
        )
    }
}

impl From<ZkpError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: ZkpError) -> Self {
        use ZkpError::{
            InvalidMerkleRoot, InvalidProof, InvalidProofData, InvalidSequencerResponse,
            NetworkError, ProverError, RootTooOld,
        };

        match &err {
            InvalidProof | InvalidProofData(_) => {
                tracing::warn!("Invalid proof: {err}");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_proof",
                    "The provided proof is invalid",
                    false,
                )
            }
            InvalidMerkleRoot => {
                tracing::warn!("Invalid merkle root");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_merkle_root",
                    "The merkle root is not valid",
                    false,
                )
            }
            RootTooOld => {
                tracing::warn!("Merkle root too old");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "root_too_old",
                    "The merkle root is too old and has been pruned",
                    false,
                )
            }
            ProverError | InvalidSequencerResponse(_) => {
                tracing::error!("Sequencer error: {err}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "verification_service_error",
                    "Verification service temporarily unavailable",
                    true,
                )
            }
            NetworkError(_) => {
                tracing::error!("Network error during verification: {err}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "network_error",
                    "Network error during verification",
                    true,
                )
            }
        }
    }
}
