//! JWT token management using AWS KMS (ES256) and derived kid from key ARN

pub mod error;

use aws_sdk_kms::{
    primitives::Blob,
    types::{MessageType, SigningAlgorithmSpec},
    Client as KmsClient,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use josekit::{
    jws::{JwsHeader, ES256},
    jwt,
    util::der::{DerReader, DerType},
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::types::Environment;
use error::JwtError;

/// JWT manager backed by AWS KMS (asymmetric ES256)
#[derive(Clone)]
pub struct JwtManager {
    kms_client: KmsClient,
    key_arn: String,
    kid: String,
    public_key_der: Vec<u8>,
    leeway_secs: i64,
}

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject - the encrypted push ID
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
}

impl JwtManager {
    /// Creates a new JWT manager: fetches public key and derives kid from key ARN.
    ///
    /// # Panics
    /// Panics if the KMS public key cannot be fetched or parsed
    pub async fn new(kms_client: KmsClient, environment: &Environment) -> Self {
        let key_arn = environment.jwt_kms_key_arn();
        let kid = derive_kid_from_arn(&key_arn);

        // Fetch public key once and cache DER
        let public_key = kms_client
            .get_public_key()
            .key_id(key_arn.clone())
            .send()
            .await
            .expect("Failed to fetch public key from KMS");
        let public_key_der = public_key
            .public_key()
            .expect("KMS public key missing")
            .as_ref()
            .to_vec();

        tracing::info!("KMS JWT manager initialized with key {kid}");

        Self {
            kms_client,
            key_arn,
            kid,
            public_key_der,
            leeway_secs: 60,
        }
    }

    /// Issues a JWT token with the given subject and expiry time.
    ///
    /// # Errors
    /// Returns `JwtError` if signing fails
    pub async fn issue_token(
        &self,
        encrypted_push_id: &str,
        expiry_secs: i64,
    ) -> Result<String, JwtError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: encrypted_push_id.to_string(),
            exp: now + expiry_secs,
            iat: now,
        };

        // Build header
        let mut header = JwsHeader::new();
        header.set_token_type("JWT");
        header.set_algorithm("ES256");
        header.set_key_id(self.kid.clone());

        // Serialize header and payload
        let header_json = header.to_string();
        let payload_json =
            serde_json::to_string(&claims).map_err(|e| JwtError::SigningError(e.to_string()))?;

        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header_json),
            URL_SAFE_NO_PAD.encode(payload_json)
        );

        // KMS sign (message is the signing input bytes)
        let sign_output = self
            .kms_client
            .sign()
            .key_id(self.key_arn.clone())
            .message(Blob::new(signing_input.as_bytes()))
            .message_type(MessageType::Raw)
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(|e| JwtError::SigningError(e.to_string()))?;

        let der_signature = sign_output
            .signature()
            .ok_or_else(|| JwtError::SigningError("No signature returned from KMS".to_string()))?;

        // Convert DER signature to raw r||s
        let raw_signature =
            der_to_raw_signature(der_signature.as_ref(), 64).map_err(JwtError::SigningError)?;

        let jwt = format!(
            "{}.{}",
            signing_input,
            URL_SAFE_NO_PAD.encode(raw_signature)
        );

        Ok(jwt)
    }

    /// Validates a JWT token and returns the claims if valid.
    ///
    /// # Errors
    /// Returns `JwtError` if token is invalid or expired
    pub fn validate_token(&self, token: &str) -> Result<Claims, JwtError> {
        // Verify signature
        let verifier = ES256
            .verifier_from_der(&self.public_key_der)
            .map_err(|e| JwtError::PublicKeyLoadError(e.to_string()))?;

        let (payload, header) =
            jwt::decode_with_verifier(token, &verifier).map_err(|_| JwtError::ValidationError)?;

        // Enforce alg and kid
        if header.claim("alg").and_then(|v| v.as_str()) != Some("ES256") {
            return Err(JwtError::HeaderError("Unexpected alg".to_string()));
        }
        if let Some(kid) = header.claim("kid").and_then(|v| v.as_str()) {
            if kid != self.kid {
                return Err(JwtError::HeaderError("Unexpected kid".to_string()));
            }
        }

        // Parse and validate claims
        let claims: Claims =
            serde_json::from_str(&payload.to_string()).map_err(|_| JwtError::ValidationError)?;

        let now = Utc::now().timestamp();
        if claims.exp <= now {
            return Err(JwtError::ValidationError);
        }
        if claims.iat > now + self.leeway_secs {
            return Err(JwtError::ValidationError);
        }

        Ok(claims)
    }
}

fn derive_kid_from_arn(arn: &str) -> String {
    let last = arn.split('/').next_back().unwrap_or(arn);
    let hash = sha2::Sha224::digest(last.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(hash);
    format!("key_{encoded}")
}

fn der_to_raw_signature(der: &[u8], signature_len: usize) -> Result<Vec<u8>, String> {
    let mut signature = Vec::with_capacity(signature_len);
    let der_vec = der.to_vec();
    let mut reader = DerReader::from_bytes(&der_vec);
    match reader.next() {
        Ok(Some(DerType::Sequence)) => {}
        _ => return Err("Invalid DER signature".to_string()),
    }
    match reader.next() {
        Ok(Some(DerType::Integer)) => {
            signature.extend_from_slice(&reader.to_be_bytes(false, signature_len / 2));
        }
        _ => return Err("Invalid DER signature".to_string()),
    }
    match reader.next() {
        Ok(Some(DerType::Integer)) => {
            signature.extend_from_slice(&reader.to_be_bytes(false, signature_len / 2));
        }
        _ => return Err("Invalid DER signature".to_string()),
    }
    Ok(signature)
}
