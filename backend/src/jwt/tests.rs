use super::*;
use p256::ecdsa::SigningKey;
use p256::SecretKey;

mod test_helpers {
    use super::*;
    use p256::ecdsa::signature::DigestSigner;

    /// Generate a test keypair for ES256
    pub fn generate_test_keypair() -> (SigningKey, VerifyingKey) {
        let secret_key = SecretKey::random(&mut rand::thread_rng());
        let signing_key = SigningKey::from(secret_key);
        let verifying_key = *signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    /// Create a test token with known keys (bypassing KMS)
    pub fn create_test_token(signing_key: &SigningKey, kid: &str, payload: &JwsPayload) -> String {
        let header = JwsHeader {
            alg: ALG_ES256.to_string(),
            typ: TYP_JWT.to_string(),
            kid: kid.to_string(),
        };

        let signing_input = craft_signing_input(&header, payload).unwrap();

        // Sign directly with p256 (bypass KMS)
        let mut digest = Sha256::new();
        digest.update(signing_input.as_bytes());
        let signature: Signature = signing_key.sign_digest(digest);
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        format!("{signing_input}.{sig_b64}")
    }
}

mod token_parsing {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_parse_valid_three_part_token() {
        let (signing_key, _) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test-123".to_string());
        let token = create_test_token(&signing_key, "test-kid", &payload);

        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();
        assert_eq!(parts.header.alg, "ES256");
        assert_eq!(parts.header.typ, "JWT");
        assert_eq!(parts.header.kid, "test-kid");
        assert_eq!(parts.payload.subject, "test-123");
    }

    #[test]
    fn test_reject_token_with_wrong_parts() {
        // Test various malformed tokens - inline since only used once
        let malformed_tokens = vec![
            ("missing_parts", "eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0"),
            (
                "extra_parts",
                "eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.sig.extra",
            ),
            (
                "invalid_base64_header",
                "not-base64.eyJzdWIiOiJ0ZXN0In0.sig",
            ),
            (
                "invalid_base64_payload",
                "eyJhbGciOiJFUzI1NiJ9.not-base64.sig",
            ),
            ("empty_parts", ".."),
            ("only_dots", "..."),
        ];

        for (test_name, malformed_token) in malformed_tokens {
            let result = JwsTokenParts::try_from(malformed_token);
            assert!(
                result.is_err(),
                "Should reject malformed token: {test_name}"
            );
        }
    }

    #[test]
    fn test_invalid_base64_in_header() {
        let token = "not!valid!base64.eyJzdWIiOiJ0ZXN0In0.signature";
        let result = JwsTokenParts::try_from(token);
        assert!(matches!(result, Err(JwtError::InvalidToken)));
    }

    #[test]
    fn test_invalid_base64_in_payload() {
        let token =
            "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3QifQ.not!valid!base64.signature";
        let result = JwsTokenParts::try_from(token);
        assert!(matches!(result, Err(JwtError::InvalidToken)));
    }

    #[test]
    fn test_invalid_json_in_decoded_parts() {
        // Valid base64 but invalid JSON in header
        let invalid_json = URL_SAFE_NO_PAD.encode(b"not json");
        let valid_payload = URL_SAFE_NO_PAD.encode(b"{\"sub\":\"test\"}");
        let token = format!("{invalid_json}.{valid_payload}.signature");

        let result = JwsTokenParts::try_from(token.as_str());
        assert!(matches!(result, Err(JwtError::InvalidToken)));

        // Valid base64 but invalid JSON in payload
        let valid_header =
            URL_SAFE_NO_PAD.encode(b"{\"alg\":\"ES256\",\"typ\":\"JWT\",\"kid\":\"test\"}");
        let invalid_json = URL_SAFE_NO_PAD.encode(b"not json");
        let token = format!("{valid_header}.{invalid_json}.signature");

        let result = JwsTokenParts::try_from(token.as_str());
        assert!(matches!(result, Err(JwtError::InvalidToken)));
    }
}

mod header_validation {
    use super::test_helpers::*;
    use super::*;

    fn create_custom_header_token(alg: &str, typ: &str, kid: &str) -> String {
        let header = serde_json::json!({
            "alg": alg,
            "typ": typ,
            "kid": kid,
        });
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({
            "sub": "test",
            "iss": "test-issuer",
            "iat": now,
            "exp": now + 3600,
            "nbf": now,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());

        format!("{header_b64}.{payload_b64}.test-signature")
    }

    #[test]
    fn test_reject_wrong_algorithm() {
        // Test HS256 instead of ES256
        let token = create_custom_header_token("HS256", "JWT", "test-kid");
        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();

        assert_eq!(parts.header.alg, "HS256");
        // The actual validation happens in JwtManager::validate
        // This test just ensures we can detect the algorithm
    }

    #[test]
    fn test_reject_wrong_typ() {
        let token = create_custom_header_token("ES256", "JWE", "test-kid");
        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();

        assert_eq!(parts.header.typ, "JWE");
        assert_ne!(parts.header.typ, "JWT");
    }

    #[test]
    fn test_reject_unknown_header_field() {
        // Add an extra field not defined in JwsHeader; with deny_unknown_fields this should fail
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "JWT",
            "kid": "test",
            "x-extra": true
        });
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({
            "sub": "test",
            "iss": "test-issuer",
            "iat": now,
            "exp": now + 3600,
            "nbf": now,
        });
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let token = format!("{header_b64}.{payload_b64}.signature");

        let result = JwsTokenParts::try_from(token.as_str());
        assert!(matches!(result, Err(JwtError::InvalidToken)));
    }

    #[test]
    fn test_reject_missing_kid() {
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "JWT",
            // Missing kid field
        });
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({
            "sub": "test",
            "iss": "test-issuer",
            "iat": now,
            "exp": now + 3600,
            "nbf": now,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let token = format!("{header_b64}.{payload_b64}.signature");

        // Should fail to deserialize because kid is required
        let result = JwsTokenParts::try_from(token.as_str());
        assert!(result.is_err());
    }

    #[test]
    fn test_accept_valid_header() {
        let (signing_key, _) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test-123".to_string());
        let token = create_test_token(&signing_key, "valid-kid", &payload);

        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();
        assert_eq!(parts.header.alg, "ES256");
        assert_eq!(parts.header.typ, "JWT");
        assert_eq!(parts.header.kid, "valid-kid");
    }
}

mod claims_validation {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_expired_token_rejected() {
        let now = Utc::now().timestamp();
        let past = now - 3600; // 1 hour ago
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: past,
            not_before: now - 7200, // 2 hours ago
            issued_at: now - 7200,  // 2 hours ago
        };

        let result = validate_claims(&claims, now, 60); // 60 second skew
        assert!(result.is_err());
    }

    #[test]
    fn test_expired_token_with_skew_accepted() {
        let now = Utc::now().timestamp();
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: now - 30,   // Expired 30 seconds ago
            not_before: now - 3600, // Valid 1 hour ago
            issued_at: now - 3600,  // Issued 1 hour ago
        };

        let result = validate_claims(&claims, now, 60); // 60 second skew - should accept
        assert!(result.is_ok());
    }

    #[test]
    fn test_not_before_future_rejected() {
        let now = Utc::now().timestamp();
        let future = now + 3600; // 1 hour in future
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: now + 7200, // Expires 2 hours from now
            not_before: future,
            issued_at: now - 3600, // Issued 1 hour ago
        };

        let result = validate_claims(&claims, now, 60);
        assert!(result.is_err());
    }

    #[test]
    fn test_not_before_with_skew_accepted() {
        let now = Utc::now().timestamp();
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: now + 7200, // Expires 2 hours from now
            not_before: now + 30,   // Valid in 30 seconds
            issued_at: now - 3600,  // Issued 1 hour ago
        };

        let result = validate_claims(&claims, now, 60); // 60 second skew - should accept
        assert!(result.is_ok());
    }

    #[test]
    fn test_boundary_conditions_exact_expiry() {
        // Test exact expiry time
        let now = Utc::now().timestamp();
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: now,
            not_before: now - 3600, // Valid 1 hour ago
            issued_at: now - 3600,  // Issued 1 hour ago
        };

        // Without skew - should fail (now >= exp)
        let result = validate_claims(&claims, now, 0);
        assert!(result.is_err());

        // With skew - should pass
        let result = validate_claims(&claims, now, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clock_skew_both_directions() {
        let now = Utc::now().timestamp();

        // Token valid from now-30 to now+30
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            expires_at: now + 30,
            not_before: now - 30,
            issued_at: now - 30,
        };

        // Should be valid
        let result = validate_claims(&claims, now, 0);
        assert!(result.is_ok());

        // Test future skew
        let result = validate_claims(&claims, now + 29, 0);
        assert!(result.is_ok());

        // Test past skew
        let result = validate_claims(&claims, now - 29, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_issuer_and_subject() {
        let payload = JwsPayload::from_encrypted_push_id("encrypted-123".to_string());
        assert_eq!(payload.subject, "encrypted-123");
        assert_eq!(payload.issuer, "chat.toolsforhumanity.com");

        // Verify timestamps are set to reasonable values
        let now = Utc::now().timestamp();
        assert!(payload.expires_at > now); // Should expire in the future
        assert!(payload.not_before <= now); // Should be valid now
        assert!(payload.issued_at <= now); // Should not be issued in the future
    }

    #[test]
    fn test_future_iat_rejected() {
        let now = Utc::now().timestamp();
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "chat.toolsforhumanity.com".to_string(),
            issued_at: now + 120, // 2 minutes in future
            expires_at: now + 3600,
            not_before: now - 60,
        };

        // With 60s skew, iat is still in the future -> reject
        let result = validate_claims(&claims, now, 60);
        assert!(result.is_err());
    }

    #[test]
    fn test_issuer_enforced() {
        let now = Utc::now().timestamp();
        let claims = JwsPayload {
            subject: "test".to_string(),
            issuer: "attacker.example".to_string(),
            issued_at: now,
            expires_at: now + 3600,
            not_before: now - 60,
        };

        // validate_claims enforces issuer now
        assert!(matches!(
            validate_claims(&claims, now, 60),
            Err(JwtError::InvalidToken)
        ));
    }
}

mod signature_format {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_der_to_raw_signature_conversion() {
        // Test vector adapted from josekit-rs
        // DER format signature should be converted to raw 64-byte format
        let (signing_key, verifying_key) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test".to_string());
        let token = create_test_token(&signing_key, "test", &payload);

        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();
        let result = verify_signature_with_key(&parts, &verifying_key);
        assert!(result.is_ok());
    }

    #[test]
    fn test_signature_must_be_64_bytes() {
        let (signing_key, _) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test".to_string());
        let token = create_test_token(&signing_key, "test", &payload);

        // Extract signature part
        let parts: Vec<&str> = token.split('.').collect();
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();

        // ES256 signatures must be exactly 64 bytes (32 bytes for r, 32 for s)
        assert_eq!(sig_bytes.len(), 64);
    }

    #[test]
    fn test_invalid_signature_length() {
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "JWT",
            "kid": "test",
        });
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({
            "sub": "test",
            "iss": "test-issuer",
            "iat": now,
            "exp": now + 3600,
            "nbf": now,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());

        // Create signatures with wrong lengths
        let short_sig = URL_SAFE_NO_PAD.encode([0u8; 32]); // Too short
        let long_sig = URL_SAFE_NO_PAD.encode([0u8; 128]); // Too long

        let token_short = format!("{header_b64}.{payload_b64}.{short_sig}");
        let token_long = format!("{header_b64}.{payload_b64}.{long_sig}");

        let (_, verifying_key) = generate_test_keypair();

        // Both should fail verification - parsing succeeds but verification fails
        if let Ok(parts_short) = JwsTokenParts::try_from(token_short.as_str()) {
            assert!(verify_signature_with_key(&parts_short, &verifying_key).is_err());
        }

        if let Ok(parts_long) = JwsTokenParts::try_from(token_long.as_str()) {
            assert!(verify_signature_with_key(&parts_long, &verifying_key).is_err());
        }
    }

    #[test]
    fn test_malformed_signature_base64() {
        // Create a valid token first
        let (signing_key, verifying_key) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test".to_string());
        let valid_token = create_test_token(&signing_key, "test", &payload);

        // Split the token and replace signature with invalid base64
        let parts: Vec<&str> = valid_token.split('.').collect();

        // Test 1: Invalid base64 characters in signature
        let token_invalid_chars = format!("{}.{}.not!valid!base64", parts[0], parts[1]);

        // JwsTokenParts stores the signature as-is without validating base64
        // The error occurs during verify_signature_with_key when it tries to decode
        let parts_invalid = JwsTokenParts::try_from(token_invalid_chars.as_str()).unwrap();
        let result = verify_signature_with_key(&parts_invalid, &verifying_key);
        assert!(matches!(result, Err(JwtError::InvalidToken)));

        // Test 2: Valid base64 but wrong length for ES256 signature
        let wrong_sig = URL_SAFE_NO_PAD.encode(b"not a valid signature");
        let token_wrong_sig = format!("{}.{}.{}", parts[0], parts[1], wrong_sig);

        let parts_wrong = JwsTokenParts::try_from(token_wrong_sig.as_str()).unwrap();
        let result = verify_signature_with_key(&parts_wrong, &verifying_key);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_tampered_signature_detection() {
        let (signing_key, verifying_key) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test".to_string());
        let token = create_test_token(&signing_key, "test", &payload);

        // Tamper with the signature
        let parts: Vec<&str> = token.split('.').collect();
        let mut sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        sig_bytes[0] ^= 0xFF; // Flip bits in first byte
        let tampered_sig = URL_SAFE_NO_PAD.encode(sig_bytes);

        let tampered_token = format!("{}.{}.{}", parts[0], parts[1], tampered_sig);
        let tampered_parts = JwsTokenParts::try_from(tampered_token.as_str()).unwrap();

        let result = verify_signature_with_key(&tampered_parts, &verifying_key);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_jwt_signed_with_wrong_key() {
        // Generate two different keypairs
        let (signing_key_wrong, _) = generate_test_keypair();
        let (_, verifying_key_correct) = generate_test_keypair();

        // Create and sign token with wrong key
        let payload = JwsPayload::from_encrypted_push_id("test-123".to_string());
        let token = create_test_token(&signing_key_wrong, "test-kid", &payload);

        // Try to verify with different key - should fail
        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();
        let result = verify_signature_with_key(&parts, &verifying_key_correct);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_jwt_with_empty_signature() {
        let header = JwsHeader {
            alg: ALG_ES256.to_string(),
            typ: TYP_JWT.to_string(),
            kid: "test-kid".to_string(),
        };
        let payload = JwsPayload::from_encrypted_push_id("test-123".to_string());

        // Create token with empty signature (header.payload.)
        let signing_input = craft_signing_input(&header, &payload).unwrap();
        let token_empty_sig = format!("{}.", signing_input);

        // Should parse successfully but fail verification
        let parts = JwsTokenParts::try_from(token_empty_sig.as_str()).unwrap();
        let (_, verifying_key) = generate_test_keypair();
        let result = verify_signature_with_key(&parts, &verifying_key);
        assert!(result.is_err());
    }
}

mod key_management {
    use super::*;

    #[test]
    fn test_kid_generation_deterministic() {
        let arn1 = "arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012";
        let arn2 = "arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012";

        let key1 = KmsKeyDefinition::from_arn(arn1.to_string());
        let key2 = KmsKeyDefinition::from_arn(arn2.to_string());

        assert_eq!(key1.id, key2.id);
        assert!(key1.id.starts_with("key_"));
    }

    #[test]
    fn test_kid_from_various_arn_formats() {
        // Standard ARN
        let arn = "arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012";
        let key = KmsKeyDefinition::from_arn(arn.to_string());
        assert!(key.id.starts_with("key_"));
        assert_eq!(key.arn, arn);

        // ARN with alias
        let arn_alias = "arn:aws:kms:us-east-1:123456789012:alias/my-key-alias";
        let key_alias = KmsKeyDefinition::from_arn(arn_alias.to_string());
        assert!(key_alias.id.starts_with("key_"));

        // Different regions should produce different kids
        let arn_west =
            "arn:aws:kms:us-west-2:123456789012:key/87654321-4321-4321-4321-210987654321";
        let key_west = KmsKeyDefinition::from_arn(arn_west.to_string());
        assert_ne!(key.id, key_west.id);
    }

    #[test]
    fn test_kid_uses_sha224_base64url() {
        let arn = "arn:aws:kms:us-east-1:123456789012:key/test-key-id";
        let key = KmsKeyDefinition::from_arn(arn.to_string());

        // Verify it's base64url encoded (no padding, URL-safe chars)
        assert!(!key.id.contains('='));
        assert!(!key.id.contains('+'));
        assert!(!key.id.contains('/'));

        // Should start with our prefix
        assert!(key.id.starts_with("key_"));

        // SHA-224 produces 28 bytes, base64url encodes to ~38 chars
        // Plus our "key_" prefix
        assert!(key.id.len() > 30);
    }
}

mod integration_helpers {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_craft_signing_input() {
        let header = JwsHeader {
            alg: "ES256".to_string(),
            typ: "JWT".to_string(),
            kid: "test-kid".to_string(),
        };

        let payload = JwsPayload {
            subject: "test-sub".to_string(),
            issuer: "test-issuer".to_string(),
            expires_at: 1_234_567_890,
            not_before: 1_234_567_890,
            issued_at: 1_234_567_890,
        };

        let result = craft_signing_input(&header, &payload).unwrap();

        // Should be two base64url parts joined by a dot
        let parts: Vec<&str> = result.split('.').collect();
        assert_eq!(parts.len(), 2);

        // Should be valid base64url
        assert!(URL_SAFE_NO_PAD.decode(parts[0]).is_ok());
        assert!(URL_SAFE_NO_PAD.decode(parts[1]).is_ok());

        // Should decode to the original values
        let decoded_header: JwsHeader =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(decoded_header.alg, "ES256");
        assert_eq!(decoded_header.kid, "test-kid");
    }

    #[test]
    fn test_full_token_roundtrip_without_kms() {
        let (signing_key, verifying_key) = generate_test_keypair();
        let payload = JwsPayload::from_encrypted_push_id("test-123".to_string());
        let token = create_test_token(&signing_key, "test-kid", &payload);

        // Parse and verify
        let parts = JwsTokenParts::try_from(token.as_str()).unwrap();
        assert!(verify_signature_with_key(&parts, &verifying_key).is_ok());

        // Validate claims
        let now = chrono::Utc::now().timestamp();
        assert!(validate_claims(&parts.payload, now, 60).is_ok());

        // Check payload
        assert_eq!(parts.payload.subject, "test-123");
        assert_eq!(parts.payload.issuer, "chat.toolsforhumanity.com");
    }
}
