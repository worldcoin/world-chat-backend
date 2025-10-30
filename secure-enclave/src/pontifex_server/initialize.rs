use std::sync::Arc;
use std::time::Duration;

use crate::{encryption::KeyPair, state::EnclaveState};
use enclave_types::{EnclaveError, EnclaveInitializeRequest, EnclaveSecretKeyRequest};
use tokio::sync::RwLock;
use tracing::info;

/// Parent CID
const PARENT_CID: u32 = 3;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    config: EnclaveInitializeRequest,
) -> Result<(), EnclaveError> {
    let client = pontifex::http::client_http2_only(
        config.braze_http_proxy_port,
        &pontifex::http::Http2ClientConfig::default(),
    );

    let ephemeral_key_pair = state.read().await.ephemeral_key_pair.clone();
    let attestation_doc_with_ephemeral_pk =
        state.read().await.attestation_doc_with_ephemeral_pk.clone();
    let key_pair = try_retrieve_key_pair(
        config.enclave_cluster_proxy_port,
        config.can_generate_key_pair,
        ephemeral_key_pair,
        attestation_doc_with_ephemeral_pk,
    )
    .await?;

    let mut state_guard = state.write().await;
    state_guard.http_proxy_client = Some(client);
    state_guard.braze_api_key = Some(config.braze_api_key);
    state_guard.braze_api_url = Some(format!(
        "https://rest.{}.braze.com",
        config.braze_api_region
    ));
    state_guard.encryption_keys = Some(key_pair);
    state_guard.initialized = true;

    info!("✅ Enclave initialized successfully");

    Ok(())
}

async fn try_retrieve_key_pair(
    enclave_cluster_proxy_port: u32,
    can_generate_key_pair: bool,
    ephemeral_key_pair: KeyPair,
    attestation_doc_with_ephemeral_pk: Vec<u8>,
) -> Result<KeyPair, EnclaveError> {
    match request_key_pair_from_enclaves_cluster(
        enclave_cluster_proxy_port,
        ephemeral_key_pair,
        attestation_doc_with_ephemeral_pk,
    )
    .await
    {
        Ok(key_pair) => Ok(key_pair),
        Err(e) => {
            tracing::error!("Error retrieving key pair from enclaves cluster: {e:?}");

            if can_generate_key_pair {
                tracing::info!("Generating new key pair");

                Ok(KeyPair::generate())
            } else {
                tracing::error!("Cannot generate key pair");
                Err(e)
            }
        }
    }
}

async fn request_key_pair_from_enclaves_cluster(
    enclave_cluster_proxy_port: u32,
    ephemeral_key_pair: KeyPair,
    attestation_doc_with_ephemeral_pk: Vec<u8>,
) -> Result<KeyPair, EnclaveError> {
    let proxy_connection_details =
        pontifex::client::ConnectionDetails::new(PARENT_CID, enclave_cluster_proxy_port);

    // Add timeout to the Pontifex call
    let timeout_duration = Duration::from_secs(5);

    let sealed_key = tokio::time::timeout(
        timeout_duration,
        pontifex::client::send::<EnclaveSecretKeyRequest>(
            proxy_connection_details,
            &EnclaveSecretKeyRequest {
                attestation_doc: attestation_doc_with_ephemeral_pk,
            },
        ),
    )
    .await
    .map_err(|_| EnclaveError::PontifexError("Request timed out after 10 seconds".to_string()))?
    .map_err(|e| EnclaveError::PontifexError(e.to_string()))??;

    let ephemeral_sk = ephemeral_key_pair.private_key;
    let secret_key = ephemeral_sk
        .unseal(&sealed_key)
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("Unseal failed: {e:?}")))?;

    let key_pair = KeyPair::from_secret_key_bytes(&secret_key)?;

    Ok(key_pair)
}
