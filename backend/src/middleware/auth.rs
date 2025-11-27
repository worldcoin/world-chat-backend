use std::sync::Arc;

use aide::OperationIo;
use axum::{
    extract::{FromRequestParts, Request},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    middleware::Next,
    response::Response,
    Extension,
};

use crate::{
    jwt::{JwsPayload, JwtManager},
    types::{AppError, Environment},
};

/// JWT cutoff date - reject tokens issued before this time.
/// Set to `None` to disable cutoff, or `Some("YYYY-MM-DDTHH:MM:SSZ")` to enable.
const JWT_ISSUED_AFTER_CUTOFF: Option<&str> = Some("2025-11-27T00:00:00Z");

/// Parse ISO 8601 date string to Unix timestamp at compile time is not possible,
/// so we parse at runtime on first use.
fn parse_cutoff_timestamp() -> Option<i64> {
    JWT_ISSUED_AFTER_CUTOFF.and_then(|date_str| {
        chrono::DateTime::parse_from_rfc3339(date_str)
            .ok()
            .map(|dt| dt.timestamp())
    })
}

/// Authenticated user information extracted from JWT
#[derive(Debug, Clone, OperationIo)]
pub struct AuthenticatedUser {
    /// The encrypted push ID from the JWT subject
    pub encrypted_push_id: String,
}

impl From<JwsPayload> for AuthenticatedUser {
    fn from(payload: JwsPayload) -> Self {
        Self {
            encrypted_push_id: payload.subject,
        }
    }
}

/// Axum extractor for authenticated user
///
/// Use this in your handlers to automatically extract and validate the authenticated user:
/// ```ignore
/// async fn protected_handler(
///     user: AuthenticatedUser,
///     // ... other extractors
/// ) -> Result<impl IntoResponse, AppError> {
///     // Access user.encrypted_push_id or user.claims
///     Ok("Protected content")
/// }
/// ```
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Self>().cloned().ok_or_else(|| {
            AppError::new(
                StatusCode::UNAUTHORIZED,
                "missing_auth",
                "Authentication required but user not found in request extensions",
                false,
            )
        })
    }
}

/// JWT Authentication middleware
///
/// This middleware:
/// 1. Extracts Bearer token from Authorization header
/// 2. Validates JWT using `JwtManager`
/// 3. Adds `AuthenticatedUser` to request extensions
/// 4. Returns 401 for invalid/missing tokens
///
/// In development, use `disable_auth` environment variable to disable auth.
///
/// # Errors
///
/// - `AppError` - Invalid/missing token with 401 status code
pub async fn auth_middleware(
    Extension(jwt_manager): Extension<Arc<JwtManager>>,
    Extension(environment): Extension<Environment>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Extract Authorization header
    let stripped_auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "));

    // If auth is disabled, we skip token validation
    // and use the token as the encrypted push id
    if environment.disable_auth() {
        if let Some(token) = stripped_auth_header {
            let authenticated_user = AuthenticatedUser {
                encrypted_push_id: token.to_string(),
            };
            request.extensions_mut().insert(authenticated_user);
        }

        return Ok(next.run(request).await);
    }

    let token = stripped_auth_header.ok_or_else(|| {
        AppError::new(
            StatusCode::UNAUTHORIZED,
            "missing_token",
            "Authorization header must contain a valid Bearer token",
            false,
        )
    })?;

    // Validate JWT
    let claims = jwt_manager
        .validate(token, parse_cutoff_timestamp())
        .map_err(|_| {
            AppError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Invalid or expired token",
                false,
            )
        })?;

    // Add authenticated user to request extensions
    let user = AuthenticatedUser::from(claims);
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}
