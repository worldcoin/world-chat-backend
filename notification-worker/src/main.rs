use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use notification_worker::types::environment::Environment;
use notification_worker::worker::{WorkerConfig, XmtpWorker};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize rustls crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Get environment
    let env = Environment::from_env();
    info!("Starting XMTP Notification Worker in {:?} environment", env);

    // Create worker configuration
    let config = WorkerConfig::from_environment(&env);
    info!("Worker configuration: {:?}", config);

    // Create and start the worker
    match XmtpWorker::new(config).await {
        Ok(worker) => {
            info!("Successfully connected to XMTP node");

            // Get shutdown token for signal handling
            let shutdown_token = worker.shutdown_token();

            // Spawn signal handler
            tokio::spawn(async move {
                match tokio::signal::ctrl_c().await {
                    Ok(()) => {
                        info!("Received Ctrl+C, initiating graceful shutdown...");
                        shutdown_token.cancel();
                    }
                    Err(e) => {
                        error!("Failed to listen for Ctrl+C: {}", e);
                    }
                }
            });

            // Run the worker
            if let Err(e) = worker.start().await {
                error!("Worker error: {}", e);
                return Err(e);
            }
        }
        Err(e) => {
            error!("Failed to create worker: {}", e);
            return Err(e);
        }
    }

    info!("XMTP Notification Worker stopped");
    Ok(())
}
