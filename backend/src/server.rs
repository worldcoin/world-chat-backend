use std::sync::Arc;

use aide::openapi::OpenApi;
use axum::Extension;
use tokio::net::TcpListener;
use tracing::Level;

use crate::routes;
use crate::{media_storage::MediaStorage, types::Environment};

/// Starts the server with the given environment and image storage
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the port
#[allow(clippy::too_many_arguments)] // logical module separation is preferred
pub async fn start(
    environment: Environment,
    media_storage: Arc<MediaStorage>,
) -> anyhow::Result<()> {
    let mut openapi = OpenApi::default();

    let router = routes::handler()
        .finish_api(&mut openapi)
        .layer(Extension(environment))
        .layer(Extension(media_storage))
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(tower_http::timeout::TimeoutLayer::new(
            std::time::Duration::from_secs(30),
        ));

    let addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT").map_or(Ok(8000), |p| p.parse())?,
    ));

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("🔄 World Chat Backend started on http://{addr}");

    axum::serve(listener, router.into_make_service())
        .await
        .map_err(anyhow::Error::from)
}
