mod dynamodb_setup;
use backend_storage::push_notification::PushNotificationStorage;
use dynamodb_setup::DynamoDbTestSetup;

use std::sync::Arc;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_sqs::Client as SqsClient;
use tokio::task::JoinHandle;
use tracing::{error, info};

use backend_storage::queue::NotificationQueue;
use notification_worker::types::environment::Environment;
use notification_worker::worker::XmtpWorker;

/// Setup test environment variables with all the required configuration
pub fn setup_test_env() {
    // Load test environment variables
    dotenvy::from_path(".env.example").ok();

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}

/// Base test setup with core dependencies and XMTP worker
#[allow(dead_code)]
pub struct TestContext {
    pub environment: Environment,
    pub notification_queue: Arc<NotificationQueue>,
    pub subscription_storage: Arc<PushNotificationStorage>,
    // Worker management (optional - just for aborting if needed)
    worker_handle: JoinHandle<()>,
    // Keep DynamoDbTestSetup alive for the duration of the test
    _dynamodb_setup: DynamoDbTestSetup,
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
        let notification_queue = Arc::new(NotificationQueue::new(
            sqs_client,
            environment.notification_queue_config(),
        ));

        // Create the worker - panic if initialization fails
        let worker = XmtpWorker::new(
            environment.clone(),
            notification_queue.clone(),
            subscription_storage.clone(),
        )
        .await
        .expect("Failed to create XMTP worker - test cannot proceed");

        info!("Successfully created XMTP worker for tests");

        // Spawn the worker in the background
        let worker_handle = tokio::spawn(async move {
            if let Err(e) = worker.start().await {
                panic!("Worker encountered error during test: {}", e);
            }
        });

        // Give the worker a moment to establish connections
        //TODO: benchmark this
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Self {
            environment,
            notification_queue,
            subscription_storage,
            worker_handle,
            _dynamodb_setup: dynamodb_test_setup,
        }
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.worker_handle.abort();
    }
}
