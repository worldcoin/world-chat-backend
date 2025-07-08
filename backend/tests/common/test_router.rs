use aws_sdk_s3::Client as S3Client;
use axum::Extension;
use backend::{image_storage::ImageStorage, routes, types::Environment};
use std::sync::Arc;

/// Get test router with real dependencies (following backup-service pattern)
pub async fn get_test_router() -> axum::Router {
    super::setup_test_env();

    // Use development environment for tests
    let environment = Environment::Development;

    // Configure AWS using environment
    let s3_config = environment.s3_client_config().await;
    let s3_client = Arc::new(S3Client::from_conf(s3_config));

    // Create image storage client
    let image_storage_client = Arc::new(ImageStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    // Use Extension pattern like server.rs
    routes::handler()
        .layer(Extension(environment))
        .layer(Extension(image_storage_client))
        .into()
}
