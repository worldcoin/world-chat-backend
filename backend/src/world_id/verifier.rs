use semaphore_rs::{packed_proof::PackedProof, protocol::Proof};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use walletkit_core::{proof::ProofContext, CredentialType, U256Wrapper};

use super::{error::WorldIdError, request::Request};

/// This is the lowest bound accepted by the sequencer
/// Prevents from using proofs with a root that is more than 1 hour old
const MAX_ROOT_AGE_SECS: i64 = 3600;

/// A struct with all the World ID proof fields.
///
/// This struct can be serialized and used directly to verify in the sequencer.
///
/// [Sequencer API spec](https://github.com/worldcoin/signup-sequencer/blob/main/schemas/openapi-v2.yaml)
///
/// [World ID concepts](https://docs.world.org/world-id/concepts)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SequencerVerificationRequest {
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
    /// Maximum age of the merkle root in seconds
    pub max_root_age_seconds: i64,
}

/// Response from the World ID sequencer's proof verification endpoint.
#[derive(Debug, Deserialize)]
struct SequencerVerificationResponse {
    /// Indicates whether the submitted proof passed all verification checks
    pub valid: bool,
}

/// Internal function that sends a verification request to the World ID sequencer.
///
/// This function handles the HTTP communication with the sequencer and parses the response.
///
/// # Arguments
/// * `request` - The pre-constructed verification request containing all proof components
/// * `endpoint` - The sequencer endpoint URL for the specific credential type and environment
///
/// # Errors
/// Returns an error if:
/// - The network request fails
/// - The sequencer returns a non-success status code
/// - The proof verification fails
async fn verify_world_id_proof_with_sequencer(
    request: &SequencerVerificationRequest,
    endpoint: &str,
) -> Result<(), WorldIdError> {
    // Send verification request to sequencer
    let response = Request::post(endpoint, request).await?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        return Err(handle_sequencer_error(&error_text, status));
    }

    let verification_response: SequencerVerificationResponse =
        response.json().await.map_err(WorldIdError::NetworkError)?;

    if verification_response.valid {
        Ok(())
    } else {
        Err(WorldIdError::InvalidProof)
    }
}

/// Verifies a World ID zero-knowledge proof with the World ID sequencer.
///
/// This is the main entry point for World ID proof verification. It takes the raw proof
/// components, constructs the necessary data structures, and verifies the proof with
/// the appropriate sequencer endpoint based on the credential type and environment.
///
/// # Arguments
/// * `app_id` - The World ID app identifier
/// * `action` - The action string that was used to generate the proof (e.g., "login", "vote")
/// * `proof` - The packed zero-knowledge proof as a hex string (256 bytes / 512 hex chars)
/// * `nullifier_hash` - The nullifier hash as a hex string (prevents double-signaling)
/// * `root` - The merkle tree root as a hex string
/// * `credential_type` - The type of credential (Device, Orb, Phone)
/// * `signal` - The signal data that was included in the proof generation
/// * `world_id_environment` - The environment (Staging or Production)
///
/// # Returns
/// * `Ok(())` if the proof is valid and verified by the sequencer
/// * `Err(WorldIdError)` if the proof is invalid or verification fails
///
/// # Errors
/// Returns an error if:
/// - The proof string cannot be parsed as a valid packed proof
/// - The root or nullifier hash are not valid hex strings
/// - The sequencer rejects the proof (invalid, expired root, etc.)
/// - Network errors occur during verification
///
/// Read about [World ID Core Concepts](https://docs.world.org/world-id/concepts)
/// ```
#[allow(clippy::too_many_arguments)]
pub async fn verify_world_id_proof(
    app_id: &str,
    action: &str,
    proof: &str,
    nullifier_hash: &str,
    root: &str,
    credential_type: CredentialType,
    signal: &str,
    world_id_environment: &walletkit_core::Environment,
) -> Result<(), WorldIdError> {
    // Parse and validate the packed proof from hex string
    let proof: Proof = PackedProof::from_str(proof)
        .map_err(|e| WorldIdError::InvalidProofData(format!("Invalid packed proof: {e}")))?
        .into();

    // Parse and validate the merkle root and nullifier hash
    let root = U256Wrapper::try_from_hex_string(root)
        .map_err(|e| WorldIdError::InvalidProofData(format!("Invalid merkle root: {e}")))?;
    let nullifier_hash = U256Wrapper::try_from_hex_string(nullifier_hash)
        .map_err(|e| WorldIdError::InvalidProofData(format!("Invalid nullifier hash: {e}")))?;

    // Generate the proof context which computes the external nullifier and signal hash
    // These are derived from the app_id, action, signal, and credential_type
    let proof_context = ProofContext::new_from_bytes(
        app_id,
        Some(action.as_bytes().to_vec()),
        Some(signal.as_bytes().to_vec()),
        credential_type,
    );

    // Construct the verification request with all necessary components
    let request = SequencerVerificationRequest {
        proof,
        root,
        nullifier_hash,
        external_nullifier_hash: proof_context.external_nullifier,
        signal_hash: proof_context.signal_hash,
        max_root_age_seconds: MAX_ROOT_AGE_SECS,
    };

    // Get the appropriate sequencer endpoint based on credential type and environment
    let endpoint = format!(
        "{}/v2/semaphore-proof/verify",
        credential_type.get_sign_up_sequencer_host(world_id_environment)
    );

    // Send the verification request to the sequencer
    verify_world_id_proof_with_sequencer(&request, &endpoint).await
}

/// Handles error responses from the World ID sequencer.
///
/// # Arguments
/// * `error_text` - The error message from the sequencer response
/// * `status` - The HTTP status code
///
/// # Returns
/// The appropriate `ZkpError` based on the error message content
fn handle_sequencer_error(error_text: &str, status: reqwest::StatusCode) -> WorldIdError {
    // Check for known error patterns
    if error_text.contains("invalid_root") {
        WorldIdError::InvalidMerkleRoot
    } else if error_text.contains("root_too_old") {
        WorldIdError::RootTooOld
    } else if error_text.contains("prover_error") {
        WorldIdError::ProverError
    } else if error_text.contains("invalid_proof") {
        WorldIdError::InvalidProof
    } else {
        WorldIdError::InvalidSequencerResponse(format!("Status {status}: {error_text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use walletkit_core::{world_id::WorldId, CredentialType, Environment};

    /// Tests the WorldIdProof construction and verification flow.
    ///
    /// This test verifies that:
    /// 1. We can construct a WorldIdProof from the expected parameters
    /// 2. The verification function correctly communicates with the staging sequencer
    /// 3. The error handling works as expected
    ///
    /// Note: This uses a test proof that won't be valid in the real sequencer
    /// since it's not from a registered World ID. To test with a real World ID,
    /// you would need to use credentials from an actual registered identity.
    #[tokio::test]
    async fn test_world_id_proof_construction_and_verification_flow() {
        // Test parameters
        let app_id = "app_staging_509648994ab005fe79c4ddd0449606ca";
        let action = "test_action";
        let signal = "test_signal_data";
        let credential_type = CredentialType::Device;

        // Create a test proof (this won't be valid in the sequencer, but tests the flow)
        // Using a properly formatted packed proof hex string (256 bytes = 512 hex chars)
        let test_proof_hex = "0x".to_string() + &"1".repeat(512);
        let test_nullifier = "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa";
        let test_root = "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d";
        // Verify the proof with the staging sequencer
        let result = verify_world_id_proof(
            app_id,
            action,
            &test_proof_hex,
            test_nullifier,
            test_root,
            credential_type,
            signal,
            &Environment::Staging,
        )
        .await;

        // The proof will be rejected since it's not from a real registered World ID
        // But this tests that our verification flow works correctly
        assert!(
            result.is_err(),
            "Test proof should be rejected by the sequencer"
        );

        // Verify we get an appropriate error
        match result {
            Err(err) => {
                tracing::debug!("Invalid proof rejected with error: {:?}", err);
            }
            _ => panic!("Test proof should not have been accepted"),
        }
    }

    /// Tests that an invalid proof is correctly rejected by the sequencer.
    #[tokio::test]
    async fn test_verify_invalid_world_id_proof() {
        // Create a completely invalid proof with garbage data
        let app_id = "app_staging_509648994ab005fe79c4ddd0449606ca";
        let action = "test_action";
        let signal = "test_signal";

        // Create an invalid proof string (zeros)
        let invalid_proof_hex = "0x".to_string() + &"0".repeat(512); // 256 bytes = 512 hex chars
        let invalid_nullifier =
            "0x0000000000000000000000000000000000000000000000000000000000000001";
        let invalid_root = "0x0000000000000000000000000000000000000000000000000000000000000002";

        // Verify the invalid proof
        let result = verify_world_id_proof(
            app_id,
            action,
            &invalid_proof_hex,
            invalid_nullifier,
            invalid_root,
            CredentialType::Device,
            signal,
            &Environment::Staging,
        )
        .await;

        // The proof should be rejected
        assert!(result.is_err(), "Invalid proof was unexpectedly accepted");

        // Check that we get the expected error type
        match result {
            Err(err) => {
                tracing::debug!("Invalid proof rejected with error: {:?}", err);
            }
            _ => panic!("Invalid proof should not have been accepted"),
        }
    }

    #[tokio::test]
    async fn test_verify_valid_world_id_proof() {
        // Create a valid proof
        let app_id = "app_staging_509648994ab005fe79c4ddd0449606ca";
        let action = "test_action";
        let signal = "test_signal";

        let world_id = WorldId::new(b"not_a_real_secret", &walletkit_core::Environment::Staging);
        let context = ProofContext::new(
            app_id,
            Some(action.to_string()),
            Some(signal.to_string()),
            CredentialType::Device,
        );

        let proof = world_id
            .generate_proof(&context)
            .await
            .expect("Failed to generate proof");

        let result = verify_world_id_proof(
            app_id,
            action,
            &proof.get_proof_as_string(),
            &proof.get_nullifier_hash().to_hex_string(),
            &proof.get_merkle_root().to_hex_string(),
            CredentialType::Device,
            signal,
            &walletkit_core::Environment::Staging,
        )
        .await;

        assert!(result.is_ok(), "Valid proof was rejected");
    }
}
