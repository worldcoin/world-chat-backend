use axum::response::Response;
use http_body_util::BodyExt;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Create a valid 64-character hex SHA256 digest for testing
pub fn create_valid_sha256() -> String {
    // Concatenate two UUIDs to get 64 hex characters (32 + 32)
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Parse response body to JSON
pub async fn parse_response_body(response: Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Generate test image data with specified size and return data + SHA-256 hash
pub fn generate_test_encrypted_image(size: usize) -> (Vec<u8>, String) {
    // Generate random data for each test run
    let mut buf = vec![0u8; size]; // pre-allocate
    rand::rngs::OsRng.fill_bytes(&mut buf); // fill in one syscall-sized burst
    let data = buf;

    let sha256 = calculate_sha256(&data);
    (data, sha256)
}

/// Calculate SHA-256 checksum of data and return as lowercase hex string
pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
