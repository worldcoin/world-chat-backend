#[path = "../common/mod.rs"]
mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

// Happy path tests (following backup-service pattern)

#[tokio::test]
async fn test_upload_media_happy_path() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1024);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = parse_response_body(response).await;
    assert!(body["presigned_url"].is_string());
    assert!(body["expires_at"].is_string());

    let presigned_url = body["presigned_url"].as_str().unwrap();
    assert!(presigned_url.contains("localhost:4566")); // LocalStack URL
}

#[tokio::test]
async fn test_upload_media_valid_content_lengths() {
    // Test various valid content lengths
    let test_cases = vec![1, 1024, 1_048_576, 15_728_640]; // 1 byte to 15 MiB

    for content_length in test_cases {
        let content_digest_sha256 = create_valid_sha256();
        let payload = create_upload_request(content_digest_sha256, content_length);

        let response = send_post_request("/v1/media/presigned-urls", payload).await;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Failed for content_length: {}",
            content_length
        );
    }
}

// Validation error tests (schemars validation - expect 400 instead of custom errors)

#[tokio::test]
async fn test_upload_media_invalid_sha256_format() {
    let payload = create_upload_request(create_invalid_sha256(), 1024);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    // With schemars, we get 400 BAD_REQUEST instead of custom error structure
}

#[tokio::test]
async fn test_upload_media_short_sha256() {
    let payload = create_upload_request(create_short_sha256(), 1024);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_too_large() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 15_728_641); // 15 MiB + 1 byte

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_zero() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 0);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_negative() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, -1);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Malformed request tests

#[tokio::test]
async fn test_upload_media_missing_sha256() {
    let payload = json!({
        "content_length": 1024
        // Missing content_digest_sha256
    });

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_missing_content_length() {
    let payload = json!({
        "content_digest_sha256": create_valid_sha256()
        // Missing content_length
    });

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_empty_json() {
    let payload = json!({});

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_invalid_json_types() {
    let payload = json!({
        "content_digest_sha256": 12345, // Should be string
        "content_length": "invalid" // Should be number
    });

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Edge case tests following backup-service patterns

#[tokio::test]
async fn test_upload_media_exact_max_content_length() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 15_728_640); // Exactly 15 MiB

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_minimum_content_length() {
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1); // Minimum allowed

    let response = send_post_request("/v1/media/presigned-urls", payload).await;
    let status = response.status().clone();
    let body = parse_response_body(response).await;

    println!("{:?}", content_digest_sha256);
    println!("{:?}", body);

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_special_hex_characters() {
    // Test with all valid hex characters
    let content_digest_sha256 = "abcdef0123456789".repeat(4); // 64 chars of valid hex
    let payload = create_upload_request(content_digest_sha256, 1024);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_uppercase_hex() {
    // Test uppercase hex (should be rejected by schemars regex)
    let content_digest_sha256 = "ABCDEF0123456789".repeat(4); // 64 chars of uppercase hex
    let payload = create_upload_request(content_digest_sha256, 1024);

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_extra_fields() {
    // Test schemars deny_unknown_fields
    let payload = json!({
        "content_digest_sha256": create_valid_sha256(),
        "content_length": 1024,
        "extra_field": "should_be_rejected"
    });

    let response = send_post_request("/v1/media/presigned-urls", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
