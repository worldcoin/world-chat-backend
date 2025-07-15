use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use axum::{body::Body, http::Request, response::Response, Extension, Router};
use backend::{media_storage::MediaStorage, routes, types::Environment};
use backend_storage::{
    push_notification::PushNotificationStorage,
    queue::{QueueConfig, SubscriptionRequestQueue},
};
use std::sync::Arc;
use tower::ServiceExt;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Setup test environment variables
pub fn setup_test_env() {
    // Load test environment variables
    // dotenvy::from_path(".env.example").ok();

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}

/// Test Context with real dependencies
pub struct TestContext {
    pub router: Router,
    pub s3_client: Arc<S3Client>,
    pub bucket_name: String,
    pub dynamodb_client: Arc<DynamoDbClient>,
    pub sqs_client: Arc<SqsClient>,
    pub subscription_request_queue_url: String,
    pub push_notification_storage: Arc<PushNotificationStorage>,
    pub subscription_queue: Arc<SubscriptionRequestQueue>,
}

impl TestContext {
    pub async fn new(presign_expiry_override: Option<u64>) -> Self {
        setup_test_env();

        let environment = Environment::Development {
            presign_expiry_override,
        };

        // Configure AWS SDK for LocalStack
        let credentials = Credentials::from_keys(
            "test", // AWS_ACCESS_KEY_ID
            "test", // AWS_SECRET_ACCESS_KEY
            None,   // no session token
        );
        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(LOCALSTACK_ENDPOINT)
            .region(Region::new(TEST_REGION))
            .credentials_provider(credentials)
            .load()
            .await;

        // Init S3 client with path-style addressing for LocalStack
        let s3_config = aws_sdk_s3::Config::from(&aws_config)
            .to_builder()
            .force_path_style(true)
            .build();
        let s3_client = Arc::new(S3Client::from_conf(s3_config));
        let bucket_name = environment.s3_bucket();
        let media_storage = Arc::new(MediaStorage::new(
            s3_client.clone(),
            bucket_name.clone(),
            environment.presigned_url_expiry_secs(),
        ));

        // Init push notification storage
        let dynamodb_client = Arc::new(DynamoDbClient::new(&aws_config));
        let push_notification_storage = Arc::new(PushNotificationStorage::new(
            dynamodb_client.clone(),
            environment.dynamodb_push_table_name(),
            environment.dynamodb_push_gsi_name(),
        ));

        // Init SQS client and create test queue
        let sqs_client = Arc::new(SqsClient::new(&aws_config));

        // Create unique FIFO queue for tests
        let test_queue_name = format!("test-subscription-queue-{}.fifo", uuid::Uuid::new_v4());
        let create_queue_response = sqs_client
            .create_queue()
            .queue_name(&test_queue_name)
            .attributes(aws_sdk_sqs::types::QueueAttributeName::FifoQueue, "true")
            .attributes(
                aws_sdk_sqs::types::QueueAttributeName::ContentBasedDeduplication,
                "true",
            )
            .send()
            .await
            .expect("Failed to create test queue");

        let subscription_request_queue_url = create_queue_response
            .queue_url()
            .expect("Queue URL not returned")
            .to_string();

        // Create subscription queue with test queue URL
        let test_queue_config = QueueConfig {
            queue_url: subscription_request_queue_url.clone(),
            default_max_messages: 10,
            default_visibility_timeout: 30,
            default_wait_time_seconds: 0, // No wait for tests
        };

        let subscription_queue = Arc::new(SubscriptionRequestQueue::new(
            sqs_client.clone(),
            test_queue_config,
        ));

        let router = routes::handler()
            .layer(Extension(environment.clone()))
            .layer(Extension(media_storage.clone()))
            .layer(Extension(push_notification_storage.clone()))
            .layer(Extension(subscription_queue.clone()))
            .into();

        Self {
            router,
            s3_client,
            bucket_name,
            dynamodb_client,
            sqs_client,
            subscription_request_queue_url,
            push_notification_storage,
            subscription_queue,
        }
    }
}

impl TestContext {
    pub async fn send_post_request(
        &self,
        route: &str,
        payload: serde_json::Value,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        let request = Request::builder()
            .uri(route)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(payload.to_string()))?;

        let response = self.router.clone().oneshot(request).await?;
        Ok(response)
    }

    pub async fn parse_response_body(
        &self,
        response: axum::response::Response,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        use http_body_util::BodyExt;

        let body = response.into_body().collect().await?.to_bytes();
        let json = serde_json::from_slice(&body)?;
        Ok(json)
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Clone the clients and URLs we need for cleanup
        let sqs_client = self.sqs_client.clone();
        let queue_url = self.subscription_request_queue_url.clone();

        // Use tokio runtime to delete queue
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = sqs_client.delete_queue().queue_url(&queue_url).send().await;
            });
        }
    }
}
