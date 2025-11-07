use std::sync::Arc;

use aide::openapi::OpenApi;
use axum::Extension;
use backend_storage::auth_proof::AuthProofStorage;
use backend_storage::group_invite::GroupInviteStorage;
use backend_storage::group_join_request::GroupJoinRequestStorage;
use backend_storage::push_subscription::PushSubscriptionStorage;
use datadog_tracing::axum::{shutdown_signal, OtelAxumLayer, OtelInResponseLayer};
use tokio::net::TcpListener;

use crate::enclave_worker_api::EnclaveWorkerApi;
use crate::routes;
use crate::{jwt::JwtManager, media_storage::MediaStorage, types::Environment};

/// Starts the server with the given environment and dependencies
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the port
#[allow(clippy::too_many_arguments)]
pub async fn start(
    environment: Environment,
    media_storage: Arc<MediaStorage>,
    jwt_manager: Arc<JwtManager>,
    auth_proof_storage: Arc<AuthProofStorage>,
    push_subscription_storage: Arc<PushSubscriptionStorage>,
    group_join_request_storage: Arc<GroupJoinRequestStorage>,
    group_invite_storage: Arc<GroupInviteStorage>,
    enclave_worker_api: Arc<dyn EnclaveWorkerApi>,
) -> anyhow::Result<()> {
    let mut openapi = OpenApi::default();

    let router = routes::handler()
        .finish_api(&mut openapi)
        .layer(Extension(openapi))
        .layer(Extension(environment))
        .layer(Extension(media_storage))
        .layer(Extension(jwt_manager))
        .layer(Extension(auth_proof_storage))
        .layer(Extension(push_subscription_storage))
        .layer(Extension(enclave_worker_api))
        .layer(Extension(group_join_request_storage))
        .layer(Extension(group_invite_storage))
        // Include trace context as header into the response
        .route_layer(OtelInResponseLayer)
        // Start OpenTelemetry trace on incoming request
        .route_layer(OtelAxumLayer::default())
        .layer(tower_http::timeout::TimeoutLayer::new(
            std::time::Duration::from_secs(5),
        ));

    let addr = std::net::SocketAddr::from((
        [0, 0, 0, 0],
        std::env::var("PORT").map_or(Ok(8000), |p| p.parse())?,
    ));

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("🔄 World Chat Backend started on http://{addr}");

    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(anyhow::Error::from)
}
