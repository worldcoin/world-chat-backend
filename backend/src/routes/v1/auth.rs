use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use walletkit_core::CredentialType;

use crate::{jwt::JwtManager, types::AppError, world_id::verifier::verify_world_id_proof};

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
    Extension(jwt_manager): Extension<Arc<JwtManager>>,
    Extension(auth_proof_storage): Extension<Arc<AuthProofStorage>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Get environment from env vars for World ID verification
    // This is safe since these are read-only operations
    let world_id_app_id =
        std::env::var("WORLD_ID_APP_ID").expect("WORLD_ID_APP_ID environment variable is not set");
    let world_id_action =
        std::env::var("WORLD_ID_ACTION").expect("WORLD_ID_ACTION environment variable is not set");
    let world_id_env = if std::env::var("APP_ENV").unwrap_or_default() == "production" {
        walletkit_core::Environment::Production
    } else {
        walletkit_core::Environment::Staging
    };

    // 1. Verify World ID proof
    verify_world_id_proof(
        &world_id_app_id,
        &world_id_action,
        &request.proof,
        &request.nullifier_hash,
        &request.merkle_root,
        CredentialType::Orb,
        // TODO: wait for deserialize support
        // request.credential_type,
        &request.signal,
        &world_id_env,
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

    // 3. Issue JWT token using cached JWT manager (BLAZING FAST!)
    let access_token =
        jwt_manager.issue_token(&auth_proof.encrypted_push_id, TOKEN_EXPIRATION_SECS)?;

    Ok(Json(AuthResponse { access_token }))
}
