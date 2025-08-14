use std::sync::Arc;

use aws_sdk_s3::Client as S3Client;

use backend::{media_storage::MediaStorage, server, types::Environment};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let environment = Environment::from_env();

    // Initialize Datadog tracing
    // This will set up OpenTelemetry with Datadog exporter
    // The _guard must be kept alive for the duration of the program
    let (_guard, tracer_shutdown) = datadog_tracing::init()?;

    let s3_client = Arc::new(S3Client::from_conf(environment.s3_client_config().await));
    let media_storage = Arc::new(MediaStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    let result = server::start(environment, media_storage).await;

    // Ensure the tracer is properly shut down
    tracer_shutdown.shutdown();

    result
}
