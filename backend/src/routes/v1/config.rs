use axum::{http::HeaderMap, Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;
use std::cmp::Ordering;

use crate::{
    routes::v1::media::{MAX_ASSETS_PER_MESSAGE, MAX_IMAGE_SIZE_BYTES, MAX_VIDEO_SIZE_BYTES},
    types::Environment,
};

/// Client platform extracted from headers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientPlatform {
    Ios,
    Android,
    Unknown,
}

/// Parsed semantic version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a version string like "1.2.3" or "1.2"
    #[must_use]
    pub fn parse(version_str: &str) -> Option<Self> {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }

        let major = parts.first().and_then(|s| s.parse().ok())?;
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Check if this version is at least the given version
    #[must_use]
    pub fn is_at_least(&self, other: &Self) -> bool {
        match self.cmp(other) {
            Ordering::Greater | Ordering::Equal => true,
            Ordering::Less => false,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Client info extracted from request headers
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub platform: ClientPlatform,
    pub version: Option<Version>,
}

impl ClientInfo {
    /// Extract client info from request headers
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let platform = headers
            .get("client-name")
            .and_then(|v| v.to_str().ok())
            .map_or(ClientPlatform::Unknown, |s| {
                match s.to_lowercase().as_str() {
                    "ios" => ClientPlatform::Ios,
                    "android" => ClientPlatform::Android,
                    _ => ClientPlatform::Unknown,
                }
            });

        let version = headers
            .get("client-version")
            .and_then(|v| v.to_str().ok())
            .and_then(Version::parse);

        Self { platform, version }
    }

    /// Check if client version is at least the specified version
    #[must_use]
    pub fn version_is_at_least(&self, major: u32, minor: u32, patch: u32) -> bool {
        self.version.is_some_and(|v| {
            v.is_at_least(&Version {
                major,
                minor,
                patch,
            })
        })
    }
}

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

pub async fn get_config(
    headers: HeaderMap,
    Extension(environment): Extension<Environment>,
) -> Json<ConfigResponse> {
    let client = ClientInfo::from_headers(&headers);

    let notification_server_version = if client.version_is_at_least(4, 0, 0) {
        "v5"
    } else {
        "v1"
    }
    .to_string();

    Json(ConfigResponse {
        max_assets_per_message: MAX_ASSETS_PER_MESSAGE,
        max_image_size_bytes: MAX_IMAGE_SIZE_BYTES,
        max_video_size_bytes: MAX_VIDEO_SIZE_BYTES,
        trusted_cdn_url: environment.cdn_url(),
        notification_server_version,
    })
}
