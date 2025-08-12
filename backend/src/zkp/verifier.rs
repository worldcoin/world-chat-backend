use super::{error::ZkpError, proof::WorldIdProof, request::Request, types::VerificationResponse};
use crate::types::Environment;

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
    environment: &Environment,
) -> Result<(), ZkpError> {
    // Get the verification endpoint for this verification level
    let endpoint = proof.get_verification_endpoint(environment);

    tracing::debug!(
        signal_hash = %proof.signal_hash,
        external_nullifier_hash = %proof.external_nullifier_hash,
        "Sending verification request to sequencer"
    );

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
