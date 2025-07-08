use anyhow::Result;
use axum::Router;
use dotenvy::dotenv;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

use aws_sdk_s3::Client as S3Client;

use backend::{
    image_storage::ImageStorage, routes, state::AppState, types::environment::Environment,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let environment = Environment::from_env();

    let s3_client = Arc::new(S3Client::from_conf(environment.s3_client_config().await));
    let image_storage_client = Arc::new(ImageStorage::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    server::start(environment, image_storage_client).await;
}
