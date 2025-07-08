#![allow(dead_code)]

mod test_router;

pub use test_router::*;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

/// Create a valid 64-character hex image ID for testing
pub fn create_valid_image_id() -> String {
    format!("{:0>64}", Uuid::new_v4().simple())
}

/// Create an invalid image ID for testing validation
pub fn create_invalid_image_id() -> String {
    "invalid_image_id".to_string()
}

/// Create a short invalid image ID
pub fn create_short_image_id() -> String {
    "abc123".to_string()
}

/// Send a POST request with JSON payload (following backup-service pattern)
pub async fn send_post_request(route: &str, payload: serde_json::Value) -> Response {
    let app = get_test_router().await;
    app.oneshot(
        Request::builder()
            .uri(route)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

/// Parse response body to JSON (following backup-service pattern)
pub async fn parse_response_body(response: Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Create test upload request JSON
pub fn create_upload_request(image_id: String, content_length: i64) -> serde_json::Value {
    json!({
        "image_id": image_id,
        "content_length": content_length
    })
}

/// Setup test environment variables
pub fn setup_test_env() {
    // Load test environment variables
    dotenvy::from_path(".env.example").ok();

    // Initialize tracing for tests (following backup-service pattern)
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}
