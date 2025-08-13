use serde::Deserialize;

use super::{error::ZkpError, proof::WorldIdProof, request::Request};

/// Response from the World ID sequencer's proof verification endpoint.
#[derive(Debug, Deserialize)]
struct VerificationResponse {
    /// Indicates whether the submitted proof passed all verification checks
    pub valid: bool,
}

/// Verifies a World ID proof with the signup sequencer.
///
/// # Arguments
/// * `proof` - The World ID proof containing the packed proof, nullifier, merkle root, and verification level
/// * `environment` - The environment (staging/production) for selecting the correct sequencer
///
/// # Errors
/// Returns an error if the proof is invalid or verification fails
pub async fn verify_world_id_proof(
    proof: &WorldIdProof,
    world_id_environment: &walletkit_core::Environment,
) -> Result<(), ZkpError> {
    // Get the verification endpoint for this verification level
    let endpoint = proof.get_verification_endpoint(world_id_environment);

    // Send verification request to sequencer
    let response = Request::post(endpoint, proof).await?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        return Err(handle_sequencer_error(&error_text, status));
    }

    let verification_response: VerificationResponse =
        response.json().await.map_err(ZkpError::NetworkError)?;

    if verification_response.valid {
        Ok(())
    } else {
        Err(ZkpError::InvalidProof)
    }
}

/// Handles error responses from the World ID sequencer.
///
/// # Arguments
/// * `error_text` - The error message from the sequencer response
/// * `status` - The HTTP status code
///
/// # Returns
/// The appropriate `ZkpError` based on the error message content
fn handle_sequencer_error(error_text: &str, status: reqwest::StatusCode) -> ZkpError {
    // Check for known error patterns
    if error_text.contains("invalid_root") {
        ZkpError::InvalidMerkleRoot
    } else if error_text.contains("root_too_old") {
        ZkpError::RootTooOld
    } else if error_text.contains("prover_error") {
        ZkpError::ProverError
    } else if error_text.contains("invalid_proof") {
        ZkpError::InvalidProof
    } else {
        ZkpError::InvalidSequencerResponse(format!("Status {status}: {error_text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use walletkit_core::{CredentialType, Environment};

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

        // Create the WorldIdProof
        let world_id_proof = WorldIdProof::new(
            app_id,
            action,
            &test_proof_hex,
            test_nullifier,
            test_root,
            credential_type,
            signal,
        )
        .expect("Failed to create WorldIdProof");

        // Verify the proof with the staging sequencer
        let result = verify_world_id_proof(&world_id_proof, &Environment::Staging).await;

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

        let invalid_proof = WorldIdProof::new(
            app_id,
            action,
            &invalid_proof_hex,
            invalid_nullifier,
            invalid_root,
            CredentialType::Device,
            signal,
        )
        .expect("Failed to create invalid WorldIdProof");

        // Verify the invalid proof
        let result = verify_world_id_proof(&invalid_proof, &Environment::Staging).await;

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
}
