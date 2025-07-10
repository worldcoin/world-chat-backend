#[path = "../common/mod.rs"]
mod common;

use aws_sdk_s3::Client as S3Client;
use axum::Extension;
use backend::{media_storage::MediaStorage, routes, types::Environment};
use common::e2e_utils::*;
use common::*;
use std::sync::Arc;

/// E2E test setup with real dependencies
pub struct E2ETestSetup {
    pub router: axum::Router,
    pub s3_client: Arc<S3Client>,
    pub media_storage: Arc<MediaStorage>,
    pub environment: Environment,
    pub bucket_name: String,
}

impl E2ETestSetup {
    /// Create a new E2E test setup with real dependencies
    pub async fn new() -> Self {
        // Setup test environment
        setup_test_env();

        // Use development environment for E2E tests (LocalStack)
        let environment = Environment::Development;

        // Configure AWS S3 client for LocalStack
        let s3_config = environment.s3_client_config().await;
        let s3_client = Arc::new(S3Client::from_conf(s3_config));

        // Get bucket name
        let bucket_name = environment.s3_bucket();

        // Create media storage client
        let media_storage = Arc::new(MediaStorage::new(
            s3_client.clone(),
            bucket_name.clone(),
            environment.presigned_url_expiry_secs(),
        ));

        // Create router with extensions
        let router = routes::handler()
            .layer(Extension(environment))
            .layer(Extension(media_storage.clone()))
            .into();

        Self {
            router,
            s3_client,
            media_storage,
            environment,
            bucket_name,
        }
    }

    /// Get presigned URL expiry duration for testing
    pub fn presigned_url_expiry_secs(&self) -> u64 {
        self.environment.presigned_url_expiry_secs()
    }
}

// Placeholder test to ensure E2E infrastructure works
#[tokio::test]
// #[ignore = "E2E tests - run manually"]
async fn test_e2e_infrastructure() {
    let setup = E2ETestSetup::new().await;

    // Test that we can generate test data
    let (data, sha256) = generate_test_image(1024);
    assert_eq!(data.len(), 1024);
    assert_eq!(sha256.len(), 64);

    // Test that we can calculate checksums
    let calculated_sha256 = calculate_sha256(&data);
    assert_eq!(sha256, calculated_sha256);

    // Test that we have LocalStack setup
    assert!(setup.is_localstack());

    println!("E2E infrastructure test passed!");
}

impl E2ETestSetup {
    /// Check if running in LocalStack environment
    pub fn is_localstack(&self) -> bool {
        matches!(self.environment, Environment::Development)
    }

    /// Send a POST request to the router and return the response
    pub async fn send_post_request(
        &self,
        route: &str,
        payload: serde_json::Value,
    ) -> Result<axum::response::Response, Box<dyn std::error::Error>> {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let request = Request::builder()
            .uri(route)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(payload.to_string()))?;

        let response = self.router.clone().oneshot(request).await?;
        Ok(response)
    }

    /// Parse response body to JSON
    pub async fn parse_response_body(
        &self,
        response: axum::response::Response,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        use http_body_util::BodyExt;

        let body = response.into_body().collect().await?.to_bytes();
        let json = serde_json::from_slice(&body)?;
        Ok(json)
    }
}

/// E2E test for the complete upload workflow happy path
#[tokio::test]
// #[ignore = "E2E tests - run manually"]
async fn test_e2e_upload_happy_path() {
    let setup = E2ETestSetup::new().await;

    // Step 1: Generate test image data with known SHA-256
    let (image_data, sha256) = generate_test_image(2048);
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
        200,
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
    println!("base64_checksum_test: {}", sha256_b64);
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

    assert!(
        verify_data_integrity(&image_data, &downloaded_data),
        "Data integrity check failed"
    );

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
        409,
        "Expected 409 Conflict for duplicate SHA-256"
    );

    println!("✅ Deduplication works correctly (409 Conflict)");

    println!("🎉 E2E upload happy path test completed successfully!");
}
