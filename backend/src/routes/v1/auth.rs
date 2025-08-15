use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use walletkit_core::CredentialType;

use crate::{
    types::{AppError, Environment},
    world_id::verifier::verify_world_id_proof,
};

#[derive(Deserialize, JsonSchema)]
pub struct AuthRequest {
    pub encrypted_push_id: String,
    // World ID proof elements
    pub proof: String,
    pub nullifier_hash: String,
    pub merkle_root: String,
    pub signal: String,
    // pub credential_type: CredentialType,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AuthResponse {
    pub access_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims<'a> {
    sub: &'a str, // Subject (encrypted push id)
    exp: i64,     // Expiration time (Unix timestamp)
    iat: i64,     // Issued at (Unix timestamp)
}

/// Token expiration time in seconds (7 days)
const TOKEN_EXPIRATION_SECS: i64 = 7 * 24 * 60 * 60;

/// Authenticates a user with World ID proof and issues a JWT token.
///
/// Verifies the World ID proof, stores/retrieves the user's auth record using
/// the nullifier hash as a unique identifier, and returns a JWT access token.
///
/// # Errors
///
/// - `WorldIdError` - Invalid World ID proof
/// - `AuthProofStorageError` - Database operation failed
/// - `AppError` - JWT generation failed
pub async fn authorize_handler(
    Extension(environment): Extension<Environment>,
    Extension(auth_proof_storage): Extension<Arc<AuthProofStorage>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Verify World ID proof
    verify_world_id_proof(
        &environment.world_id_app_id(),
        &environment.world_id_action(),
        &request.proof,
        &request.nullifier_hash,
        &request.merkle_root,
        CredentialType::Orb,
        // TODO: wait for deserialize support
        // request.credential_type,
        &request.signal,
        &environment.world_id_environment(),
    )
    .await?;

    // 2. Fetch or create the auth-proof record
    let auth_proof = match auth_proof_storage
        .get_by_nullifier(&request.nullifier_hash)
        .await?
    {
        Some(existing_proof) => {
            // Existing user - return the existing proof
            existing_proof
        }
        None => {
            // New user - create auth-proof record
            auth_proof_storage
                .insert(AuthProofInsertRequest {
                    nullifier: request.nullifier_hash.clone(),
                    encrypted_push_id: request.encrypted_push_id.clone(),
                })
                .await?
        }
    };

    // 3. Issue JWT token
    let access_token = issue_token(&auth_proof.encrypted_push_id, &environment.jwt_secret())?;

    Ok(Json(AuthResponse { access_token }))
}

/// Issues a JWT access token with the encrypted push ID as subject.
///
/// # Errors
///
/// Returns `AppError` if JWT encoding fails.
fn issue_token(encrypted_push_id: &str, jwt_secret: &str) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: encrypted_push_id,
        exp: now + TOKEN_EXPIRATION_SECS,
        iat: now,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )
    .map_err(|e| {
        tracing::error!("Failed to encode JWT: {}", e);
        AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "token_generation_failed",
            "Failed to generate access token",
            false,
        )
    })
}
