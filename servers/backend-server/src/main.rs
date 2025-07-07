use anyhow::Result;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting World Chat Backend Server");

    // Build service routers
    let notification_router = notification_service::build_http_router().await;
    let image_router = image_service::build_http_router().await;

    // Compose all services
    let app = Router::new()
        .nest("/notifications", notification_router)
        .nest("/images", image_router)
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
