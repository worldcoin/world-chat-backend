#![allow(dead_code)]

mod test_router;

pub use test_router::*;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

/// Create a valid 64-character hex SHA256 digest for testing
pub fn create_valid_sha256() -> String {
    // Concatenate two UUIDs to get 64 hex characters (32 + 32)
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Create an invalid SHA256 digest for testing validation
pub fn create_invalid_sha256() -> String {
    "invalid_sha256_digest".to_string()
}

/// Create a short invalid SHA256 digest
pub fn create_short_sha256() -> String {
    "abc123".to_string()
}

/// Send a POST request with JSON payload
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

/// Parse response body to JSON
pub async fn parse_response_body(response: Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Create test upload request JSON
pub fn create_upload_request(
    content_digest_sha256: String,
    content_length: i64,
) -> serde_json::Value {
    json!({
        "content_digest_sha256": content_digest_sha256,
        "content_length": content_length
    })
}

/// Assert that response has expected status code
pub fn assert_status(response: &Response, expected: StatusCode) {
    assert_eq!(
        response.status(),
        expected,
        "Expected status {}, got {}",
        expected,
        response.status()
    );
}

/// Assert that response is a validation error (schemars format)
pub async fn assert_validation_error(response: Response) {
    assert_status(&response, StatusCode::BAD_REQUEST);
    // With schemars, we just expect 400 status - no specific error structure
}

/// Assert that response is a successful upload response
pub async fn assert_upload_success(response: Response) -> serde_json::Value {
    assert_status(&response, StatusCode::OK);
    let body = parse_response_body(response).await;

    assert!(body["presigned_url"].is_string());
    assert!(body["expires_at"].is_string());
    assert!(body["asset_id"].is_string());

    body
}

/// Setup test environment variables
pub fn setup_test_env() {
    // Load test environment variables
    dotenvy::from_path(".env.example").ok();

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}
