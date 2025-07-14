//! Queue test setup utilities

#![allow(dead_code)]

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_sqs::Client as SqsClient;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use uuid::Uuid;

/// Generic helper function to assert queue messages match
pub fn assert_queue_message<T>(received: &backend_storage::queue::QueueMessage<T>, expected: &T)
where
    T: PartialEq + std::fmt::Debug,
{
    assert_eq!(
        received.body, *expected,
        "Queue message content should match"
    );
}

/// Test context that provides SQS client and queue setup
pub struct QueueTestContext {
    pub sqs_client: Arc<SqsClient>,
    pub queue_url: String,
}

impl QueueTestContext {
    /// Creates a new test context with a unique FIFO queue
    pub async fn new(test_name: &str) -> Self {
        // Create unique queue name
        let queue_name = format!("{}-{}.fifo", test_name, Uuid::new_v4());

        // Setup LocalStack client with hardcoded credentials for CI
        let credentials = Credentials::from_keys(
            "test", // AWS_ACCESS_KEY_ID
            "test", // AWS_SECRET_ACCESS_KEY
            None,   // no session token
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url("http://localhost:4566")
            .credentials_provider(credentials)
            .load()
            .await;

        let sqs_client = Arc::new(SqsClient::new(&config));

        // Create FIFO queue with content-based deduplication
        let result = sqs_client
            .create_queue()
            .queue_name(&queue_name)
            .attributes(aws_sdk_sqs::types::QueueAttributeName::FifoQueue, "true")
            .attributes(
                aws_sdk_sqs::types::QueueAttributeName::ContentBasedDeduplication,
                "true",
            )
            .send()
            .await
            .expect("Failed to create test queue");

        let queue_url = result
            .queue_url()
            .expect("Queue URL not returned")
            .to_string();

        Self {
            sqs_client,
            queue_url,
        }
    }
}

impl Drop for QueueTestContext {
    fn drop(&mut self) {
        // Clean up the queue
        let client = self.sqs_client.clone();
        let queue_url = self.queue_url.clone();

        // Use tokio runtime to delete queue
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = client.delete_queue().queue_url(&queue_url).send().await;
            });
        }
    }
}
