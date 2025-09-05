use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::EnclaveResponse;
use tokio::sync::RwLock;

pub async fn handler(state: Arc<RwLock<EnclaveState>>) -> EnclaveResponse {
    EnclaveResponse::HealthCheck {
        initialized: state.read().await.initialized,
    }
}
