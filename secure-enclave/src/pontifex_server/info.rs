use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclaveInfoRequest, EnclaveInfoResponse};
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    _: EnclaveInfoRequest,
) -> Result<EnclaveInfoResponse, EnclaveError> {
    let state = state.read().await.enclave_instance_id.clone();

    Ok(EnclaveInfoResponse {
        enclave_instance_id: state,
    })
}
