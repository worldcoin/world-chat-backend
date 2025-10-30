use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclaveSecretKeyRequest};
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclaveSecretKeyRequest,
) -> Result<Vec<u8>, EnclaveError> {
    let attestation_verifier = &state.read().await.attestation_verifier;

    let encryption_keys = state.read().await.encryption_keys.clone();
    let secret_key = encryption_keys
        .as_ref()
        .ok_or(EnclaveError::NotInitialized)?
        .private_key
        .to_bytes();

    let response = attestation_verifier
        .verify_attestation_document_and_encrypt(&request.attestation_doc, &secret_key)
        .map_err(|e| {
            EnclaveError::AttestationVerificationFailed(format!(
                "Failed to verify attestation document: {}",
                e
            ))
        })?;
    let sealed_key = response.ciphertext;

    Ok(sealed_key)
}
