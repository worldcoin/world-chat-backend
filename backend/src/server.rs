use std::sync::Arc;

use aide::openapi::OpenApi;
use axum::Extension;
use backend_storage::auth_proof::AuthProofStorage;
use datadog_tracing::axum::{shutdown_signal, OtelAxumLayer, OtelInResponseLayer};
use tokio::net::TcpListener;

use crate::routes;
use crate::{jwt::JwtManager, media_storage::MediaStorage, types::Environment};

/// Starts the server with the given environment and dependencies
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the port
pub async fn start(
    environment: Environment,
    media_storage: Arc<MediaStorage>,
    jwt_manager: Arc<JwtManager>,
    auth_proof_storage: Arc<AuthProofStorage>,
) -> anyhow::Result<()> {
    let mut openapi = OpenApi::default();

    let router = routes::handler()
        .finish_api(&mut openapi)
        .layer(Extension(openapi))
        .layer(Extension(environment))
        .layer(Extension(media_storage))
        .layer(Extension(jwt_manager))
        .layer(Extension(auth_proof_storage))
        // Include trace context as header into the response
        .layer(OtelInResponseLayer)
        // Start OpenTelemetry trace on incoming request
        .layer(OtelAxumLayer::default())
        .layer(tower_http::timeout::TimeoutLayer::new(
            std::time::Duration::from_secs(5),
        ));

    let addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT").map_or(Ok(8001), |p| p.parse())?,
    ));

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("🔄 World Chat Backend started on http://{addr}");

    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(anyhow::Error::from)
}
