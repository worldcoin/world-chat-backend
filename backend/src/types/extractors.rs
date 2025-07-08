//! Custom extractors for request validation

use aide::operation::OperationInput;
use aide::OperationOutput;
use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use schemars::JsonSchema;
use validator::Validate;

use crate::types::error::AppError;

/// Custom JSON extractor that validates the payload
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: serde::de::DeserializeOwned + Validate + JsonSchema,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // First extract JSON
        let Json(payload) = Json::<T>::from_request(req, state)
            .await
            .map_err(|err| match err {
                JsonRejection::MissingJsonContentType(_) => AppError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "invalid_content_type",
                    "Missing Content-Type: application/json header",
                    false,
                ),
                _ => AppError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "invalid_json",
                    "Invalid JSON payload",
                    false,
                ),
            })?;

        // Then validate
        payload.validate().map_err(|errors| {
            // Get the first field error and use its message as the error code
            for (_field, field_errors) in errors.field_errors() {
                if let Some(error) = field_errors.first() {
                    // Use the custom message if provided, otherwise fall back to validation_error
                    if let Some(message) = &error.message {
                        // The message contains our error code
                        let error_code = message.as_ref();
                        return AppError::validation_from_str("", error_code);
                    }
                }
            }
            AppError::validation_from_str("", "validation_error")
        })?;

        Ok(Self(payload))
    }
}

impl<T> OperationInput for ValidatedJson<T>
where
    T: JsonSchema,
{
    fn operation_input(ctx: &mut aide::generate::GenContext, operation: &mut aide::openapi::Operation) {
        // Delegate to Json<T>'s implementation since ValidatedJson has the same structure
        Json::<T>::operation_input(ctx, operation);
    }

    fn inferred_early_responses(
        ctx: &mut aide::generate::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Vec<(Option<u16>, aide::openapi::Response)> {
        // Document validation error responses
        AppError::inferred_responses(ctx, operation)
    }
}
