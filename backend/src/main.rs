use std::sync::Arc;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use backend_storage::auth_proof::AuthProofStorage;

use backend::{media_storage::MediaStorage, server, types::Environment};
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

    let dynamodb_client = Arc::new(DynamoDbClient::from_conf(
        environment.dynamodb_client_config().await,
    ));
    let auth_proof_storage = Arc::new(AuthProofStorage::new(
        dynamodb_client,
        environment.dynamodb_auth_table_name(),
    ));

    server::start(environment, media_storage, auth_proof_storage).await
}
