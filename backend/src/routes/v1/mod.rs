mod media;
mod notifications;

use aide::axum::{routing::post, ApiRouter};

/// Creates the v1 API router with all v1 handler routes
pub fn handler() -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/media/presigned-urls",
            post(media::create_presigned_upload_url),
        )
        .api_route("/notifications/subscribe", post(notifications::subscribe))
        .api_route(
            "/notifications/unsubscribe",
            post(notifications::unsubscribe),
        )
}
