mod common;

use chrono::Utc;
use common::TestSetup;
use http::StatusCode;
use serde_json::json;
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
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
async fn test_authorize_jwt_is_verifiable_with_josekit() {
    use josekit::jws::alg::ecdsa::EcdsaJwsAlgorithm;

    let context = TestSetup::new(None).await;

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

    // Fetch KMS public key (DER, SubjectPublicKeyInfo)
    let pk_out = context
        .kms_client
        .get_public_key()
        .key_id(context.environment.jwt_kms_key_arn())
        .send()
        .await
        .expect("get_public_key failed");
    let der = pk_out
        .public_key()
        .expect("missing public_key in GetPublicKey output")
        .as_ref()
        .to_vec();

    // Verify with josekit ES256
    let verifier = EcdsaJwsAlgorithm::Es256
        .verifier_from_der(&der)
        .expect("verifier_from_der failed");
    let (payload, header) =
        josekit::jwt::decode_with_verifier(token, &verifier).expect("JWT verification failed");

    // Basic header assertions
    assert_eq!(header.algorithm(), Some("ES256"));
    assert_eq!(header.token_type(), Some("JWT"));

    // Ensure payload is a JSON object and contains expected claims shape
    let payload_json: serde_json::Value =
        serde_json::from_str(&payload.to_string()).expect("payload should be valid JSON");
    assert!(payload_json.is_object());
    assert_eq!(
        payload_json.get("sub").expect("missing sub").as_str(),
        Some(encrypted_push_id.as_str())
    );
}

#[tokio::test]
async fn test_jwks_wellknown_verifies_token_signature() {
    use josekit::{jwk::Jwk, jws::alg::ecdsa::EcdsaJwsAlgorithm};

    let context = TestSetup::new(None).await;

    // Issue a valid access token
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

    // Fetch JWKS from well-known endpoint
    let jwks_resp = context
        .send_get_request("/.well-known/jwks.json")
        .await
        .expect("Failed to GET jwks");
    assert_eq!(jwks_resp.status(), StatusCode::OK);
    let jwks_body: serde_json::Value = context
        .parse_response_body(jwks_resp)
        .await
        .expect("Failed to parse jwks body");

    let keys = jwks_body["keys"].as_array().expect("keys must be an array");
    assert!(!keys.is_empty(), "jwks keys must not be empty");
    let first_key = &keys[0];

    // Build verifier from JWK
    let jwk = Jwk::from_bytes(first_key.to_string().as_bytes()).expect("invalid jwk json");
    let verifier = EcdsaJwsAlgorithm::Es256
        .verifier_from_jwk(&jwk)
        .expect("verifier_from_jwk failed");

    // Verify token using JWKS-derived verifier
    let (payload, header) =
        josekit::jwt::decode_with_verifier(token, &verifier).expect("JWT verification failed");
    assert_eq!(header.algorithm(), Some("ES256"));
    assert_eq!(header.token_type(), Some("JWT"));

    // Check sub claim matches the encrypted_push_id we used
    let payload_json: serde_json::Value =
        serde_json::from_str(&payload.to_string()).expect("payload should be valid JSON");
    assert_eq!(
        payload_json.get("sub").expect("missing subject").as_str(),
        Some(encrypted_push_id.as_str())
    );
    assert_eq!(
        payload_json.get("iss").expect("missing issuer").as_str(),
        Some("chat.toolsforhumanity.com")
    );
}

#[tokio::test]
async fn test_authorize_with_future_timestamp() {
    let context = TestSetup::new(None).await;

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
    let context = TestSetup::new(None).await;

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
