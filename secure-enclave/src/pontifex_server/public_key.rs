use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclavePublicKeyRequest, EnclavePublicKeyResponse};
use pontifex::SecureModule;
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    _: EnclavePublicKeyRequest,
) -> Result<EnclavePublicKeyResponse, EnclaveError> {
    let public_key = state.read().await.keys.public_key.to_bytes().to_vec();
    let nsm = SecureModule::try_global().ok_or(EnclaveError::SecureModuleNotInitialized)?;

    let attestation = nsm
        .raw_attest(None::<Vec<u8>>, None::<Vec<u8>>, Some(public_key))
        .map_err(|e| {
            tracing::error!("failed to attest: {e:?}");
            EnclaveError::AttestationFailed()
        })?;

    Ok(EnclavePublicKeyResponse { attestation })
}
