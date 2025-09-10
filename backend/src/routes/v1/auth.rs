use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use walletkit_core::CredentialType;

use crate::{
    jwt::{JwsPayload, JwtManager},
    push_id_challenger::PushIdChallenger,
    types::{AppError, Environment},
    world_id::{error::WorldIdError, verifier::verify_world_id_proof},
};

/// The threshold for the last push id rotation in seconds
const PUSH_ID_ROTATION_THRESHOLD_SECS: i64 = 6 * 30 * 24 * 60 * 60; // 6 months

#[derive(Deserialize, JsonSchema)]
pub struct AuthRequest {
    pub encrypted_push_id: String,
    /// Timestamp used in the proof's signal to prevent replay attacks in a 5 minute window
    pub timestamp: i64,
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
    /// JWT access token
    pub access_token: String,
    /// Expires at Unix timestamp in seconds
    pub expires_at: i64,
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
    Extension(push_id_challenger): Extension<Arc<dyn PushIdChallenger>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Verify World ID proof
    let signal = validate_and_craft_signal(&request.encrypted_push_id, request.timestamp)?;

    verify_world_id_proof(
        &environment.world_id_app_id(),
        &environment.world_id_action(),
        &request.proof,
        &request.nullifier_hash,
        &request.merkle_root,
        request.credential_type,
        &signal,
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

    // 3. Challenge push ids
    let push_ids_match = push_id_challenger
        .challenge_push_ids(
            auth_proof.encrypted_push_id.clone(),
            request.encrypted_push_id.clone(),
        )
        .await?;

    // 4.a If the push ids match, issue JWT token with stored encrypted push id
    if push_ids_match {
        let jws_payload = JwsPayload::from_encrypted_push_id(auth_proof.encrypted_push_id);
        let access_token = jwt_manager.issue_token(&jws_payload).await?;

        return Ok(Json(AuthResponse {
            access_token,
            expires_at: jws_payload.expires_at,
        }));
    }

    // If the push ids don't match
    // and the push id rotation is within the threshold, we return a 403
    // This is to prevent abuse of the push id rotation
    let is_push_id_rotation_within_threshold =
        Utc::now().timestamp() <= auth_proof.push_id_rotated_at + PUSH_ID_ROTATION_THRESHOLD_SECS;
    if is_push_id_rotation_within_threshold {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "push_ids_mismatch",
            "Push IDs mismatch",
            false,
        ));
    }

    // Otherwise, we update the encrypted push id and issue a JWT token with the new encrypted push id
    auth_proof_storage
        .update_encrypted_push_id(&auth_proof.nullifier, &request.encrypted_push_id.clone())
        .await?;

    // 5. Issue JWT token with stored encrypted push id
    let jws_payload = JwsPayload::from_encrypted_push_id(request.encrypted_push_id);
    let access_token = jwt_manager.issue_token(&jws_payload).await?;

    Ok(Json(AuthResponse {
        access_token,
        expires_at: jws_payload.expires_at,
    }))
}

/// Enforce a 5 minute window for the timestamp used in the signal.
/// This prevents old proofs from being used.
const TIMESTAMP_EXPIRATION_SECS: i64 = 5 * 60;

/// Creates a signal by combining the encrypted push id and the timestamp.
/// This way we ensure:
///  - The proof can be used only in a 5 minute window
///  - The proof belongs to the user with the requested push id
///
/// # Errors
/// - `WorldIdError::InvalidProof` - Because we don't want to leak information about the proof
fn validate_and_craft_signal(
    encrypted_push_id: &str,
    timestamp: i64,
) -> Result<String, WorldIdError> {
    let now = Utc::now().timestamp();
    if timestamp > now {
        return Err(WorldIdError::InvalidProof);
    }
    if now - timestamp > TIMESTAMP_EXPIRATION_SECS {
        return Err(WorldIdError::InvalidProof);
    }

    Ok(format!("{encrypted_push_id}:{timestamp}"))
}
