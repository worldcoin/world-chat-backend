use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use walletkit_core::CredentialType;

use crate::{
    jwt::{JwtManager, WorldChatJwtPayload},
    types::{AppError, Environment},
    world_id::verifier::verify_world_id_proof,
};

#[derive(Deserialize, JsonSchema)]
pub struct AuthRequest {
    pub encrypted_push_id: String,
    /// Zero-knowledge proof
    pub proof: String,
    /// Nullifier hash - unique identifier for the user
    pub nullifier_hash: String,
    /// Root of the World ID merkle tree
    pub merkle_root: String,
    /// Enum: `orb`, `device`, `document`, `secure_document`
    #[schemars(with = "String")]
    pub credential_type: CredentialType,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AuthResponse {
    pub access_token: String,
}

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
    Extension(environment): Extension<Environment>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Verify World ID proof
    verify_world_id_proof(
        &environment.world_id_app_id(),
        &environment.world_id_action(),
        &request.proof,
        &request.nullifier_hash,
        &request.merkle_root,
        request.credential_type,
        // We use the encrypted push id as the signal
        // to ensure the proof belongs to the user with requested push id
        &request.encrypted_push_id,
        &environment.world_id_environment(),
    )
    .await?;

    // 2. Fetch or create the auth-proof record
    let auth_proof = auth_proof_storage
        .get_or_insert(AuthProofInsertRequest {
            nullifier: request.nullifier_hash.clone(),
            encrypted_push_id: request.encrypted_push_id.clone(),
        })
        .await?;

    // 3. Issue JWT token with stored encrypted push id
    let access_token = jwt_manager
        .issue_token(WorldChatJwtPayload {
            encrypted_push_id: auth_proof.encrypted_push_id,
        })
        .await?;

    Ok(Json(AuthResponse { access_token }))
}
