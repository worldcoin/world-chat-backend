use anyhow::{Context, Result};
use enclave_types::EnclaveInitializeRequest;
use std::env;
use std::time::Duration;
use tracing::{error, info};

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY_SECS: u64 = 2;

#[tokio::main]
async fn main() -> Result<()> {
    let (_guard, _tracer_shutdown) = datadog_tracing::init()?;

    info!("Starting enclave initialization");

    // Read environment variables
    let enclave_cid: u32 = env::var("NITRO_CID")
        .context("NITRO_CID environment variable not set")?
        .parse()
        .context("Invalid NITRO_CID value")?;
        
    let enclave_port: u32 = env::var("NITRO_PORT")
        .context("NITRO_PORT environment variable not set")?
        .parse()
        .context("Invalid NITRO_PORT value")?;
        
    let braze_api_key = env::var("BRAZE_API_KEY")
        .context("BRAZE_API_KEY environment variable not set")?;
        
    let braze_api_region = env::var("BRAZE_API_REGION")
        .context("BRAZE_API_REGION environment variable not set")?;
        
    let braze_http_proxy_port: u32 = env::var("BRAZE_HTTP_PROXY_PORT")
        .context("BRAZE_HTTP_PROXY_PORT environment variable not set")?
        .parse()
        .context("Invalid BRAZE_HTTP_PROXY_PORT value")?;

    // Create connection details for pontifex
    let connection_details = pontifex::client::ConnectionDetails::new(enclave_cid, enclave_port);
    
    // Create initialization request
    let init_request = EnclaveInitializeRequest {
        braze_api_key,
        braze_api_region,
        braze_http_proxy_port,
    };

    // Retry loop for initialization
    for attempt in 1..=MAX_RETRIES {
        info!("Initialization attempt {}/{}", attempt, MAX_RETRIES);
        
        match pontifex::client::send::<EnclaveInitializeRequest>(
            connection_details,
            &init_request,
        )
        .await
        {
            Ok(_) => {
                info!("✅ Enclave initialized successfully");
                return Ok(());
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    error!("Initialization attempt {} failed: {}. Retrying in {} seconds...", 
                           attempt, e, RETRY_DELAY_SECS);
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                } else {
                    return Err(anyhow::anyhow!("Failed to initialize enclave after {} attempts: {}", 
                                                MAX_RETRIES, e));
                }
            }
        }
    }

    unreachable!()
}
