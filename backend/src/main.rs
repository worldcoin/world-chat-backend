use anyhow::Result;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::info;

mod handlers;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting World Chat Backend Server");

    // Build router
    let app = Router::new()
        .merge(handlers::routes())
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
