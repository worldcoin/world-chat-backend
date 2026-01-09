mod common;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use common::TestSetup;
use http::StatusCode;
use p256::ecdsa::{signature::DigestSigner, Signature, SigningKey};
use p256::SecretKey;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use walletkit_core::{
    proof::{ProofContext, ProofOutput},
    world_id::WorldId,
    CredentialType,
};

/// Helper to create a request with a valid World ID proof
/// Uses `walletkit` to create a staging identity and proof
async fn create_valid_world_id_proof(encrypted_push_id: String, timestamp: i64) -> ProofOutput {
    let app_id = std::env::var("WORLD_ID_APP_ID").expect("WORLD_ID_APP_ID must be set");
    let action = std::env::var("WORLD_ID_ACTION").expect("WORLD_ID_ACTION must be set");

    let world_id = WorldId::new(b"not_a_real_secret", &walletkit_core::Environment::Staging);
    let signal = format!("{}:{}", encrypted_push_id, timestamp);
    let context = ProofContext::new(&app_id, Some(action), Some(signal), CredentialType::Device);

    world_id
        .generate_proof(&context)
        .await
        .expect("Failed to generate proof")
}

#[tokio::test]
async fn test_authorize_with_valid_world_id_proof() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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
async fn test_authorize_with_stolen_proof() {
    let context = TestSetup::default().await;

    let encrypted_push_id_user1 = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id_user1.clone(), timestamp).await;

    let encrypted_push_id_user2 = format!("encrypted-push-{}", Uuid::new_v4());

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id_user2,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_authorize_invalid_proof_format() {
    let context = TestSetup::default().await;

    // Create auth request with invalid proof format
    let auth_request = json!({
        "proof": "invalid_proof_not_hex", // Invalid format
        "nullifier_hash": "0x1234567890abcdef",
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "encrypted_push_id": "encrypted-push-123",
        "timestamp": Utc::now().timestamp(),
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
}

#[tokio::test]
async fn test_authorize_missing_fields() {
    let context = TestSetup::default().await;

    // Test with missing required fields
    let test_cases = vec![
        (
            json!({
                // Missing proof
                "nullifier_hash": "0x1234567890abcdef",
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                "encrypted_push_id": "encrypted-push-123",
                "timestamp": Utc::now().timestamp(),
                "credential_type": "orb"
            }),
            "missing proof",
        ),
        (
            json!({
                "proof": format!("0x{}", "1".repeat(512)),
                // Missing nullifier_hash
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                "encrypted_push_id": "encrypted-push-123",
                "timestamp": Utc::now().timestamp(),
                "credential_type": "orb"
            }),
            "missing nullifier_hash",
        ),
        (
            json!({
                "proof": format!("0x{}", "1".repeat(512)),
                "nullifier_hash": "0x1234567890abcdef",
                "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
                // Missing encrypted_push_id
                "timestamp": Utc::now().timestamp(),
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
            StatusCode::UNPROCESSABLE_ENTITY,
            "Request with {} should return 422",
            case_name
        );
    }
}

#[tokio::test]
async fn test_authorize_malformed_world_id_proof() {
    let context = TestSetup::default().await;

    // Test with malformed proof (not 512 hex chars)
    let auth_request = json!({
        "proof": "0x123", // Too short
        "nullifier_hash": "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa",
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "encrypted_push_id": "encrypted-push-123",
        "timestamp": Utc::now().timestamp(),
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
}

#[tokio::test]
async fn test_authorize_invalid_nullifier_format() {
    let context = TestSetup::default().await;

    // Test with invalid nullifier hash format
    let auth_request = json!({
        "proof": format!("0x{}", "1".repeat(512)),
        "nullifier_hash": "invalid_nullifier", // Not hex format
        "merkle_root": "0x2a7c09e8af01f39a87d89e9f0a9ba66fbf6fb304cc643051dd4ea24c4e9f7e8d",
        "encrypted_push_id": "encrypted-push-123",
        "timestamp": Utc::now().timestamp(),
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
    let context = TestSetup::default().await;

    // Test with invalid merkle root format
    let auth_request = json!({
        "proof": format!("0x{}", "1".repeat(512)),
        "nullifier_hash": "0x1359a81e3a42dc1c34786cbefbcc672a3d730510dba7a3be9941b207b0cf52fa",
        "merkle_root": "not_a_valid_root", // Invalid format
        "encrypted_push_id": "encrypted-push-123",
        "timestamp": Utc::now().timestamp(),
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

#[tokio::test]
async fn test_authorize_jwt_is_validatable_by_manager() {
    let context = TestSetup::default().await;

    // Get a valid access token
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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

    let token = body["access_token"]
        .as_str()
        .expect("access_token must be a string");

    // Validate using JwtManager (independent of route issuance path)
    let manager = backend::jwt::JwtManager::new(context.kms_client.clone(), &context.environment)
        .await
        .expect("failed to build JwtManager");
    let claims = manager
        .validate(token, None)
        .expect("token should validate");
    assert_eq!(claims.subject, encrypted_push_id);
    assert_eq!(claims.issuer, context.environment.jwt_issuer_url());
}

#[tokio::test]
async fn test_validate_rejects_wrong_alg() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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

    let token = body["access_token"]
        .as_str()
        .expect("access_token must be a string");
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);

    // Decode header, change alg to HS256, re-encode
    let mut header_json: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).expect("header b64 decode"))
            .expect("parse header json");
    header_json["alg"] = serde_json::Value::String("HS256".to_string());
    let header_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header_json).expect("serialize header"));
    let tampered = format!("{}.{}.{}", header_b64, parts[1], parts[2]);

    let manager = backend::jwt::JwtManager::new(context.kms_client.clone(), &context.environment)
        .await
        .expect("failed to build JwtManager");
    let result = manager.validate(&tampered, None);
    assert!(result.is_err(), "wrong alg should be rejected");
}

#[tokio::test]
async fn test_validate_rejects_wrong_kid() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;
    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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

    let token = body["access_token"]
        .as_str()
        .expect("access_token must be a string");
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);

    // Decode header, change kid, re-encode
    let mut header_json: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).expect("header b64 decode"))
            .expect("parse header json");
    header_json["kid"] = serde_json::Value::String("invalid_kid".to_string());
    let header_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header_json).expect("serialize header"));
    let tampered = format!("{}.{}.{}", header_b64, parts[1], parts[2]);

    let manager = backend::jwt::JwtManager::new(context.kms_client.clone(), &context.environment)
        .await
        .expect("failed to build JwtManager");
    let result = manager.validate(&tampered, None);
    assert!(result.is_err(), "wrong kid should be rejected");
}

#[tokio::test]
async fn test_validate_rejects_payload_tamper() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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

    let token = body["access_token"]
        .as_str()
        .expect("access_token must be a string");
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);

    // Decode payload, modify subject, re-encode
    let mut payload_json: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("payload b64 decode"),
    )
    .expect("parse payload json");
    payload_json["sub"] = serde_json::Value::String("tampered-subject".to_string());
    let payload_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload_json).expect("serialize payload"));
    let tampered = format!("{}.{}.{}", parts[0], payload_b64, parts[2]);

    let manager = backend::jwt::JwtManager::new(context.kms_client.clone(), &context.environment)
        .await
        .expect("failed to build JwtManager");
    let result = manager.validate(&tampered, None);
    assert!(result.is_err(), "payload tamper should be rejected");
}

#[tokio::test]
async fn test_protected_endpoint_rejects_jwt_with_different_signing_key() {
    // Test with auth enabled
    let context = TestSetup::new(None, false).await;

    // Get a valid JWT token first
    let valid_token = get_valid_jwt_token(&context).await;

    // Split the token into its three parts
    let parts: Vec<&str> = valid_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have three parts");

    // Generate an attacker's P-256 keypair (different from KMS key)
    let attacker_secret = SecretKey::random(&mut rand::thread_rng());
    let attacker_signing_key = SigningKey::from(attacker_secret);

    // Create the signing input (header.payload)
    let signing_input = format!("{}.{}", parts[0], parts[1]);

    // Sign with the attacker's key using the same algorithm (ES256/P-256 with SHA-256)
    let mut digest = Sha256::new();
    digest.update(signing_input.as_bytes());
    let attacker_signature: Signature = attacker_signing_key.sign_digest(digest);

    // Convert signature to raw format (64 bytes) and encode as base64url
    let attacker_sig_b64 = URL_SAFE_NO_PAD.encode(attacker_signature.to_bytes());

    // Reconstruct the token with the attacker's signature
    // This is a properly signed JWT, but with the wrong key
    let forged_token = format!("{}.{}", signing_input, attacker_sig_b64);

    // Try to use the forged token on a protected endpoint
    let media_request = json!({
        "content_digest_sha256": "a".repeat(64),
        "content_length": 1024,
        "content_type": "image/jpeg"
    });

    let response = context
        .send_post_request_with_headers(
            "/v1/media/presigned-urls",
            media_request,
            vec![("Authorization", &format!("Bearer {}", forged_token))],
        )
        .await
        .expect("Failed to send request");

    // Should be rejected with 401 Unauthorized because the signature
    // doesn't match the KMS key's public key
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "JWT signed with different key should be rejected"
    );
}

#[tokio::test]
async fn test_authorize_with_future_timestamp() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp() + 60; // 1 minute in the future
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_authorize_with_expired_timestamp() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    // Expired by 1 second beyond 5 minutes
    let timestamp = Utc::now().timestamp() - (5 * 60 + 1);
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
    });

    let response = context
        .send_post_request("/v1/authorize", auth_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Helper to get a valid JWT token from the authorize endpoint
async fn get_valid_jwt_token(context: &TestSetup) -> String {
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let timestamp = Utc::now().timestamp();
    let proof = create_valid_world_id_proof(encrypted_push_id.clone(), timestamp).await;

    let auth_request = json!({
        "proof": proof.get_proof_as_string(),
        "nullifier_hash": proof.get_nullifier_hash().to_hex_string(),
        "merkle_root": proof.get_merkle_root().to_hex_string(),
        "encrypted_push_id": encrypted_push_id,
        "timestamp": timestamp,
        "credential_type": proof.get_credential_type(),
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

    body["access_token"]
        .as_str()
        .expect("access_token must be a string")
        .to_string()
}

/// TODO: TEMPORARY: Remove this once we finish mobile dev testing
#[tokio::test]
#[ignore]
async fn test_protected_endpoint_without_auth_header() {
    // Test with auth enabled (disable_auth = false)
    let context = TestSetup::new(None, false).await;

    // Try to access protected endpoint without Authorization header
    let media_request = json!({
        "content_digest_sha256": "a".repeat(64),
        "content_length": 1024,
        "content_type": "image/jpeg"
    });

    let response = context
        .send_post_request("/v1/media/presigned-urls", media_request)
        .await
        .expect("Failed to send request");

    // Should fail with 401 Unauthorized
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Protected endpoint should require auth when auth is enabled"
    );
}

/// TODO: TEMPORARY: Remove this once we finish mobile dev testing
#[tokio::test]
#[ignore]
async fn test_protected_endpoint_with_invalid_auth_header() {
    // Test with auth enabled (disable_auth = false)
    let context = TestSetup::new(None, false).await;

    // Try to access protected endpoint with invalid token
    let media_request = json!({
        "content_digest_sha256": "a".repeat(64),
        "content_length": 1024,
        "content_type": "image/jpeg"
    });

    let response = context
        .send_post_request_with_headers(
            "/v1/media/presigned-urls",
            media_request,
            vec![("Authorization", "Bearer invalid.jwt.token")],
        )
        .await
        .expect("Failed to send request");

    // Should fail with 401 Unauthorized
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Protected endpoint should reject invalid tokens"
    );
}

/// TODO: TEMPORARY: Remove this once we finish mobile dev testing
#[tokio::test]
#[ignore]
async fn test_protected_endpoint_with_valid_token() {
    // Test with auth enabled (disable_auth = false)
    let context = TestSetup::new(None, false).await;

    // Get a valid JWT token first
    let token = get_valid_jwt_token(&context).await;

    // Try to access protected endpoint with valid token
    let media_request = json!({
        "content_digest_sha256": "a".repeat(64),
        "content_length": 1024,
        "content_type": "image/jpeg"
    });

    let response = context
        .send_post_request_with_headers(
            "/v1/media/presigned-urls",
            media_request,
            vec![("Authorization", &format!("Bearer {}", token))],
        )
        .await
        .expect("Failed to send request");

    // Should succeed with 200 OK
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Protected endpoint should accept valid tokens"
    );

    // Verify response structure
    let body = context
        .parse_response_body(response)
        .await
        .expect("Failed to parse response");

    assert!(
        body["presigned_url"].is_string() || body["asset_url"].is_string(),
        "Response should contain either presigned_url or asset_url"
    );
}
