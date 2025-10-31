use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclaveSecretKeyRequest};
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    _request: EnclaveSecretKeyRequest,
) -> Result<Vec<u8>, EnclaveError> {
    // TODO: Validate incoming attestation document and encapsulate key only if it matches current bytecode
    let state = state.read().await;
    let secret_key = state
        .encryption_keys
        .as_ref()
        .ok_or(EnclaveError::NotInitialized)?
        .private_key
        .to_bytes()
        .to_vec();

    Ok(secret_key)
}
