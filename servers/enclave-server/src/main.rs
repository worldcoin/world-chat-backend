use anyhow::Result;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting Enclave Server");

    // Build service router
    let enclave_router = enclave_service::build_http_router().await;

    // Create app
    let app = Router::new()
        .nest("/", enclave_router)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // Start server on different port
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;
    info!("Enclave server listening on {}", listener.local_addr()?);
    
    axum::serve(listener, app).await?;

    Ok(())
}