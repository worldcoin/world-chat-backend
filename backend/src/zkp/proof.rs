use std::str::FromStr;

use semaphore_rs::{hash_to_field, packed_proof::PackedProof, protocol::Proof};
use serde::Serialize;

use super::error::ZkpError;
use crate::{
    types::Environment,
    zkp::types::{U256Wrapper, VerificationLevel},
};
use alloy_core::sol_types::SolValue;

/// This follows the World ID protocol for external nullifier generation.
#[allow(clippy::option_if_let_else)]
fn compute_external_nullifier(app_id: &str, action: Option<&[u8]>) -> U256Wrapper {
    let mut pre_image = hash_to_field(app_id.as_bytes()).abi_encode_packed();

    if let Some(action) = action {
        pre_image.extend_from_slice(&action);
    }

    hash_to_field(&pre_image).into()
}

/// Computes the signal hash from optional signal bytes.
/// Returns the hash of empty bytes if no signal is provided.
fn compute_signal_hash(signal: Option<&[u8]>) -> U256Wrapper {
    let signal_bytes = signal.unwrap_or_default();
    hash_to_field(signal_bytes).into()
}

/// This struct contains the World ID proof and all the required fields for verification.
/// This struct can be used to verify the proof with the sequencer, it complies with sequencer request spec.
/// https://github.com/worldcoin/signup-sequencer/blob/main/schemas/openapi-v2.yaml
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldIdProof {
    /// The zk-proof
    pub proof: Proof,
    /// The merkle root of the World ID tree
    pub root: U256Wrapper,
    /// The nullifier hash
    pub nullifier_hash: U256Wrapper,
    /// The hashed external nullifier for preventing double-signaling
    pub external_nullifier_hash: U256Wrapper,
    /// The hashed signal which is included in the ZKP
    pub signal_hash: U256Wrapper,

    /// The verification level being used
    /// This is not serialized because it's not part of the sequencer request
    #[serde(skip_serializing)]
    verification_level: VerificationLevel,
}

impl WorldIdProof {
    /// Creates a new World ID proof from the provided parameters.
    ///
    /// # Errors
    /// Returns an error if the proof format is invalid or hex strings cannot be parsed.
    pub fn new(
        proof: &str,
        nullifier_hash: &str,
        root: &str,
        app_id: &str,
        action: &str,
        verification_level: VerificationLevel,
        signal: &str,
    ) -> Result<Self, ZkpError> {
        let external_nullifier_hash = compute_external_nullifier(app_id, Some(action.as_bytes()));
        let signal_hash = compute_signal_hash(Some(signal.as_bytes()));

        let proof: Proof = PackedProof::from_str(proof)
            .map_err(|e| ZkpError::InvalidProofData(format!("Invalid packed proof: {e}")))?
            .into();

        let root = U256Wrapper::try_from_hex_string(root)?;
        let nullifier_hash = U256Wrapper::try_from_hex_string(nullifier_hash)?;

        Ok(Self {
            external_nullifier_hash,
            verification_level,
            signal_hash,
            proof,
            root,
            nullifier_hash,
        })
    }

    pub fn get_verification_endpoint(&self, environment: &Environment) -> String {
        self.verification_level
            .get_verification_endpoint(environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a dummy packed proof for testing
    const fn dummy_proof() -> &'static str {
        // This is a valid base64-encoded packed proof structure for testing
        "0x0f8fe1b21318e00c251fe7ee85d9a35330af28b447a834d70fc58becf0fcfc6c126dc5b8acfcf3c3e92f2b4f4428f873c3be0959de93e9ae58a66d8bb9e1cf1100ba5f992fa1b709d541c0dcb57a4c32ecacd374bbc645f67f26a2389997884a007fd7ba45c7c9af477c6b24f45325b084bf6768a248d66a8beeb995ab066cfa2157025f342b047deb92e29b3aa943c2b30f74475ed470b36c40cea4f129053f110624a601102efee4895ad83f2935e1388ba42d48ed48f95518b7bd49f1817829cd93ba3ef15e80a64840b9b7ab8152b0ad845018fe93721e68bd38796be4bb194cd6ce7637f222cf5e239ad0ce8b77746e3504c633ed7cbd3b4690b755d26b"
    }

    // Helper function to create a dummy hex string for testing
    const fn dummy_hex_string() -> &'static str {
        "0x2a4463bbe55f44c56f6b9320811ee045f136c65afa78047c7764ffda847bcf68"
    }

    #[test]
    fn test_world_id_proof_creation() {
        let proof = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "",
            VerificationLevel::Device,
            "",
        )
        .expect("Failed to create proof");

        assert_eq!(proof.verification_level, VerificationLevel::Device);
        assert!(!proof.external_nullifier_hash.to_hex_string().is_empty());
        assert!(!proof.signal_hash.to_hex_string().is_empty());
    }

    #[test]
    fn test_world_id_proof_with_different_actions() {
        let proof1 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "",
            VerificationLevel::Orb,
            "",
        )
        .expect("Failed to create proof1");

        let proof2 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "vote",
            VerificationLevel::Orb,
            "",
        )
        .expect("Failed to create proof2");

        // Different actions should produce different external nullifiers
        assert_ne!(
            proof1.external_nullifier_hash,
            proof2.external_nullifier_hash
        );
        // But same signal hash (both empty)
        assert_eq!(proof1.signal_hash, proof2.signal_hash);
    }

    #[test]
    fn test_world_id_proof_with_different_signals() {
        let proof1 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "",
            VerificationLevel::Document,
            "",
        )
        .expect("Failed to create proof1");

        let proof2 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "",
            VerificationLevel::Document,
            "test_signal",
        )
        .expect("Failed to create proof2");

        // Same external nullifier (same app_id, same empty action)
        assert_eq!(
            proof1.external_nullifier_hash,
            proof2.external_nullifier_hash
        );
        // But different signal hashes
        assert_ne!(proof1.signal_hash, proof2.signal_hash);
    }

    #[test]
    fn test_deterministic_hashing() {
        let proof1 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "action",
            VerificationLevel::SecureDocument,
            "signal",
        )
        .expect("Failed to create proof1");

        let proof2 = WorldIdProof::new(
            dummy_proof(),
            dummy_hex_string(),
            dummy_hex_string(),
            "app_123",
            "action",
            VerificationLevel::SecureDocument,
            "signal",
        )
        .expect("Failed to create proof2");

        // Same inputs should produce same outputs
        assert_eq!(
            proof1.external_nullifier_hash,
            proof2.external_nullifier_hash
        );
        assert_eq!(proof1.signal_hash, proof2.signal_hash);
    }

    #[test]
    fn test_external_nullifier_hash() {
        let external_nullifier_hash =
            compute_external_nullifier("app_7681258c0610a996cd5cfec7225d6635", Some(b"verify"));

        assert_eq!(
            external_nullifier_hash,
            U256Wrapper::try_from_hex_string(
                "0x00209bd2bd86ff71920dcabca55549d264831e9a496cb4b2b2048dc6895e9188"
            )
            .unwrap()
        );
    }
}
