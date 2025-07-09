use aws_sdk_s3::Client as S3Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::time::sleep;

/// Generate test image data with specified size and return data + SHA-256 hash
pub fn generate_test_image(size: usize) -> (Vec<u8>, String) {
    // Generate random-like data using a simple pattern
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i % 256) as u8);
    }
    
    let sha256 = calculate_sha256(&data);
    (data, sha256)
}

/// Calculate SHA-256 checksum of data and return as lowercase hex string
pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Upload data to S3 using presigned URL
pub async fn upload_to_s3(
    presigned_url: &str,
    data: &[u8],
    content_type: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from(data.len()));
    
    if let Some(ct) = content_type {
        headers.insert(CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
    }

    let client = reqwest::Client::new();
    client
        .put(presigned_url)
        .headers(headers)
        .body(data.to_vec())
        .send()
        .await
}

/// Download data from S3 using S3 client
pub async fn download_from_s3(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let response = s3_client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let body = response.body.collect().await?;
    Ok(body.into_bytes().to_vec())
}

/// Check if S3 object exists
pub async fn s3_object_exists(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    match s3_client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.to_string().contains("NotFound") {
                Ok(false)
            } else {
                Err(e.into())
            }
        }
    }
}

/// Wait for specified duration (useful for expiration tests)
pub async fn wait_for_duration(duration: Duration) {
    sleep(duration).await;
}

/// Create test data with specific content that will produce a known SHA-256
pub fn create_test_data_with_known_hash() -> (Vec<u8>, String) {
    let data = b"Hello, World! This is test data for E2E upload testing.";
    let sha256 = calculate_sha256(data);
    (data.to_vec(), sha256)
}

/// Create test data with wrong checksum (for bad actor tests)
pub fn create_data_with_wrong_checksum() -> (Vec<u8>, String, String) {
    let data = b"This is the actual data";
    let wrong_sha256 = "a".repeat(64); // Invalid SHA-256 that will fail validation
    let correct_sha256 = calculate_sha256(data);
    (data.to_vec(), wrong_sha256, correct_sha256)
}

/// Verify that two byte arrays are identical
pub fn verify_data_integrity(original: &[u8], downloaded: &[u8]) -> bool {
    original == downloaded
}

/// Extract S3 key from asset_id returned by the API
pub fn extract_s3_key_from_asset_id(asset_id: &str) -> String {
    asset_id.to_string()
}

/// Create headers for S3 upload with specific content type and checksum
pub fn create_upload_headers(
    content_length: usize,
    content_type: &str,
    checksum_sha256: Option<&str>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from(content_length));
    headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());
    
    if let Some(checksum) = checksum_sha256 {
        headers.insert("x-amz-checksum-sha256", HeaderValue::from_str(checksum).unwrap());
    }
    
    headers
}

/// Parse presigned URL to extract query parameters (useful for debugging)
pub fn parse_presigned_url_params(url: &str) -> std::collections::HashMap<String, String> {
    let parsed = url::Url::parse(url).unwrap();
    parsed.query_pairs().into_owned().collect()
}