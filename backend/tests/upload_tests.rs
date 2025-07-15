mod common;

use common::*;

use http::StatusCode;
use serde_json::json;

pub fn create_upload_request(
    content_digest_sha256: String,
    content_length: i64,
) -> serde_json::Value {
    json!({
        "content_digest_sha256": content_digest_sha256,
        "content_length": content_length
    })
}

// Happy path tests

#[tokio::test]
async fn test_upload_media_happy_path() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1024);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body = parse_response_body(response).await;
    assert!(body["presigned_url"].is_string());
    assert!(body["expires_at"].is_string());

    let presigned_url = body["presigned_url"].as_str().unwrap();
    assert!(presigned_url.contains("localhost:4566")); // LocalStack URL
}

#[tokio::test]
async fn test_upload_media_valid_content_lengths() {
    let setup = TestContext::new(None).await;

    // Test various valid content lengths
    let test_cases = vec![1, 1024, 1_048_576, 15_728_640]; // 1 byte to 15 MiB

    for content_length in test_cases {
        let content_digest_sha256 = create_valid_sha256();
        let payload = create_upload_request(content_digest_sha256, content_length);

        let response = setup
            .send_post_request("/v1/media/presigned-urls", payload)
            .await
            .expect("Failed to send request");

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
    let setup = TestContext::new(None).await;

    let payload = create_upload_request("invalid_sha256_digest".to_string(), 1024);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_short_sha256() {
    let setup = TestContext::new(None).await;

    let payload = create_upload_request("abc123".to_string(), 1024);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_too_large() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 15_728_641); // 15 MiB + 1 byte

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_zero() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 0);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_content_length_negative() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, -1);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Malformed request tests

#[tokio::test]
async fn test_upload_media_missing_sha256() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "content_length": 1024
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
    let setup = TestContext::new(None).await;

    let payload = json!({
        "content_digest_sha256": create_valid_sha256()
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
    let setup = TestContext::new(None).await;

    let payload = json!({});

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_invalid_json_types() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "content_digest_sha256": 12345, // Should be string
        "content_length": "invalid" // Should be number
    });

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Edge case tests

#[tokio::test]
async fn test_upload_media_exact_max_content_length() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256, 15_728_640); // Exactly 15 MiB

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_minimum_content_length() {
    let setup = TestContext::new(None).await;

    let content_digest_sha256 = create_valid_sha256();
    let payload = create_upload_request(content_digest_sha256.clone(), 1); // Minimum allowed

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_special_hex_characters() {
    let setup = TestContext::new(None).await;

    // Test with all valid hex characters
    let content_digest_sha256 = "abcdef0123456789".repeat(4); // 64 chars of valid hex
    let payload = create_upload_request(content_digest_sha256, 1024);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upload_media_uppercase_hex() {
    let setup = TestContext::new(None).await;

    // Test uppercase hex (should be rejected by schemars regex)
    let content_digest_sha256 = "ABCDEF0123456789".repeat(4); // 64 chars of uppercase hex
    let payload = create_upload_request(content_digest_sha256, 1024);

    let response = setup
        .send_post_request("/v1/media/presigned-urls", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_media_extra_fields() {
    let setup = TestContext::new(None).await;

    // Test schemars deny_unknown_fields
    let payload = json!({
        "content_digest_sha256": create_valid_sha256(),
        "content_length": 1024,
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
    let setup = TestContext::new(None).await;

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
        "content_length": image_data.len()
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
    let asset_id = response_body["asset_id"]
        .as_str()
        .expect("Missing asset_id in response");
    let expires_at = response_body["expires_at"]
        .as_str()
        .expect("Missing expires_at in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(!asset_id.is_empty(), "Asset ID should not be empty");
    assert!(!expires_at.is_empty(), "Expires at should not be empty");

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset ID: {}", asset_id);

    // Step 3: Upload image to S3 using the presigned URL with checksum headers
    let sha256_b64 = hex_sha256_to_base64(&sha256);
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        Some("application/octet-stream"),
        Some(&sha256_b64), // Include SHA-256 checksum header in base64 format
    )
    .await
    .expect("Failed to upload to S3");

    assert!(
        upload_response.status().is_success(),
        "S3 upload failed with status: {}",
        upload_response.status()
    );

    println!("Successfully uploaded to S3");

    // Step 4: Download image from S3 using the S3 client directly
    let downloaded_data = download_from_s3(&setup.s3_client, &setup.bucket_name, asset_id)
        .await
        .expect("Failed to download from S3");

    println!("Downloaded {} bytes from S3", downloaded_data.len());

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
        "content_length": image_data.len()
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

    println!("✅ Deduplication works correctly (409 Conflict)");

    println!("🎉 E2E upload happy path test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_wrong_checksum() {
    let setup = TestContext::new(None).await;

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
        "content_length": image_data.len()
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
    let asset_id = response_body["asset_id"]
        .as_str()
        .expect("Missing asset_id in response");
    let expires_at = response_body["expires_at"]
        .as_str()
        .expect("Missing expires_at in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(!asset_id.is_empty(), "Asset ID should not be empty");
    assert!(!expires_at.is_empty(), "Expires at should not be empty");

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset ID: {}", asset_id);

    // Step 3: Upload image to S3 using the presigned URL with checksum headers
    let wrong_sha256_b64 = hex_sha256_to_base64(&"a".repeat(64));
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        Some("application/octet-stream"),
        Some(&wrong_sha256_b64), // Include SHA-256 checksum header in base64 format
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist
    let file_exists = s3_object_exists(&setup.s3_client, &setup.bucket_name, asset_id)
        .await
        .expect("Failed to check if file exists");

    assert!(!file_exists, "File should not exist");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with wrong checksum test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_wrong_content_length() {
    let setup = TestContext::new(None).await;

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
        "content_length": image_data.len()
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
    let asset_id = response_body["asset_id"]
        .as_str()
        .expect("Missing asset_id in response");
    let expires_at = response_body["expires_at"]
        .as_str()
        .expect("Missing expires_at in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(!asset_id.is_empty(), "Asset ID should not be empty");
    assert!(!expires_at.is_empty(), "Expires at should not be empty");

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset ID: {}", asset_id);

    // Step 3: Upload image to S3 using the presigned URL with checksum headers
    let sha256_b64 = hex_sha256_to_base64(&sha256);
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data[..1024],
        Some("application/octet-stream"),
        Some(&sha256_b64), // Include SHA-256 checksum header in base64 format
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist
    let file_exists = s3_object_exists(&setup.s3_client, &setup.bucket_name, asset_id)
        .await
        .expect("Failed to check if file exists");

    assert!(!file_exists, "File should not exist");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with wrong checksum test completed successfully!");
}

#[tokio::test]
async fn test_e2e_upload_with_expired_presigned_url() {
    // 1 second presigned url expiry
    let setup = TestContext::new(Some(1)).await;

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
        "content_length": image_data.len()
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

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Extract response fields
    let presigned_url = response_body["presigned_url"]
        .as_str()
        .expect("Missing presigned_url in response");
    let asset_id = response_body["asset_id"]
        .as_str()
        .expect("Missing asset_id in response");
    let expires_at = response_body["expires_at"]
        .as_str()
        .expect("Missing expires_at in response");

    // Verify response format
    assert!(
        presigned_url.contains("localhost:4566"),
        "Expected LocalStack URL"
    );
    assert!(!asset_id.is_empty(), "Asset ID should not be empty");
    assert!(!expires_at.is_empty(), "Expires at should not be empty");

    println!("Presigned URL obtained: {}", presigned_url);
    println!("Asset ID: {}", asset_id);

    // Step 3: Upload image to S3 using the presigned URL with checksum headers
    let sha256_b64 = hex_sha256_to_base64(&sha256);
    let upload_response = upload_to_s3(
        presigned_url,
        &image_data,
        Some("application/octet-stream"),
        Some(&sha256_b64), // Include SHA-256 checksum header in base64 format
    )
    .await
    .expect("Failed to upload to S3");

    assert_eq!(
        upload_response.status(),
        403,
        "Expected 403 Forbidden error"
    );

    // Step 4: Assert that file doesnt exist
    let file_exists = s3_object_exists(&setup.s3_client, &setup.bucket_name, asset_id)
        .await
        .expect("Failed to check if file exists");

    assert!(!file_exists, "File should not exist");

    println!("✅ File does not exist");

    println!("🎉 E2E upload with wrong checksum test completed successfully!");
}
