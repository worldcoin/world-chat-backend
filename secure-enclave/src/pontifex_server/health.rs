use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclaveHealthCheckRequest};
use tokio::sync::RwLock;

// This handler simply checks if the enclave is initialized
pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    _: EnclaveHealthCheckRequest,
) -> Result<(), EnclaveError> {
    if !state.read().await.initialized {
        return Err(EnclaveError::NotInitialized);
    }

    Ok(())
}
