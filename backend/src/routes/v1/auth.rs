use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use backend_storage::auth_proof::{AuthProofInsertRequest, AuthProofStorage};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use walletkit_core::CredentialType;

use crate::{
    enclave_worker_api::EnclaveWorkerApi,
    jwt::{JwsPayload, JwtManager},
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
    Extension(enclave_worker_api): Extension<Arc<dyn EnclaveWorkerApi>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Validate inputs
    let signal = validate_and_craft_signal(&request.encrypted_push_id, request.timestamp)?;
    let nullifier_hash = validate_and_normalize_nullifier_hash(&request.nullifier_hash)?;

    // 2. Verify World ID proof
    verify_world_id_proof(
        &environment.world_id_app_id(),
        &environment.world_id_action(),
        &request.proof,
        &nullifier_hash,
        &request.merkle_root,
        request.credential_type,
        &signal,
        &environment.world_id_environment(),
    )
    .await?;

    // 3. Fetch or create the auth-proof record
    let auth_proof = auth_proof_storage
        .get_or_insert(AuthProofInsertRequest {
            nullifier: nullifier_hash,
            encrypted_push_id: request.encrypted_push_id.clone(),
        })
        .await?;

    // 4. Decide the push id action
    // - If the push ids match, issue a JWT token with the stored encrypted push id
    // - If the push ids don't match, but the push id rotation is within the threshold, reject the rotation
    // - Otherwise, rotate the push id and issue a JWT token with the new encrypted push id
    let push_id_action = {
        let push_ids_match = enclave_worker_api
            .challenge_push_ids(
                auth_proof.encrypted_push_id.clone(),
                request.encrypted_push_id.clone(),
            )
            .await?;
        let is_push_id_rotation_within_threshold = Utc::now().timestamp()
            <= auth_proof.push_id_rotated_at + PUSH_ID_ROTATION_THRESHOLD_SECS;

        if push_ids_match {
            PushIdAction::IssueStored(auth_proof.encrypted_push_id)
        } else if is_push_id_rotation_within_threshold {
            PushIdAction::RejectRotation
        } else {
            PushIdAction::RotateAndIssue(request.encrypted_push_id)
        }
    };

    match push_id_action {
        PushIdAction::IssueStored(encrypted_push_id) => {
            issue_jwt_token(&jwt_manager, encrypted_push_id).await
        }
        PushIdAction::RejectRotation => Err(AppError::new(
            StatusCode::FORBIDDEN,
            "push_ids_mismatch",
            "Push IDs mismatch",
            false,
        )),
        PushIdAction::RotateAndIssue(encrypted_push_id) => {
            auth_proof_storage
                .update_encrypted_push_id(&auth_proof.nullifier, &encrypted_push_id)
                .await?;
            issue_jwt_token(&jwt_manager, encrypted_push_id).await
        }
    }
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

/// Validates and normalizes a nullifier hash.
///
/// Ensures the nullifier hash:
/// - Starts with '0x'
/// - Is exactly 66 characters long (0x + 64 hex chars)
/// - Contains only hexadecimal characters after the prefix
///
/// Returns the lowercase normalized nullifier hash on success.
///
/// # Errors
/// - `WorldIdError::InvalidProofData` - If the nullifier hash format is invalid
fn validate_and_normalize_nullifier_hash(nullifier_hash: &str) -> Result<String, WorldIdError> {
    let lowercased = nullifier_hash.to_lowercase();

    if !lowercased.starts_with("0x") {
        return Err(WorldIdError::InvalidProofData(
            "Nullifier hash must start with 0x".to_string(),
        ));
    }

    if lowercased.len() != 66 {
        return Err(WorldIdError::InvalidProofData(
            "Nullifier hash must be 66 characters long".to_string(),
        ));
    }

    // Check that all characters after "0x" are valid hex digits
    if !lowercased[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(WorldIdError::InvalidProofData(
            "Nullifier hash must start with 0x and contain only hexadecimal characters".to_string(),
        ));
    }

    Ok(lowercased)
}

/// Enum with possible push id action states
enum PushIdAction {
    IssueStored(String),
    RejectRotation,
    RotateAndIssue(String),
}

/// Helper function to issue a JWT token and return a Json<AuthResponse>
async fn issue_jwt_token(
    jwt_manager: &JwtManager,
    encrypted_push_id: String,
) -> Result<Json<AuthResponse>, AppError> {
    let jws_payload = JwsPayload::from_encrypted_push_id(encrypted_push_id, jwt_manager.issuer());
    let access_token = jwt_manager.issue_token(&jws_payload).await?;

    Ok(Json(AuthResponse {
        access_token,
        expires_at: jws_payload.expires_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_nullifier_hash_valid() {
        let result = validate_and_normalize_nullifier_hash(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        );
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        );
    }

    #[test]
    fn test_validate_nullifier_hash_normalizes_to_lowercase() {
        let result = validate_and_normalize_nullifier_hash(
            "0xABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890",
        );
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        );
    }

    #[test]
    fn test_validate_nullifier_hash_missing_prefix() {
        let result = validate_and_normalize_nullifier_hash(
            "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        );
        assert!(result.is_err());
        match result {
            Err(WorldIdError::InvalidProofData(msg)) => {
                assert!(msg.contains("must start with 0x"));
            }
            _ => panic!("Expected InvalidProofData error"),
        }
    }

    #[test]
    fn test_validate_nullifier_hash_too_short() {
        let result = validate_and_normalize_nullifier_hash("0x1234567890abcdef");
        assert!(result.is_err());
        match result {
            Err(WorldIdError::InvalidProofData(msg)) => {
                assert!(msg.contains("66 characters"));
            }
            _ => panic!("Expected InvalidProofData error"),
        }
    }

    #[test]
    fn test_validate_nullifier_hash_too_long() {
        let result = validate_and_normalize_nullifier_hash(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef00",
        );
        assert!(result.is_err());
        match result {
            Err(WorldIdError::InvalidProofData(msg)) => {
                assert!(msg.contains("66 characters"));
            }
            _ => panic!("Expected InvalidProofData error"),
        }
    }

    #[test]
    fn test_validate_nullifier_hash_invalid_hex_chars() {
        let result = validate_and_normalize_nullifier_hash(
            "0xg234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        );
        assert!(result.is_err());
        match result {
            Err(WorldIdError::InvalidProofData(msg)) => {
                assert!(msg.contains("hexadecimal characters"));
            }
            _ => panic!("Expected InvalidProofData error"),
        }
    }
}
