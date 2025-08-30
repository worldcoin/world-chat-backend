use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use notification_worker::health;
use notification_worker::types::environment::Environment;
use notification_worker::worker::XmtpWorker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Initialize rustls crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Get environment
    let env = Environment::from_env();
    info!("Starting XMTP Notification Worker in {:?} environment", env);

    // Create and start the worker
    match XmtpWorker::new(env.clone()).await {
        Ok(worker) => {
            info!("Successfully connected to XMTP node");

            // Get shutdown token for signal handling
            let shutdown_token = worker.shutdown_token();

            // Start health check server
            let health_shutdown = shutdown_token.clone();
            tokio::spawn(async move {
                if let Err(e) = health::start_health_server(health_shutdown).await {
                    error!("Health server error: {}", e);
                }
            });

            // Spawn signal handler
            let signal_shutdown = shutdown_token.clone();
            tokio::spawn(async move {
                match tokio::signal::ctrl_c().await {
                    Ok(()) => {
                        info!("Received Ctrl+C, initiating graceful shutdown...");
                        signal_shutdown.cancel();
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
