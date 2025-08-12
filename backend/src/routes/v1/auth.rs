use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    types::{AppError, Environment},
    zkp::{proof::WorldIdProof, verify_world_id_proof, VerificationLevel},
};

#[derive(Deserialize, JsonSchema)]
pub struct AuthRequest {
    pub encrypted_push_id: String,
    // World ID proof elements
    pub proof: String,
    pub nullifier: String,
    pub merkle_root: String,
    pub signal: String,
    pub verification_level: VerificationLevel,
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

/// 30 days
const MIN_UPDATE_AGE_SECS: i64 = 30 * 24 * 60 * 60;
/// 7 days
const TOKEN_EXPIRATION_SECS: i64 = 7 * 24 * 60 * 60;

pub async fn authorize_handler(
    Extension(environment): Extension<Environment>,
    Extension(auth_proof_storage): Extension<Arc<AuthProofStorage>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Verify ZKP
    tracing::debug!(
        proof = %request.proof,
        nullifier = %request.nullifier,
        merkle_root = %request.merkle_root,
        app_id = %environment.world_id_app_id(),
        action = %environment.world_id_action(),
        verification_level = ?request.verification_level,
        signal = %request.signal,
        "Verifying WorldIdProof"
    );
    let world_id_proof = WorldIdProof::new(
        &request.proof,
        &request.nullifier,
        &request.merkle_root,
        &environment.world_id_app_id(),
        &environment.world_id_action(),
        request.verification_level,
        &request.signal,
    )?;
    verify_world_id_proof(&world_id_proof, &environment).await?;

    // 2. Fetch or create the auth-proof record
    let Some(auth_proof) = auth_proof_storage
        .get_by_nullifier(&request.nullifier)
        .await?
    else {
        // 2.5 New user path - create auth-proof and issue token
        auth_proof_storage
            .insert(AuthProofInsertRequest {
                nullifier: request.nullifier,
                encrypted_push_id: request.encrypted_push_id.clone(),
            })
            .await?;
        let access_token = issue_token(&request.encrypted_push_id, &environment.jwt_secret())?;
        return Ok(Json(AuthResponse { access_token }));
    };

    // 3. If the push id matches, issue a token
    //TODO: This function call is mocked, replace it with enclave call
    if push_id_matches(&auth_proof.encrypted_push_id, &request.encrypted_push_id) {
        let access_token = issue_token(&auth_proof.encrypted_push_id, &environment.jwt_secret())?;
        return Ok(Json(AuthResponse { access_token }));
    }

    // 4. Throw error if it's too soon to rotate push id
    let now = Utc::now().timestamp();
    if now - auth_proof.updated_at < MIN_UPDATE_AGE_SECS {
        warn!(
            nullifier = %auth_proof.nullifier,
            "User attempted to rotate push id too soon"
        );
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "auth_proof_too_recent",
            "Auth proof too recent",
            false,
        ));
    }

    // 5. Update the auth-proof record with the new push id and issue a token
    auth_proof_storage
        .update_encrypted_push_id(&auth_proof.nullifier, &request.encrypted_push_id)
        .await?;
    let access_token = issue_token(&request.encrypted_push_id, &environment.jwt_secret())?;
    Ok(Json(AuthResponse { access_token }))
}

fn issue_token(encrypted_push_id: &str, jwt_secret: &str) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: encrypted_push_id,
        exp: now + TOKEN_EXPIRATION_SECS,
        iat: now,
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )?;

    Ok(token)
}

// TODO: Replace this with enclave call
fn push_id_matches(a: &str, b: &str) -> bool {
    a == b || are_plaintext_push_ids_equal(a, b)
}

fn are_plaintext_push_ids_equal(a: &str, b: &str) -> bool {
    a == b
}
