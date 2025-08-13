use std::str::FromStr;

use semaphore_rs::{packed_proof::PackedProof, protocol::Proof};
use serde::Serialize;
use walletkit_core::{proof::ProofContext, CredentialType, U256Wrapper};

use super::error::ZkpError;

/// A struct with all the World ID proof fields.
///
/// This struct can be serialized and used directly to verify in the sequencer.
///
/// [Sequencer API spec](https://github.com/worldcoin/signup-sequencer/blob/main/schemas/openapi-v2.yaml)
///
/// [World ID concepts](https://docs.world.org/world-id/concepts)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldIdProof {
    /// The Zero-Knowledge proof
    pub proof: Proof,
    /// The merkle root of the World ID tree
    pub root: U256Wrapper,
    /// The nullifier hash
    pub nullifier_hash: U256Wrapper,
    /// The hashed external nullifier for preventing double-signaling
    pub external_nullifier_hash: U256Wrapper,
    /// The hashed signal which is included in the ZKP
    pub signal_hash: U256Wrapper,

    /// The credential type isn't part of the sequencer request, but it's used to get the verification endpoint
    #[serde(skip_serializing)]
    credential_type: CredentialType,
}

impl WorldIdProof {
    /// Creates a new World ID proof from the provided parameters.
    ///
    /// # Errors
    /// Returns an error if the proof format is invalid or hex strings cannot be parsed.
    pub fn new(
        app_id: &str,
        action: &str,
        proof: &str,
        nullifier_hash: &str,
        root: &str,
        credential_type: CredentialType,
        signal: &str,
    ) -> Result<Self, ZkpError> {
        let proof: Proof = PackedProof::from_str(proof)
            .map_err(|e| ZkpError::InvalidProofData(format!("Invalid packed proof: {e}")))?
            .into();

        let root = U256Wrapper::try_from_hex_string(root)
            .map_err(|e| ZkpError::InvalidProofData(format!("Invalid merkle root: {e}")))?;
        let nullifier_hash = U256Wrapper::try_from_hex_string(nullifier_hash)
            .map_err(|e| ZkpError::InvalidProofData(format!("Invalid nullifier hash: {e}")))?;

        let proof_context = ProofContext::new_from_bytes(
            app_id,
            Some(action.as_bytes().to_vec()),
            Some(signal.as_bytes().to_vec()),
            credential_type,
        );

        Ok(Self {
            credential_type,
            signal_hash: proof_context.signal_hash,
            external_nullifier_hash: proof_context.external_nullifier,
            proof,
            root,
            nullifier_hash,
        })
    }

    #[must_use]
    pub const fn get_verification_endpoint(
        &self,
        world_id_environment: &walletkit_core::Environment,
    ) -> &str {
        self.credential_type
            .get_sign_up_sequencer_host(world_id_environment)
    }
}
