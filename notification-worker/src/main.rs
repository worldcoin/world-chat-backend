use std::sync::Arc;
use tracing::{error, info};

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_sqs::Client as SqsClient;

use backend_storage::push_subscription::PushSubscriptionStorage;
use backend_storage::queue::NotificationQueue;
use notification_worker::health;
use notification_worker::types::environment::Environment;
use notification_worker::worker::XmtpWorker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize Datadog tracing
    // This will set up OpenTelemetry with Datadog exporter
    // The _guard must be kept alive for the duration of the program
    let (_guard, tracer_shutdown) = datadog_tracing::init()?;

    // Initialize rustls crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Get environment
    let env = Environment::from_env();
    info!("Starting XMTP Notification Worker in {:?} environment", env);

    // Initialize notification queue
    let sqs_client = Arc::new(SqsClient::new(&env.aws_config().await));
    let notification_queue = Arc::new(NotificationQueue::new(
        sqs_client,
        env.notification_queue_config(),
    ));

    // Initialise Push Notification Subscription storage
    let dynamodb_client = Arc::new(DynamoDbClient::new(&env.aws_config().await));
    let subscription_storage = Arc::new(PushSubscriptionStorage::new(
        dynamodb_client,
        env.push_subscription_table_name(),
    ));

    // Create and start the worker
    match XmtpWorker::new(env.clone(), notification_queue, subscription_storage).await {
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

    // Ensure the tracer is properly shut down
    tracer_shutdown.shutdown();

    Ok(())
}
