use anyhow::Result;
use enclave_types::EnclaveInitializeRequest;
use std::env;
use std::time::Duration;
use tracing::{error, info, warn};

mod redis;
use redis::RedisKeyManager;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_SECS: u64 = 2;

/// This is the entry point for the enclave initialization process.
/// It will attempt to initialize the enclave and will retry up to MAX_RETRIES times.
/// If the enclave initialization fails, it will exit with a non-zero exit code.
///
/// Uses Redis to coordinate key generation between enclaves.
#[tokio::main]
async fn main() -> Result<()> {
    let (_guard, _tracer_shutdown) = datadog_tracing::init()?;

    info!("Starting enclave initialization");

    // Read environment variables
    let enclave_cid: u32 = env::var("NITRO_CID")
        .expect("NITRO_CID environment variable not set")
        .parse()
        .expect("Invalid NITRO_CID value");

    let enclave_port: u32 = env::var("NITRO_PORT")
        .expect("NITRO_PORT environment variable not set")
        .parse()
        .expect("Invalid NITRO_PORT value");

    let braze_api_key =
        env::var("BRAZE_API_KEY").expect("BRAZE_API_KEY environment variable not set");

    let braze_api_region =
        env::var("BRAZE_API_REGION").expect("BRAZE_API_REGION environment variable not set");

    let braze_http_proxy_port: u32 = env::var("BRAZE_HTTP_PROXY_PORT")
        .expect("BRAZE_HTTP_PROXY_PORT environment variable not set")
        .parse()
        .expect("Invalid BRAZE_HTTP_PROXY_PORT value");

    let enclave_cluster_proxy_port: u32 = env::var("ENCLAVE_CLUSTER_PROXY_PORT")
        .expect("ENCLAVE_CLUSTER_PROXY_PORT environment variable not set")
        .parse()
        .expect("Invalid ENCLAVE_CLUSTER_PROXY_PORT value");

    // Get track identifier
    let track = env::var("ENCLAVE_TRACK").expect("ENCLAVE_TRACK environment variable not set");
    info!("Initializing enclave for track: {}", track);

    // Get Redis URL from environment
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL environment variable not set");

    // Initialize Redis key manager
    let key_manager = RedisKeyManager::new(&redis_url, &track)
        .await
        .expect("Failed to connect to Redis");

    // Determine if we should generate a key using Redis mutex
    let can_generate_key_pair = key_manager.should_generate_key().await.unwrap_or_else(|e| {
        warn!("Failed to check key generation status, assuming we should not generate a key: {e}",);
        false
    });

    // Create connection details for pontifex
    let connection_details = pontifex::client::ConnectionDetails::new(enclave_cid, enclave_port);

    // Create initialization request
    let init_request = EnclaveInitializeRequest {
        braze_api_key,
        braze_api_region,
        braze_http_proxy_port,
        enclave_cluster_proxy_port,
        can_generate_key_pair,
    };

    // Retry loop for initialization
    for attempt in 1..=MAX_RETRIES {
        info!("Initialization attempt {attempt}/{MAX_RETRIES}");

        // Flatten the double Result and convert to a single error type
        let result =
            pontifex::client::send::<EnclaveInitializeRequest>(connection_details, &init_request)
                .await
                .map_err(|e| anyhow::anyhow!("Transport error: {}", e))
                .and_then(|inner| inner.map_err(|e| anyhow::anyhow!("Enclave error: {:?}", e)));

        match result {
            Ok(()) => {
                info!("✅ Enclave initialized successfully, track: {track}, can_generate_key_pair: {can_generate_key_pair}");

                // If we generated a key, mark it as loaded in Redis
                if can_generate_key_pair {
                    if let Err(e) = key_manager.mark_key_loaded().await {
                        error!("Failed to mark key as loaded in Redis: {}", e);
                        // Continue anyway - the key was generated successfully
                    }
                }

                return Ok(());
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    error!(
                        "Initialization attempt {attempt} failed: {e:?}. Retrying in {RETRY_DELAY_SECS} seconds...",
                    );
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                } else {
                    // Release the lock if we were trying to generate a key but failed
                    if can_generate_key_pair {
                        if let Err(e) = key_manager.release_lock().await {
                            error!("Failed to release key generation lock: {}", e);
                        }
                    }

                    error!(
                        "FATAL: Failed to initialize enclave after {MAX_RETRIES} attempts: {e:?}",
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    unreachable!()
}
