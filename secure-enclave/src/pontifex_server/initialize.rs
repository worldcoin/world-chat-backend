use std::sync::Arc;

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
    let key_pair = try_retrieve_key_pair(
        config.enclave_cluster_proxy_port,
        config.can_generate_key_pair,
    )
    .await?;

    let mut state = state.write().await;
    state.http_proxy_client = Some(client);
    state.braze_api_key = Some(config.braze_api_key);
    state.braze_api_url = Some(format!(
        "https://rest.{}.braze.com",
        config.braze_api_region
    ));
    state.keys = Some(key_pair);
    state.initialized = true;

    info!("✅ Enclave initialized successfully");

    Ok(())
}

async fn try_retrieve_key_pair(
    enclave_cluster_proxy_port: u32,
    can_generate_key_pair: bool,
) -> Result<KeyPair, EnclaveError> {
    match request_key_pair_from_enclaves_cluster(enclave_cluster_proxy_port).await {
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
) -> Result<KeyPair, EnclaveError> {
    let proxy_connection_details =
        pontifex::client::ConnectionDetails::new(PARENT_CID, enclave_cluster_proxy_port);
    let response = pontifex::client::send::<EnclaveSecretKeyRequest>(
        proxy_connection_details,
        // TODO: Add attestation doc with empheral public key
        &EnclaveSecretKeyRequest {
            attestation_doc: vec![],
        },
    )
    .await
    .map_err(|e| EnclaveError::PontifexError(e.to_string()))??;

    let key_pair = KeyPair::from_secret_key_bytes(&response)?;

    Ok(key_pair)
}
