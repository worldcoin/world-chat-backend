use std::sync::Arc;

use anyhow::Result;
use backend_storage::{push_subscription::PushSubscriptionStorage, queue::NotificationQueue};
use enclave_worker::{server, types::Environment};
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

    let _ = server::start(env, notification_queue, subscription_storage).await;

    // Ensure the tracer is properly shut down
    tracer_shutdown.shutdown();

    Ok(())
}
