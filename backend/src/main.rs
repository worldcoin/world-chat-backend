use anyhow::Result;
use axum::Router;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

use aws_sdk_s3::Client as S3Client;

use backend::{
    handlers, image_storage::ImageStorageClient, state::AppState, types::environment::Environment,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment configuration
    let environment = Environment::from_env();

    // Initialize tracing
    let log_level = if environment.debug_logging() {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    info!(
        "Starting World Chat Backend Server in {:?} mode",
        environment
    );

    // Configure AWS using environment configuration
    info!("Configuring AWS SDK");
    let s3_config = environment.s3_client_config().await;
    let s3_client = Arc::new(S3Client::from_conf(s3_config));

    // Initialize image storage client
    info!("Initializing image storage client");
    let image_storage_client = Arc::new(ImageStorageClient::new(
        s3_client,
        environment.s3_bucket(),
        environment.presigned_url_expiry_secs(),
    ));

    // Create app state
    let app_state = AppState {
        image_storage_client,
    };

    // Build router
    let app = Router::new()
        .merge(handlers::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
