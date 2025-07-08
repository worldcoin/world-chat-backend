use anyhow::Result;
use axum::Router;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

use backend::{
    bucket::BucketClient, 
    handlers,
    state::AppState,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting World Chat Backend Server");

    // Initialize bucket client
    info!("Initializing S3 bucket client");
    let bucket_client = Arc::new(BucketClient::new().await?);

    // Create app state
    let app_state = AppState {
        bucket_client,
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