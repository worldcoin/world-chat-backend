use std::sync::Arc;

use anyhow::Context;
use enclave_types::{EnclaveRequest, EnclaveResponse};
use tokio::sync::RwLock;
mod health;
mod initialize;

use crate::state::EnclaveState;

async fn handle_request(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclaveRequest,
) -> EnclaveResponse {
    match request {
        EnclaveRequest::Initialize(config) => initialize::handler(state, config).await,
        EnclaveRequest::HealthCheck => health::handler(state).await,
    }
}

pub async fn start_pontifex_server(
    state: Arc<RwLock<EnclaveState>>,
    port: u32,
) -> anyhow::Result<()> {
    pontifex::listen(port, move |request| handle_request(state.clone(), request))
        .await
        .context("failed to pontifex start server")
}
