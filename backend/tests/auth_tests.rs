mod common;

use common::TestSetup;
use http::StatusCode;
use serde_json::json;
use uuid::Uuid;

/// Helper function to create a test auth request with a mock proof
/// This will fail World ID verification but allows us to test the flow
fn create_test_auth_request() -> serde_json::Value {
    // Create a properly formatted but invalid proof (similar to verifier.rs tests)
    let proof = format!("0x{}", "1".repeat(512)); // 256 bytes = 512 hex chars
                                                  // Use valid format for nullifier hash (32 bytes hex)
    let nullifier_hash = "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa";
    json!({
        "proof": proof,
        "nullifier_hash": nullifier_hash,
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "signal": "test_signal",
        "encrypted_push_id": format!("encrypted-push-{}", Uuid::new_v4()),
        "credential_type": "orb"
    })
}

#[tokio::test]
async fn test_authorize_with_invalid_world_id_proof() {
    let context = TestSetup::new(None).await;

    // Create test auth request with mock proof (will fail World ID verification)
    let auth_request = create_test_auth_request();

    // This should fail because the World ID proof is invalid
    let response = context
        .send_post_request("/v1/authorize", auth_request.clone())
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body = context
        .parse_response_body(response)
        .await
        .expect("Failed to parse response");

    // Should return either 401 (invalid proof) or 500 (sequencer error) depending on the failure mode
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 401 or 500 for invalid World ID proof, got {}",
        status
    );

    // Verify error structure
    assert!(
        body["error"].is_object(),
        "Expected error object in response"
    );

    // Error code could be either invalid_proof or sequencer_error depending on failure mode
    let error_code = body["error"]["code"].as_str().unwrap_or("");
    assert!(
        error_code == "invalid_proof"
            || error_code == "sequencer_error"
            || error_code == "invalid_proof_data",
        "Expected invalid_proof, sequencer_error, or invalid_proof_data error code, got {}",
        error_code
    );
}

#[tokio::test]
#[ignore = "Requires valid World ID credentials"]
async fn test_authorize_with_valid_world_id_proof() {
    // This test would require real World ID credentials to pass
    // It's kept here as documentation of the expected happy path flow

    // To run this test with real credentials:
    // 1. Get valid World ID app credentials
    // 2. Generate a valid proof using the World ID SDK
    // 3. Replace the mock values below with real ones
    // 4. Remove the #[ignore] attribute

    let context = TestSetup::new(None).await;

    // This would need to be a real valid proof from World ID
    let auth_request = json!({
        "proof": "0x_REAL_PROOF_HERE",
        "nullifier_hash": "0x_REAL_NULLIFIER_HERE",
        "merkle_root": "0x_REAL_ROOT_HERE",
        "signal": "test_signal",
        "encrypted_push_id": "encrypted-push-123",
        "credential_type": "orb"
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body = context
        .parse_response_body(response)
        .await
        .expect("Failed to parse response");

    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn test_authorize_invalid_proof_format() {
    let context = TestSetup::new(None).await;

    // Create auth request with invalid proof format
    let auth_request = json!({
        "proof": "invalid_proof_not_hex", // Invalid format
        "nullifier_hash": "0x1234567890abcdef",
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "signal": "test_signal",
        "encrypted_push_id": "encrypted-push-123",
        "credential_type": "orb"
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    // Should fail with bad request due to invalid proof format
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid proof format should return 400"
    );

    let body = context
        .parse_response_body(response)
        .await
        .expect("Failed to parse error response");

    // Verify error structure (note: fields are camelCase due to serde rename)
    assert!(
        body["error"].is_object(),
        "Expected error object in response: {:?}",
        body
    );
    assert!(
        body.get("allowRetry").is_some(),
        "Expected allowRetry field in response: {:?}",
        body
    );

    println!("✅ Invalid proof format correctly rejected: {:?}", body);
}

#[tokio::test]
async fn test_authorize_missing_fields() {
    let context = TestSetup::new(None).await;

    // Test with missing required fields
    let test_cases = vec![
        (
            json!({
                // Missing proof
                "nullifier_hash": "0x1234567890abcdef",
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                "signal": "test_signal",
                "encrypted_push_id": "encrypted-push-123",
                "credential_type": "orb"
            }),
            "missing proof",
        ),
        (
            json!({
                "proof": format!("0x{}", "1".repeat(512)),
                // Missing nullifier_hash
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                "signal": "test_signal",
                "encrypted_push_id": "encrypted-push-123",
                "credential_type": "orb"
            }),
            "missing nullifier_hash",
        ),
        (
            json!({
                "proof": format!("0x{}", "1".repeat(512)),
                "nullifier_hash": "0x1234567890abcdef",
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                "signal": "test_signal",
                // Missing encrypted_push_id
                "credential_type": "orb"
            }),
            "missing encrypted_push_id",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_post_request("/v1/authorize", request)
            .await
            .expect("Failed to send authorizerequest");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Request with {} should return 400",
            case_name
        );
    }
}

#[tokio::test]
async fn test_authorize_malformed_world_id_proof() {
    let context = TestSetup::new(None).await;

    // Test with malformed proof (not 512 hex chars)
    let auth_request = json!({
        "proof": "0x123", // Too short
        "nullifier_hash": "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa",
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "signal": "test_signal",
        "encrypted_push_id": "encrypted-push-123",
        "credential_type": "orb"
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    // Should return BAD_REQUEST for malformed proof
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 for malformed proof"
    );

    let body = context
        .parse_response_body(response)
        .await
        .expect("Failed to parse response");

    assert!(
        body["error"].is_object(),
        "Expected error object in response"
    );
}

#[tokio::test]
async fn test_authorize_invalid_nullifier_format() {
    let context = TestSetup::new(None).await;

    // Test with invalid nullifier hash format
    let auth_request = json!({
        "proof": format!("0x{}", "1".repeat(512)),
        "nullifier_hash": "invalid_nullifier", // Not hex format
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "signal": "test_signal",
        "encrypted_push_id": "encrypted-push-123",
        "credential_type": "orb"
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    // Should return BAD_REQUEST for invalid nullifier format
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 for invalid nullifier format"
    );
}

#[tokio::test]
async fn test_authorize_invalid_merkle_root_format() {
    let context = TestSetup::new(None).await;

    // Test with invalid merkle root format
    let auth_request = json!({
        "proof": format!("0x{}", "1".repeat(512)),
        "nullifier_hash": "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa",
        "merkle_root": "not_a_valid_root", // Invalid format
        "signal": "test_signal",
        "encrypted_push_id": "encrypted-push-123",
        "credential_type": "orb"
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    // Should return BAD_REQUEST for invalid merkle root
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 for invalid merkle root format"
    );
}
