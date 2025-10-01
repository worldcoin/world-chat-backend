use anyhow::Result;
use secure_enclave::{
    encryption::{verify_nsm_hwrng_current, KeyPair},
    pontifex_server::start_pontifex_server,
    state::EnclaveState,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

const PONTIFEX_PORT: u32 = 1000;
/// EX_CONFIG exit code
const EXIT_RNG_MISCONFIG: i32 = 78;

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: This is used for development, remove this before we go live
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_level(true)
        .pretty()
        .init();

    info!("Starting Secure Enclave");

    // Ensure kernel RNG is backed by the Nitro Secure Module HW RNG.
    // Otherwise, hard fail.
    if let Err(_e) = verify_nsm_hwrng_current() {
        std::process::exit(EXIT_RNG_MISCONFIG);
    }

    let keys = KeyPair::generate();

    tracing::info!("🔑 Generated encryption keys");

    // Id for the enclave's lifecycle
    let enclave_instance_id = uuid::Uuid::new_v4().to_string().to_lowercase();
    let state = Arc::new(RwLock::new(EnclaveState::new(keys, enclave_instance_id)));
    if let Err(e) = start_pontifex_server(state, PONTIFEX_PORT).await {
        error!("Failed to start pontifex server: {e}");
        return Err(e);
    }

    info!("Shutting down Secure Enclave");

    Ok(())
}
