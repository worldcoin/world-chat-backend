mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

// Happy path tests (following backup-service pattern)

#[tokio::test]
async fn test_upload_image_happy_path() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id.clone(), 1024);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = parse_response_body(response).await;
    assert!(body["presigned_url"].is_string());
    assert!(body["expires_at"].is_string());

    let presigned_url = body["presigned_url"].as_str().unwrap();
    assert!(presigned_url.contains(&image_id));
    assert!(presigned_url.contains("localhost:4566")); // LocalStack URL
}

#[tokio::test]
async fn test_upload_image_valid_content_lengths() {
    // Test various valid content lengths
    let test_cases = vec![1, 1024, 1_048_576, 15_728_640]; // 1 byte to 15 MiB

    for content_length in test_cases {
        let image_id = create_valid_image_id();
        let payload = create_upload_request(image_id, content_length);

        let response = send_post_request("/v1/images/upload", payload).await;
        
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Failed for content_length: {}",
            content_length
        );
    }
}

// Validation error tests (following backup-service error patterns)

#[tokio::test]
async fn test_upload_image_invalid_image_id_format() {
    let payload = create_upload_request(create_invalid_image_id(), 1024);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = parse_response_body(response).await;
    assert_eq!(body["allowRetry"], false);
    assert_eq!(body["error"]["code"], "invalid_image_id");
    assert_eq!(
        body["error"]["message"],
        "Image ID must be a 64-character hexadecimal string"
    );
}

#[tokio::test]
async fn test_upload_image_short_image_id() {
    let payload = create_upload_request(create_short_image_id(), 1024);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = parse_response_body(response).await;
    assert_eq!(body["error"]["code"], "invalid_image_id");
}

#[tokio::test]
async fn test_upload_image_content_length_too_large() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id, 15_728_641); // 15 MiB + 1 byte

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = parse_response_body(response).await;
    assert_eq!(body["allowRetry"], false);
    assert_eq!(body["error"]["code"], "payload_too_large");
    assert_eq!(
        body["error"]["message"],
        "Content length exceeds maximum allowed size"
    );
}

#[tokio::test]
async fn test_upload_image_content_length_zero() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id, 0);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = parse_response_body(response).await;
    assert_eq!(body["error"]["code"], "payload_too_large");
}

#[tokio::test]
async fn test_upload_image_content_length_negative() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id, -1);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = parse_response_body(response).await;
    assert_eq!(body["error"]["code"], "payload_too_large");
}

// Malformed request tests

#[tokio::test]
async fn test_upload_image_missing_image_id() {
    let payload = json!({
        "content_length": 1024
        // Missing image_id
    });

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_image_missing_content_length() {
    let payload = json!({
        "image_id": create_valid_image_id()
        // Missing content_length
    });

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_image_empty_json() {
    let payload = json!({});

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_image_invalid_json_types() {
    let payload = json!({
        "image_id": 12345, // Should be string
        "content_length": "invalid" // Should be number
    });

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Edge case tests following backup-service patterns

#[tokio::test]
async fn test_upload_image_exact_max_content_length() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id, 15_728_640); // Exactly 15 MiB

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_image_minimum_content_length() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id, 1); // Minimum allowed

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_image_special_hex_characters() {
    // Test with all valid hex characters
    let image_id = "abcdef0123456789".repeat(4); // 64 chars of valid hex
    let payload = create_upload_request(image_id, 1024);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_image_uppercase_hex_should_fail() {
    // Image ID should be lowercase hex only
    let image_id = "ABCDEF".to_string() + &"0".repeat(58);
    let payload = create_upload_request(image_id, 1024);

    let response = send_post_request("/v1/images/upload", payload).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    let body = parse_response_body(response).await;
    assert_eq!(body["error"]["code"], "invalid_image_id");
}

// Test for duplicate upload (object already exists)
// Note: This test demonstrates the pattern but won't work without proper mocking
// since we can't easily simulate an existing object in LocalStack between tests
#[tokio::test]
#[ignore = "Requires mock implementation or state management between requests"]
async fn test_upload_image_object_already_exists() {
    let image_id = create_valid_image_id();
    let payload = create_upload_request(image_id.clone(), 1024);

    // First upload should succeed
    let response1 = send_post_request("/v1/images/upload", payload.clone()).await;
    assert_eq!(response1.status(), StatusCode::OK);
    
    // In a real scenario with mocking, the second upload would fail with CONFLICT
    // For now, this is commented out as it requires mock implementation
    /*
    let response2 = send_post_request("/v1/images/upload", payload).await;
    assert_eq!(response2.status(), StatusCode::CONFLICT);
    
    let body = parse_response_body(response2).await;
    assert_eq!(body["allowRetry"], false);
    assert_eq!(body["error"]["code"], "already_exists");
    */
}