use std::sync::Arc;

use anyhow::Result;
use backend_storage::{push_subscription::PushSubscriptionStorage, queue::NotificationQueue};
use datadog_tracing::axum::shutdown_signal;
use enclave_worker::{notification_processor::NotificationProcessor, server, types::Environment};
use tokio_util::sync::CancellationToken;
use tracing::info;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_sqs::Client as SqsClient;

#[tokio::main]
async fn main() -> Result<()> {
    let env = Environment::from_env();

    info!("Starting Enclave Worker in {:?} environment", env);

    // Initialize Datadog tracing
    // This will set up OpenTelemetry with Datadog exporter
    // The _guard must be kept alive for the duration of the program
    let (_guard, tracer_shutdown) = datadog_tracing::init()?;

    // Initialize DogStatsD metrics client
    let dd_agent_host =
        std::env::var("DD_AGENT_HOST").expect("DD_AGENT_HOST environment variable is not set");
    datadog_metrics::init(
        dd_agent_host,
        "world_chat",
        "enclave_worker",
        env.to_string(),
    );

    info!("✅ Initialized DogStatsD metrics");

    // Initialize notification queue
    let sqs_client = Arc::new(SqsClient::new(&env.aws_config().await));
    let notification_queue = Arc::new(NotificationQueue::new(
        sqs_client,
        env.notification_queue_config(),
    ));

    info!("✅ Initialized notification queue");

    // Initialise Push Notification Subscription storage
    let dynamodb_client = Arc::new(DynamoDbClient::new(&env.aws_config().await));
    let subscription_storage = Arc::new(PushSubscriptionStorage::new(
        dynamodb_client,
        env.push_subscription_table_name(),
    ));

    info!("✅ Initialized push subscription storage");

    // Initialize Enclave connection details
    let enclave_connection_details =
        pontifex::client::ConnectionDetails::new(env.enclave_cid(), env.enclave_port());

    // Single shutdown token for everything
    let shutdown_token = CancellationToken::new();
    let signal_token = shutdown_token.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("Shutting down Enclave Worker...");
        signal_token.cancel();
    });

    // Start notification processor
    let notification_processor_handle = {
        let queue = notification_queue.clone();
        let storage = subscription_storage.clone();
        let token = shutdown_token.clone();

        tokio::spawn(async move {
            NotificationProcessor::new(queue, storage, token)
                .start()
                .await;
        })
    };

    // Start HTTP server (blocks until shutdown)
    let server_result = server::start(
        env,
        notification_queue,
        subscription_storage,
        enclave_connection_details,
        shutdown_token,
    )
    .await;

    // Wait for processor to finish
    notification_processor_handle.await.ok();

    // Ensure the tracer is properly shut down
    tracer_shutdown.shutdown();

    info!("✅ Enclave Worker shutdown complete");

    server_result
}
