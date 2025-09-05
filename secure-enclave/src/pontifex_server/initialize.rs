use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveConfig, EnclaveResponse};
use tokio::sync::RwLock;
use tracing::info;

pub async fn handler(state: Arc<RwLock<EnclaveState>>, config: EnclaveConfig) -> EnclaveResponse {
    let client = pontifex::http::client(config.braze_http_proxy_port);

    let mut state = state.write().await;
    state.http_proxy_client = Some(client);
    state.braze_api_key = Some(config.braze_api_key);
    state.braze_api_endpoint = Some(config.braze_api_endpoint);
    state.initialized = true;

    info!("✅ Enclave initialized successfully");

    EnclaveResponse::InitializeSuccess
}
