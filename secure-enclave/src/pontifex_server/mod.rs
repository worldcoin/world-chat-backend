use std::sync::Arc;

use anyhow::Context;
use enclave_types::{
    EnclaveAttestationDocRequest, EnclaveHealthCheckRequest, EnclaveInitializeRequest,
    EnclaveNotificationRequest, EnclavePushIdChallengeRequest,
};
use pontifex::Router;
use tokio::sync::RwLock;

mod health;
mod initialize;
mod notification;
mod public_key;
mod push_id_challenge;

use crate::state::EnclaveState;

pub async fn start_pontifex_server(
    state: Arc<RwLock<EnclaveState>>,
    port: u32,
) -> anyhow::Result<()> {
    // Build pontifex router
    let router = Router::with_state(state)
        .route::<EnclaveInitializeRequest, _, _>(initialize::handler)
        .route::<EnclaveHealthCheckRequest, _, _>(health::handler)
        .route::<EnclaveAttestationDocRequest, _, _>(public_key::handler)
        .route::<EnclavePushIdChallengeRequest, _, _>(push_id_challenge::handler)
        .route::<EnclaveNotificationRequest, _, _>(notification::handler);

    // Start pontifex server
    router
        .serve(port)
        .await
        .context("failed to start pontifex server")?;

    Ok(())
}
