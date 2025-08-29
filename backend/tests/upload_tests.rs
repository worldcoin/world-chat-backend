mod common;

use common::*;

use http::StatusCode;
use serde_json::json;

pub fn create_upload_request(
    content_digest_sha256: String,
    content_length: i64,
    content_type: Option<String>,
) -> serde_json::Value {
    let mut request = json!({
        "content_digest_sha256": content_digest_sha256,
        "content_length": content_length
    });

    if let Some(mime) = content_type {
        request["content_type"] = json!(mime);
    } else {
        // Default to an image mime type
        request["content_type"] = json!("image/png");
    }

    request
}

#[tokio::test]
async fn test_config_enforced_max_image_size_plus_one_fails() {
    let setup = TestSetup::new(None).await;

    // Fetch media config
    let response = setup
        .send_get_request("/v1/media/config")
        .await
        .expect("Failed to send GET /v1/media/config");
    assert_eq!(response.status(), StatusCode::OK);
    let body = parse_response_body(response).await;

    let max_image_size_bytes = body["max_image_size_bytes"]
        .as_i64()
        .expect("max_image_size_bytes should be an integer");

    // Prepare an upload one byte larger than allowed
    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        max_image_size_bytes + 1,
        Some("image/png".to_string()),
    );

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send POST /v1/media/presigned-urls");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Happy path tests

#[tokio::test]
async fn test_upload_media_happy_path() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1024, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body = parse_response_body(response).await;
    assert!(body["presigned_url"].is_string());
    assert!(body["asset_url"].is_string());
    assert!(body["content_digest_base64"].is_string());

    let presigned_url = body["presigned_url"].as_str().unwrap();
    assert!(presigned_url.contains("localhost:4566")); // LocalStack URL

    let asset_url = body["asset_url"].as_str().unwrap();
    assert!(asset_url.starts_with("http://localhost:4566/world-chat-media/")); // LocalStack CDN URL
}

// Validation error tests (schemars validation - expect 400 instead of custom errors)

#[tokio::test]
async fn test_upload_media_invalid_sha256_format() {
    let setup = TestSetup::new(None).await;

    let payload = create_upload_request("invalid_sha256_digest".to_string(), 1024, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_short_sha256() {
    let setup = TestSetup::new(None).await;

    let payload = create_upload_request("abc123".to_string(), 1024, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_too_large() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 15_728_641, None); // 15 MiB + 1 byte

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_zero() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 0, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_negative() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, -1, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Malformed request tests

#[tokio::test]
async fn test_upload_media_missing_sha256() {
    let setup = TestSetup::new(None).await;

    let payload = json!({
        "content_length": 1024,
        "content_type": "image/png"
        // Missing content_digest_sha256
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_missing_content_length() {
    let setup = TestSetup::new(None).await;

    let payload = json!({
        "content_digest_sha256": create_valid_sha256(),
        "content_type": "image/png"
        // Missing content_length
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_empty_json() {
    let setup = TestSetup::new(None).await;

    let payload = json!({});

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_invalid_json_types() {
    let setup = TestSetup::new(None).await;

    let payload = json!({
        "content_digest_sha256": 12345, // Should be string
        "content_length": "invalid", // Should be number
        "content_type": "image/png"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// MIME type validation tests

#[tokio::test]
async fn test_upload_media_invalid_content_type_text() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        1024,
        Some("text/plain".to_string()), // Not an allowed mime type
    );

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_popular_content_types() {
    let setup = TestSetup::new(None).await;

    // Test various allowed MIME types
    let valid_content_types = vec![
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
        "image/heic",
        "image/heif",
        "image/tiff",
        "video/mp4",
        "video/quicktime",
        "video/webm",
        "video/x-matroska",
        "video/avi",
    ];

    for content_type in valid_content_types {
        let content_digest_sha256 = create_valid_sha256();
        let payload = create_upload_request(
            content_digest_sha256,
            1_048_576, // 1 MB
            Some(content_type.to_string()),
        );

        let response = setup
            .send_post_request("/v1/media/presigned-urls", payload)
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Failed for content_type: {}",
            content_type
        );
    }
}

// Edge case tests

#[tokio::test]
async fn test_upload_media_video_exact_max_content_length() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        15_728_640,
        Some("video/mp4".to_string()),
    ); // Exactly 15 MiB

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_video_too_large() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        15_728_641,
        Some("video/mp4".to_string()),
    ); // 15 MiB + 1 byte

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_image_exact_max_content_length() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        5_242_880,
        Some("image/png".to_string()),
    ); // Exactly 5 MiB

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_image_too_large() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(
        content_digest_sha256,
        5_242_881,
        Some("image/png".to_string()),
    ); // 5 MiB + 1 byte

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_minimum_content_length() {
    let setup = TestSetup::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1, None); // Minimum allowed

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_special_hex_characters() {
    let setup = TestSetup::new(None).await;

    // Test with all valid hex characters
    let content_digest_sha256 = "abcdef0123456789".repeat(4); // 64 chars of valid hex
    let payload = create_upload_request(content_digest_sha256, 1024, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_uppercase_hex() {
    let setup = TestSetup::new(None).await;

    // Test uppercase hex (should be rejected by schemars regex)
    let content_digest_sha256 = "ABCDEF0123456789".repeat(4); // 64 chars of uppercase hex
    let payload = create_upload_request(content_digest_sha256, 1024, None);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_extra_fields() {
    let setup = TestSetup::new(None).await;

    // Test schemars deny_unknown_fields
    let payload = json!({
        "content_digest_sha256": create_valid_sha256(),
        "content_length": 1024,
        "content_type": "image/png",
        "extra_field": "should_be_rejected"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Testing e2e upload flows

#[tokio::test]
async fn test_e2e_upload_happy_path() {
    let setup = TestSetup::new(None).await;

    // Step 1: Generate test image data with known SHA-256
    let (image_data, sha256) = generate_test_encrypted_image(2048);
    println!(
        "Generated test image: {} bytes, SHA-256: {}",
        image_data.len(),
        sha256
    );

    // Step 2: Request presigned URL from the API endpoint
    let upload_request = serde_json::json!({
        "content_digest_sha256": sha256,
        "content_length": image_data.len(),
        "content_type": "image/png"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", upload_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK for presigned URL request"
    );

    let response_body = setup
        .parse_response_body(response)
        .await
        .expect("Failed to parse response body");

    println!(
        "API Response: {}",
        serde_json::to_string_pretty(&response_body).unwrap()
    );

    // Extract response fields
    let presigned_url = response_body["presigned_url"]
        .as_str()
        .expect("Missing presigned_url in response");
    let content_digest_base64 = response_body["content_digest_base64"]
        .as_str()
        .expect("Missing content_digest_base64 in response");
    let asset_url = response_body["asset_url"]
        .as_str()
        .expect("Missing asset_url in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(
        !content_digest_base64.is_empty(),
        "Content digest base64 should not be empty"
    );
    assert!(
        asset_url.starts_with("http://localhost:4566/world-chat-media/"),
        "Asset URL should start with LocalStack CDN URL"
    );

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset URL: {}", asset_url);
    println!("Content Digest Base64: {}", content_digest_base64);

    // Step 3: Upload image to S3 using the presigned URL with checksum headers
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        "image/png",
        content_digest_base64,
    )
    .await
    .expect("Failed to upload to S3");

    assert!(
        upload_response.status().is_success(),
        "S3 upload failed with status: {}",
        upload_response.status()
    );

    println!("Successfully uploaded to S3");

    // Step 4: Download image from asset URL using HTTP
    let downloaded_data = download_from_asset_url(asset_url)
        .await
        .expect("Failed to download from asset URL");

    println!("Downloaded {} bytes from asset URL", downloaded_data.len());

    // Step 5: Verify data integrity by comparing checksums
    assert_eq!(
        image_data.len(),
        downloaded_data.len(),
        "Downloaded data size mismatch"
    );

    let downloaded_sha256 = calculate_sha256(&downloaded_data);
    assert_eq!(
        sha256, downloaded_sha256,
        "SHA-256 mismatch: expected {}, got {}",
        sha256, downloaded_sha256
    );

    assert!(image_data == downloaded_data, "Data integrity check failed");

    println!("✅ Data integrity verified!");

    // Step 6: Test deduplication - second call with same SHA-256 should return 409 Conflict
    let duplicate_request = serde_json::json!({
        "content_digest_sha256": sha256,
        "content_length": image_data.len(),
        "content_type": "image/png"
    });

    let duplicate_response = setup
        .send_post_request("/v1/media/presigned-urls", duplicate_request)
        .await
        .expect("Failed to send duplicate request");

    assert_eq!(
        duplicate_response.status(),
        StatusCode::CONFLICT,
        "Expected 409 Conflict for duplicate SHA-256"
    );

    let duplicate_response_body = setup
        .parse_response_body(duplicate_response)
        .await
        .expect("Failed to parse response body");

    assert_eq!(
        duplicate_response_body["asset_url"], asset_url,
        "Expected asset_url to be the same as the original"
    );

    println!("✅ Deduplication works correctly (409 Conflict)");

    println!("🎉 E2E upload happy path test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_wrong_checksum() {
    let setup = TestSetup::new(None).await;

    // Step 1: Generate test image data with known SHA-256
    let (image_data, sha256) = generate_test_encrypted_image(2048);
    println!(
        "Generated test image: {} bytes, SHA-256: {}",
        image_data.len(),
        sha256
    );

    // Step 2: Request presigned URL from the API endpoint
    let upload_request = serde_json::json!({
        "content_digest_sha256": sha256,
        "content_length": image_data.len(),
        "content_type": "image/png"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", upload_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK for presigned URL request"
    );

    let response_body = setup
        .parse_response_body(response)
        .await
        .expect("Failed to parse response body");

    println!(
        "API Response: {}",
        serde_json::to_string_pretty(&response_body).unwrap()
    );

    // Extract response fields
    let presigned_url = response_body["presigned_url"]
        .as_str()
        .expect("Missing presigned_url in response");
    let asset_url = response_body["asset_url"]
        .as_str()
        .expect("Missing asset_url in response");
    let _content_digest_base64 = response_body["content_digest_base64"]
        .as_str()
        .expect("Missing content_digest_base64 in response"); // Intentionally unused - we use a wrong checksum

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(
        asset_url.starts_with("http://localhost:4566/world-chat-media/"),
        "Asset URL should start with LocalStack CDN URL"
    );

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset URL: {}", asset_url);

    // Step 3: Upload image to S3 using the presigned URL with WRONG checksum
    // Generate a different checksum to simulate integrity check failure
    // This is intentionally wrong to test failure case
    let wrong_checksum_b64 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="; // Invalid base64 checksum
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        "image/png",
        wrong_checksum_b64, // Wrong checksum should cause upload to fail
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist at asset URL
    let file_exists = asset_exists_at_url(asset_url)
        .await
        .expect("Failed to check if file exists at URL");

    assert!(!file_exists, "File should not exist at asset URL");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with wrong checksum test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_wrong_content_length() {
    let setup = TestSetup::new(None).await;

    // Step 1: Generate test image data with known SHA-256
    let (image_data, sha256) = generate_test_encrypted_image(2048);
    println!(
        "Generated test image: {} bytes, SHA-256: {}",
        image_data.len(),
        sha256
    );

    // Step 2: Request presigned URL from the API endpoint
    let upload_request = serde_json::json!({
        "content_digest_sha256": sha256,
        "content_length": image_data.len(),
        "content_type": "image/png"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", upload_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK for presigned URL request"
    );

    let response_body = setup
        .parse_response_body(response)
        .await
        .expect("Failed to parse response body");

    println!(
        "API Response: {}",
        serde_json::to_string_pretty(&response_body).unwrap()
    );

    // Extract response fields
    let presigned_url = response_body["presigned_url"]
        .as_str()
        .expect("Missing presigned_url in response");
    let asset_url = response_body["asset_url"]
        .as_str()
        .expect("Missing asset_url in response");
    let content_digest_base64 = response_body["content_digest_base64"]
        .as_str()
        .expect("Missing content_digest_base64 in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(
        asset_url.starts_with("http://localhost:4566/world-chat-media/"),
        "Asset URL should start with LocalStack CDN URL"
    );

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset URL: {}", asset_url);

    // Step 3: Upload image to S3 using the presigned URL with correct checksum but wrong content length
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data[..1024], // Upload only partial data
        "image/png",
        content_digest_base64, // Use the content_digest_base64 from the response
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist at asset URL
    let file_exists = asset_exists_at_url(asset_url)
        .await
        .expect("Failed to check if file exists at URL");

    assert!(!file_exists, "File should not exist at asset URL");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with wrong content length test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_expired_presigned_url() {
    // 1 second presigned url expiry
    let setup = TestSetup::new(Some(1)).await;

    // Step 1: Generate test image data with known SHA-256
    let (image_data, sha256) = generate_test_encrypted_image(2048);
    println!(
        "Generated test image: {} bytes, SHA-256: {}",
        image_data.len(),
        sha256
    );

    // Step 2: Request presigned URL from the API endpoint
    let upload_request = serde_json::json!({
        "content_digest_sha256": sha256,
        "content_length": image_data.len(),
        "content_type": "image/png"
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", upload_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK for presigned URL request"
    );

    let response_body = setup
        .parse_response_body(response)
        .await
        .expect("Failed to parse response body");

    println!(
        "API Response: {}",
        serde_json::to_string_pretty(&response_body).unwrap()
    );

    // Wait for the presigned URL to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Extract response fields
    let presigned_url = response_body["presigned_url"]
        .as_str()
        .expect("Missing presigned_url in response");
    let asset_url = response_body["asset_url"]
        .as_str()
        .expect("Missing asset_url in response");
    let content_digest_base64 = response_body["content_digest_base64"]
        .as_str()
        .expect("Missing content_digest_base64 in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(
        asset_url.starts_with("http://localhost:4566/world-chat-media/"),
        "Asset URL should start with LocalStack CDN URL"
    );

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset URL: {}", asset_url);

    // Step 3: Try to upload after expiry using the presigned URL
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        "image/png",
        content_digest_base64, // Use the content_digest_base64 from the response
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist at asset URL
    let file_exists = asset_exists_at_url(asset_url)
        .await
        .expect("Failed to check if file exists at URL");

    assert!(!file_exists, "File should not exist at asset URL");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with expired presigned URL test completed successfully!");
}
