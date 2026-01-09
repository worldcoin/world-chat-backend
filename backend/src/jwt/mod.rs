//! JWT/JWS management using AWS KMS (ES256) with `p256` verification.
//!
//! Overview:
//! - We hand-roll compact JWS (header.payload.signature) per RFC 7515/7518
//! - Signing uses AWS KMS `Sign` with `EcdsaSha256` over the compact input
//! - KMS returns DER-encoded ECDSA signatures; we convert to raw r||s
//! - Verification uses `p256`'s `VerifyingKey` over SHA-256 of the compact input
//! - `kid` is derived deterministically from the KMS key ARN and embedded in the header
//!
//! Rationale:
//! - Most rust jwt libraries didn't support external signing
//! - From the libraries that did, they supported only synchronous signing, leading to sync/async gymnastics
//! - We also wanted to avoid using OpenSSL
//! - eg. `josekit` `JwsSigner` trait is sync and used OpenSSL
//! - eg. `jwt-compact` even though it didn't use OpenSSL it still had a sync `Algorithm` trait without a good support for Errors

pub mod error;
mod types;

use error::JwtError;
pub use types::{JwsPayload, KmsKeyDefinition};

use aws_sdk_kms::{
    primitives::Blob,
    types::{MessageType, SigningAlgorithmSpec},
    Client as KmsClient,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use p256::ecdsa::{signature::DigestVerifier, Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
// use serde::de::DeserializeOwned; // no longer needed
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::{
    jwt::types::{JwsHeader, JwsTokenParts},
    types::Environment,
};

const ALG_ES256: &str = "ES256";
const TYP_JWT: &str = "JWT";
const MAX_SKEW_SECS: i64 = 60;

// removed helper: decoding now lives on `JwsTokenParts`

#[derive(Clone)]
pub struct JwtManager {
    verifying_key: VerifyingKey,
    kid: String,
    kms_client: Arc<KmsClient>,
    key_arn: String,
    pub issuer: String,
}

impl JwtManager {
    /// Create a new JWT manager backed by AWS KMS.
    ///
    /// # Errors
    /// Returns an error if the KMS public key cannot be retrieved or parsed.
    pub async fn new(
        kms_client: Arc<KmsClient>,
        environment: &Environment,
    ) -> Result<Self, JwtError> {
        let key = KmsKeyDefinition::from_arn(environment.jwt_kms_key_arn());
        let spki = kms_client
            .get_public_key()
            .key_id(&key.arn)
            .send()
            .await
            .map_err(|e| JwtError::Kms(Box::new(e.into())))?
            .public_key()
            .ok_or_else(|| anyhow::anyhow!("missing public key in KMS response"))?
            .as_ref()
            .to_vec();

        let verifying_key =
            VerifyingKey::from_public_key_der(&spki).map_err(|e| JwtError::Other(e.into()))?;
        Ok(Self {
            verifying_key,
            kid: key.id,
            kms_client,
            key_arn: key.arn,
            issuer: environment.jwt_issuer_url(),
        })
    }

    /// Issue a compact JWS (JWT) string using ES256 via AWS KMS.
    ///
    /// # Errors
    /// Returns an error if header/payload serialization fails or KMS signing fails.
    pub async fn issue_token(&self, payload: &JwsPayload) -> Result<String, JwtError> {
        let header = JwsHeader {
            alg: ALG_ES256.to_string(),
            typ: TYP_JWT.to_string(),
            kid: self.kid.clone(),
        };
        let signing_input = craft_signing_input(&header, payload)?;

        // Sign via KMS asynchronously and convert DER -> raw (r||s).
        let der_sig = self
            .kms_client
            .sign()
            .key_id(&self.key_arn)
            .message(Blob::new(signing_input.as_bytes()))
            .message_type(MessageType::Raw)
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(|e| JwtError::Kms(Box::new(e.into())))?
            .signature
            .ok_or_else(|| anyhow::anyhow!("empty signature from KMS"))?;

        let sig = Signature::from_der(der_sig.as_ref())
            .map_err(|e| JwtError::Other(e.into()))?
            .to_bytes();
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig);

        let mut token = signing_input;
        token.push('.');
        token.push_str(&sig_b64);
        Ok(token)
    }

    /// Validate a compact JWS (JWT) string and return parsed claims on success.
    ///
    /// # Arguments
    /// * `token_str` - The compact JWS token string to validate
    /// * `issued_after` - Optional cutoff timestamp; reject tokens with `iat` before this time
    ///
    /// # Errors
    /// Returns an error if parsing fails, header is unexpected, signature is invalid,
    /// or time-based claims fail validation.
    pub fn validate(
        &self,
        token_str: &str,
        issued_after: Option<i64>,
    ) -> Result<JwsPayload, JwtError> {
        let parts = JwsTokenParts::try_from(token_str)?;

        // Header checks: enforce alg, typ, and kid to prevent alg confusion
        let header: &JwsHeader = &parts.header;
        if header.alg != ALG_ES256 || header.typ != TYP_JWT || header.kid != self.kid {
            return Err(JwtError::InvalidToken);
        }

        // Signature verification
        verify_signature_with_key(&parts, &self.verifying_key)?;

        // Claims + time validation with small skew
        let claims: JwsPayload = parts.payload;
        let now = chrono::Utc::now().timestamp();
        validate_claims(&claims, now, MAX_SKEW_SECS, &self.issuer)?;

        // Cutoff check: reject tokens issued before the cutoff timestamp
        if let Some(cutoff) = issued_after {
            if claims.issued_at < cutoff {
                return Err(JwtError::InvalidToken);
            }
        }

        Ok(claims)
    }
}

// Extracted functions for testability

/// Verify ES256 signature over the compact input using a known key.
pub(crate) fn verify_signature_with_key(
    parts: &JwsTokenParts<'_>,
    verifying_key: &VerifyingKey,
) -> Result<(), JwtError> {
    let mut digest = Sha256::new();
    digest.update(parts.header_b64.as_bytes());
    digest.update(b".");
    digest.update(parts.payload_b64.as_bytes());

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts.signature)
        .map_err(|_| JwtError::InvalidToken)?;
    let sig = Signature::try_from(sig_bytes.as_slice()).map_err(|_| JwtError::InvalidSignature)?;
    verifying_key
        .verify_digest(digest, &sig)
        .map_err(|_| JwtError::InvalidSignature)
}

/// Validate issuer, `nbf`, `exp`, and `iat` with a small clock skew allowance.
pub(crate) fn validate_claims(
    claims: &JwsPayload,
    now: i64,
    skew: i64,
    expected_issuer: &str,
) -> Result<(), JwtError> {
    // Enforce known issuer
    if claims.issuer != expected_issuer {
        return Err(JwtError::InvalidToken);
    }
    if now + skew < claims.not_before {
        return Err(JwtError::InvalidToken);
    }
    if now - skew >= claims.expires_at {
        return Err(JwtError::InvalidToken);
    }
    // Follow josekit validator practice: iat must not be in the future.
    if claims.issued_at > now + skew {
        return Err(JwtError::InvalidToken);
    }
    Ok(())
}

/// Serialize + base64url-encode header and payload, and join with a dot.
pub(crate) fn craft_signing_input(
    header: &JwsHeader,
    payload: &JwsPayload,
) -> Result<String, JwtError> {
    let header_json = serde_json::to_vec(header)
        .map_err(|e| JwtError::SigningInput(format!("serialize header: {e}")))?;
    let payload_json = serde_json::to_vec(payload)
        .map_err(|e| JwtError::SigningInput(format!("serialize payload: {e}")))?;

    let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);

    let mut signing_input = String::with_capacity(header_b64.len() + 1 + payload_b64.len());
    signing_input.push_str(&header_b64);
    signing_input.push('.');
    signing_input.push_str(&payload_b64);
    Ok(signing_input)
}

// issuer validation is handled inside validate_claims

#[cfg(test)]
mod tests;
