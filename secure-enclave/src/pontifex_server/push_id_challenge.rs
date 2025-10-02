use std::sync::Arc;

use enclave_types::{EnclaveError, EnclavePushIdChallengeRequest};
use tokio::sync::RwLock;

use crate::state::EnclaveState;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclavePushIdChallengeRequest,
) -> Result<bool, EnclaveError> {
    let state = state.read().await;
    let encryption_key = state.keys.private_key.clone();

    let decrypted_push_id_1 = encryption_key
        .unseal(&request.encrypted_push_id_1)
        .map_err(|e| EnclaveError::DecryptPushIdFailed(e.to_string()))?;
    let decrypted_push_id_2 = encryption_key
        .unseal(&request.encrypted_push_id_2)
        .map_err(|e| EnclaveError::DecryptPushIdFailed(e.to_string()))?;

    let push_ids_match = decrypted_push_id_1 == decrypted_push_id_2;

    Ok(push_ids_match)
}
