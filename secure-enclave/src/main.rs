use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting Secure Enclave");

    // Print the HELLO environment variable with a counter every 5 seconds
    let hello = std::env::var("HELLO").unwrap_or_else(|_| "Hello from enclave!".to_string());
    let mut count = 1;

    tokio::spawn(async move {
        loop {
            println!("[{count:4}] {msg}", count = count, msg = hello);
            count += 1;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    // Keep enclave running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down Secure Enclave");

    Ok(())
}
