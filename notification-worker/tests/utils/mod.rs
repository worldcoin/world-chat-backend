#![allow(unused_imports, dead_code)]

mod dynamodb_setup;
mod sqs_setup;

use backend_storage::push_notification::PushNotificationStorage;
use dynamodb_setup::DynamoDbTestSetup;

use std::sync::Arc;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_sqs::Client as SqsClient;
use backend_storage::queue::{NotificationQueue, QueueConfig};
use notification_worker::types::environment::Environment;

use notification_worker::worker::message_processor::MessageProcessor;

use crate::utils::sqs_setup::SqsSetup;

/// Setup test environment variables with all the required configuration
fn setup_test_env() {
    // Load test environment variables
    dotenvy::from_path(".env.example").ok();

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}

/// Base test setup with core dependencies and XMTP worker
pub struct TestContext {
    pub environment: Environment,
    pub notification_queue: Arc<NotificationQueue>,
    pub subscription_storage: Arc<PushNotificationStorage>,
    pub message_processor: MessageProcessor,
    // Background handles for test duration
    _dynamodb_setup: DynamoDbTestSetup,
    _sqs_setup: SqsSetup,
}

impl TestContext {
    /// Create a new test context with the XMTP worker running in the background
    pub async fn new() -> Self {
        setup_test_env();

        let environment = Environment::Development;

        // Initialize DynamoDB and tables
        let dynamodb_client = Arc::new(DynamoDbClient::new(&environment.aws_config().await));
        let dynamodb_test_setup = DynamoDbTestSetup::new(dynamodb_client.clone()).await;
        let subscription_storage = Arc::new(PushNotificationStorage::new(
            dynamodb_client,
            dynamodb_test_setup.push_subscriptions_table_name.clone(),
            dynamodb_test_setup.push_subscription_gsi_name.clone(),
        ));

        // Initialize notification queue
        let sqs_client = Arc::new(SqsClient::new(&environment.aws_config().await));
        let sqs_setup = SqsSetup::new(sqs_client.clone(), "notification-queue").await;
        let notification_queue = Arc::new(NotificationQueue::new(
            sqs_client.clone(),
            QueueConfig {
                queue_url: sqs_setup.queue_url.clone(),
                default_max_messages: 10,
                default_visibility_timeout: 60,
                default_wait_time_seconds: 0,
            },
        ));

        // Create MessageProcessor directly (no background worker needed)
        let message_processor = MessageProcessor::new(
            0, // worker_id
            notification_queue.clone(),
            subscription_storage.clone(),
        );

        Self {
            environment,
            notification_queue,
            subscription_storage,
            message_processor,
            _dynamodb_setup: dynamodb_test_setup,
            _sqs_setup: sqs_setup,
        }
    }
}
