use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::EnclaveInitializeRequest;
use tokio::sync::RwLock;
use tracing::info;

pub async fn handler(state: Arc<RwLock<EnclaveState>>, config: EnclaveInitializeRequest) {
    let client = pontifex::http::client_http2_only(
        config.braze_http_proxy_port,
        &pontifex::http::Http2ClientConfig::default(),
    );

    let mut state = state.write().await;
    state.http_proxy_client = Some(client);
    state.braze_api_key = Some(config.braze_api_key);
    state.braze_api_url = Some(format!(
        "https://rest.{}.braze.com",
        config.braze_api_region
    ));
    state.initialized = true;

    info!("✅ Enclave initialized successfully");
}
