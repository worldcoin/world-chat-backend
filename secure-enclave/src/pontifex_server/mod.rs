use std::sync::Arc;

use anyhow::Context;
use enclave_types::{EnclaveHealthCheckRequest, EnclaveInitializeRequest};
use pontifex::Router;
use tokio::sync::RwLock;
mod health;
mod initialize;

use crate::state::EnclaveState;

pub async fn start_pontifex_server(
    state: Arc<RwLock<EnclaveState>>,
    port: u32,
) -> anyhow::Result<()> {
    // Build pontifex router
    let router = Router::with_state(state)
        .route::<EnclaveInitializeRequest, _, _>(initialize::handler)
        .route::<EnclaveHealthCheckRequest, _, _>(health::handler);

    // Start pontifex server
    router
        .serve(port)
        .await
        .context("failed to start pontifex server")?;

    Ok(())
}
