//! Universal error handling for the API

use aide::OperationOutput;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_jsonschema::Json;
use backend_storage::auth_proof::AuthProofStorageError;
use backend_storage::push_subscription::PushSubscriptionStorageError;
use schemars::JsonSchema;
use serde::Serialize;

use crate::jwt::error::JwtError;
use crate::media_storage::BucketError;
use crate::world_id::error::WorldIdError;

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

impl From<PushSubscriptionStorageError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: PushSubscriptionStorageError) -> Self {
        use PushSubscriptionStorageError::{
            DynamoDbDeleteError, DynamoDbGetError, DynamoDbPutError, DynamoDbQueryError,
            DynamoDbUpdateError, ParseSubscriptionError, PushSubscriptionExists,
            SerializationError,
        };

        match &err {
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
            // This should never happen, mapping this for completeness
            PushSubscriptionExists => {
                tracing::error!("Push subscription already exists");
                Self::new(
                    StatusCode::CONFLICT,
                    "already_exists",
                    "Push subscription already exists",
                    false,
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

/// Convert World ID ZKP verification errors to application errors
impl From<WorldIdError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: WorldIdError) -> Self {
        use WorldIdError::{
            InvalidMerkleRoot, InvalidProof, InvalidProofData, InvalidSequencerResponse,
            NetworkError, ProverError, RootTooOld,
        };

        match &err {
            InvalidProof => {
                tracing::warn!("World ID proof verification failed");
                Self::new(
                    StatusCode::UNAUTHORIZED,
                    "invalid_proof",
                    "World ID proof verification failed",
                    false,
                )
            }
            InvalidMerkleRoot => {
                tracing::warn!("Invalid World ID merkle root");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_merkle_root",
                    "Invalid or unknown merkle root",
                    false,
                )
            }
            RootTooOld => {
                tracing::warn!("World ID merkle root is too old");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "root_too_old",
                    "Merkle root is too old and has been pruned",
                    false,
                )
            }
            InvalidProofData(msg) => {
                tracing::warn!("Invalid World ID proof data: {msg}");
                Self::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_proof_data",
                    "Invalid proof data format",
                    false,
                )
            }
            ProverError => {
                tracing::error!("World ID prover service error");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "prover_error",
                    "World ID prover service temporarily unavailable",
                    true,
                )
            }
            NetworkError(e) => {
                tracing::error!("Network error contacting World ID sequencer: {e}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "network_error",
                    "Unable to verify World ID proof due to network issues",
                    true,
                )
            }
            InvalidSequencerResponse(msg) => {
                tracing::error!("Invalid World ID sequencer response: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "sequencer_error",
                    "World ID verification service error",
                    true,
                )
            }
        }
    }
}

/// Convert JWT errors to application errors
impl From<JwtError> for AppError {
    #[allow(clippy::cognitive_complexity)]
    fn from(err: JwtError) -> Self {
        use JwtError::{InvalidSignature, InvalidToken, Kms, Other, SigningInput};

        match &err {
            InvalidToken => Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Invalid or malformed token",
                false,
            ),
            Kms(e) => {
                tracing::error!("KMS error: {e}");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "kms_error",
                    "Key management service temporarily unavailable",
                    true,
                )
            }
            InvalidSignature => Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Invalid or expired token",
                false,
            ),
            SigningInput(msg) => {
                tracing::error!("JWT signing input error: {msg}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error",
                    false,
                )
            }
            Other(e) => {
                tracing::error!("Other JWT error: {e}");
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

/// Convert reqwest errors to application errors
impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        tracing::error!("Reqwest error: {err:?}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            false,
        )
    }
}
