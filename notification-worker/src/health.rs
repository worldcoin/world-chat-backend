use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use datadog_tracing::axum::{OtelAxumLayer, OtelInResponseLayer};
use serde_json::json;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Simple health check endpoint
///
/// Returns 200 OK for now. In the future, this will check:
/// - gRPC stream connectivity
/// - SQS connectivity  
/// - Worker thread status
///
/// Returns 503 if any critical component is down
async fn health() -> impl IntoResponse {
    // TODO: Add actual health checks later:
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "service": "notification-worker",
        })),
    )
}

/// Start the health check HTTP server
///
/// # Errors
///
/// Returns an error if the server fails to bind to the specified address
pub async fn start_health_server(shutdown_token: CancellationToken) -> anyhow::Result<()> {
    let app = Router::new().route("/health", get(health));
    // Include trace context as header into the response
    // .layer(OtelInResponseLayer)
    // // Start OpenTelemetry trace on incoming request
    // .layer(OtelAxumLayer::default());

    let addr = SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT").map_or(Ok(8001), |p| p.parse())?,
    ));
    let listener = TcpListener::bind(addr).await?;
    info!("Health check server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_token.cancelled().await;
        })
        .await?;

    Ok(())
}
