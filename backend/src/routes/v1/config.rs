use axum::{Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    routes::v1::media::{MAX_ASSETS_PER_MESSAGE, MAX_IMAGE_SIZE_BYTES, MAX_VIDEO_SIZE_BYTES},
    types::Environment,
};

#[derive(Serialize, JsonSchema)]
pub struct ConfigResponse {
    /// Maximum count of assets per message
    max_assets_per_message: usize,
    /// Maximum image size in bytes
    max_image_size_bytes: i64,
    /// Maximum video size in bytes
    max_video_size_bytes: i64,
    /// Trusted CDN URL
    trusted_cdn_url: String,
    /// Notification server version
    /// Clients use this to resubscribe push notifications when the enclave's public key is rotated
    notification_server_version: String,
}

pub async fn get_config(Extension(environment): Extension<Environment>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        max_assets_per_message: MAX_ASSETS_PER_MESSAGE,
        max_image_size_bytes: MAX_IMAGE_SIZE_BYTES,
        max_video_size_bytes: MAX_VIDEO_SIZE_BYTES,
        trusted_cdn_url: environment.cdn_url(),
        notification_server_version: "v2".to_string(),
    })
}
