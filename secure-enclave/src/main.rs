use std::sync::Arc;

use anyhow::Result;
use secure_enclave::{pontifex_server::start_pontifex_server, state::EnclaveState};
use tokio::sync::RwLock;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_level(true)
        .pretty()
        .init();

    info!("Starting Secure Enclave");

    let state = Arc::new(RwLock::new(EnclaveState::default()));
    if let Err(e) = start_pontifex_server(state, 1000).await {
        error!("Failed to start pontifex server: {e}");
        return Err(e);
    }

    info!("Shutting down Secure Enclave");

    Ok(())
}
