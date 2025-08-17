use std::sync::Arc;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use backend_storage::auth_proof::AuthProofStorage;

use backend::{jwt::JwtManager, media_storage::MediaStorage, server, types::Environment};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let environment = Environment::from_env();

    // Initialize Datadog tracing
    // This will set up OpenTelemetry with Datadog exporter
    // The _guard must be kept alive for the duration of the program
    let (_guard, tracer_shutdown) = datadog_tracing::init()?;

    // Initialize JWT manager with cached secret (loaded once at startup)
    let secrets_manager_client = SecretsManagerClient::new(&environment.aws_config().await);
    let jwt_manager = Arc::new(JwtManager::new(secrets_manager_client, &environment).await);

    // Initialize S3 client and media storage
    let s3_client = Arc::new(S3Client::from_conf(environment.s3_client_config().await));
    let media_storage = Arc::new(MediaStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    // Initialize DynamoDB client and auth proof storage
    let dynamodb_client = Arc::new(DynamoDbClient::new(&environment.aws_config().await));
    let auth_proof_storage = Arc::new(AuthProofStorage::new(
        dynamodb_client,
        environment.dynamodb_auth_table_name(),
    ));

    let result = server::start(environment, media_storage, jwt_manager, auth_proof_storage).await;

    // Ensure the tracer is properly shut down
    tracer_shutdown.shutdown();

    result
}
