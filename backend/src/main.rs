use std::sync::Arc;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;

use backend::{media_storage::MediaStorage, server, types::Environment};
use backend_storage::{
    push_notification::PushNotificationStorage, queue::SubscriptionRequestQueue,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let environment = Environment::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let s3_client = Arc::new(S3Client::from_conf(environment.s3_client_config().await));
    let media_storage = Arc::new(MediaStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    let sqs_client = Arc::new(SqsClient::from_conf(environment.sqs_client_config().await));
    let subscription_queue = Arc::new(SubscriptionRequestQueue::new(
        sqs_client,
        environment.subscription_queue_config(),
    ));

    let dynamodb_client = Arc::new(DynamoDbClient::from_conf(
        environment.dynamodb_client_config().await,
    ));
    let push_notification_storage = Arc::new(PushNotificationStorage::new(
        dynamodb_client,
        environment.dynamodb_push_table_name(),
        environment.dynamodb_push_gsi_name(),
    ));

    server::start(
        environment,
        media_storage,
        push_notification_storage,
        subscription_queue,
    )
    .await
}
