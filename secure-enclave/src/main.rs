use anyhow::Result;
use secure_enclave::{
    encryption::verify_nsm_hwrng_current, pontifex_server::start_pontifex_server,
    state::EnclaveState,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Port for the pontifex server
const PONTIFEX_PORT: u32 = 1000;
/// EX_CONFIG exit code
const EXIT_RNG_MISCONFIG: i32 = 78;

#[tokio::main]
async fn main() -> Result<()> {
    // We use tracing for logging, this is only useful when the enclave runs on DEBUG MODE
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_level(true)
        .pretty()
        .init();

    info!("Starting Secure Enclave");

    // Ensure kernel RNG is backed by the Nitro Secure Module HW RNG.
    // Otherwise, hard fail.
    if let Err(_e) = verify_nsm_hwrng_current() {
        std::process::exit(EXIT_RNG_MISCONFIG);
    }

    let state = EnclaveState::new().await?;
    let state = Arc::new(RwLock::new(state));
    if let Err(e) = start_pontifex_server(state, PONTIFEX_PORT).await {
        error!("Failed to start pontifex server: {e}");
        return Err(e);
    }

    info!("Shutting down Secure Enclave");

    Ok(())
}
