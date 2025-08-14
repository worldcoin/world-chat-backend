use std::sync::Arc;

use aws_sdk_s3::Client as S3Client;

use backend::{media_storage::MediaStorage, server, types::Environment};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let environment = Environment::from_env();

    // Configure logging format based on environment
    // Use JSON format for staging/production (Datadog), regular format for development
    match environment {
        Environment::Production | Environment::Staging => {
            fmt()
                .json()
                .with_env_filter(EnvFilter::from_default_env())
                .init();
        }
        Environment::Development { .. } => {
            fmt().with_env_filter(EnvFilter::from_default_env()).init();
        }
    }

    let s3_client = Arc::new(S3Client::from_conf(environment.s3_client_config().await));
    let media_storage = Arc::new(MediaStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    server::start(environment, media_storage).await
}
