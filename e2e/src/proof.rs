use chrono::Utc;
use walletkit_core::{proof::ProofContext, world_id::WorldId, CredentialType};

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
    pub credential_type: CredentialType,
}

pub async fn generate_proof(
    app_id: &str,
    action: &str,
    encrypted_push_id: &str,
    world_id_secret: &[u8],
) -> AuthRequest {
    let now = Utc::now().timestamp();
    let signal = format!("{encrypted_push_id}:{now}");

    let world_id = WorldId::new(world_id_secret, &walletkit_core::Environment::Staging);
    let context = ProofContext::new(
        app_id,
        Some(action.to_string()),
        Some(signal),
        CredentialType::Device,
    );

    let proof = world_id
        .generate_proof(&context)
        .await
        .expect("Failed to generate proof");

    AuthRequest {
        encrypted_push_id: encrypted_push_id.to_string(),
        timestamp: now,
        proof: proof.get_proof_as_string(),
        nullifier_hash: proof.nullifier_hash.to_string(),
        merkle_root: proof.merkle_root.to_string(),
        credential_type: CredentialType::Device,
    }
}
