use anyhow::Result;
use secure_enclave::{pontifex_server::start_pontifex_server, state::EnclaveState};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

const PONTIFEX_PORT: u32 = 1000;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_level(true)
        .pretty()
        .init();

    info!("Starting Secure Enclave");

    // TODO: Explore retrying on failure and compatibility with pod restart
    let state = Arc::new(RwLock::new(EnclaveState::default()));
    if let Err(e) = start_pontifex_server(state, PONTIFEX_PORT).await {
        error!("Failed to start pontifex server: {e}");
        return Err(e);
    }

    info!("Shutting down Secure Enclave");

    Ok(())
}
