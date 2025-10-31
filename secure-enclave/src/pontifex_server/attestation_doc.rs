use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveAttestationDocRequest, EnclaveAttestationDocResponse, EnclaveError};
use pontifex::SecureModule;
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    _: EnclaveAttestationDocRequest,
) -> Result<EnclaveAttestationDocResponse, EnclaveError> {
    let state = state.read().await;
    let public_key = state
        .encryption_keys
        .as_ref()
        .ok_or(EnclaveError::NotInitialized)?
        .public_key
        .to_bytes();
    let nsm = SecureModule::try_global().ok_or(EnclaveError::SecureModuleNotInitialized)?;

    let attestation = nsm
        .raw_attest(None::<Vec<u8>>, None::<Vec<u8>>, Some(public_key))
        .map_err(|e| {
            tracing::error!("failed to attest: {e:?}");
            EnclaveError::AttestationFailed()
        })?;

    Ok(EnclaveAttestationDocResponse { attestation })
}
