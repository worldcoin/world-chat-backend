use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::jwt::error::JwtError;
// no extra serde imports needed here

/// Default access token lifetime.
pub const TOKEN_EXPIRATION: Duration = Duration::days(7);

/// Compact JWS header used for ES256 tokens.
///
/// Fields follow RFC 7515/7518 conventions:
/// - `alg`: algorithm, fixed to "ES256"
/// - `typ`: token type, fixed to "JWT"
/// - `kid`: key identifier derived from the AWS KMS key ARN
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwsHeader {
    pub alg: String,
    pub typ: String,
    pub kid: String,
}

/// World Chat JWT claims (payload). Minimal subset we use today.
///
/// Times are seconds since epoch. Optional to accommodate different flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwsPayload {
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "iss")]
    pub issuer: String,
    #[serde(rename = "iat", skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<i64>,
    #[serde(rename = "exp", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(rename = "nbf", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<i64>,
}

impl JwsPayload {
    #[must_use]
    pub fn from_encrypted_push_id(encrypted_push_id: String) -> Self {
        let now = Utc::now().timestamp();
        let exp = (Utc::now() + TOKEN_EXPIRATION).timestamp();
        Self {
            subject: encrypted_push_id,
            issuer: "chat.toolsforhumanity.com".to_owned(),
            issued_at: Some(now),
            expires_at: Some(exp),
            not_before: Some(now),
        }
    }
}

/// Definition of the KMS key used for signing/verifying JWTs.
///
/// `id` is a stable `kid` derived from the ARN (SHA-224, base64url, prefixed),
/// so we can rotate keys without breaking validation routing.
#[derive(Debug, Clone)]
pub struct KmsKeyDefinition {
    pub id: String,
    pub arn: String,
}

impl KmsKeyDefinition {
    #[must_use]
    pub fn from_arn(arn: String) -> Self {
        let last = arn.split('/').next_back().unwrap_or(&arn);
        let hash = sha2::Sha224::digest(last.as_bytes());
        let encoded = URL_SAFE_NO_PAD.encode(hash);
        let id = format!("key_{encoded}");
        Self { id, arn }
    }
}

/// Borrowing view over a compact JWS (header.payload.signature)
/// used only during validation to avoid allocations.
pub struct JwsTokenParts<'a> {
    pub header_b64: &'a str,
    pub payload_b64: &'a str,
    pub signature: &'a str,
    pub header: JwsHeader,
    pub payload: JwsPayload,
}

impl<'a> TryFrom<&'a str> for JwsTokenParts<'a> {
    type Error = JwtError;
    fn try_from(token: &'a str) -> Result<Self, Self::Error> {
        let mut parts = token.split('.');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(h_b64), Some(p_b64), Some(s)) if parts.next().is_none() => {
                let header_bytes = URL_SAFE_NO_PAD
                    .decode(h_b64)
                    .map_err(|_| JwtError::InvalidToken)?;
                let header_decoded: JwsHeader =
                    serde_json::from_slice(&header_bytes).map_err(|_| JwtError::InvalidToken)?;

                let payload_bytes = URL_SAFE_NO_PAD
                    .decode(p_b64)
                    .map_err(|_| JwtError::InvalidToken)?;
                let payload_decoded: JwsPayload =
                    serde_json::from_slice(&payload_bytes).map_err(|_| JwtError::InvalidToken)?;

                Ok(Self {
                    header_b64: h_b64,
                    payload_b64: p_b64,
                    signature: s,
                    header: header_decoded,
                    payload: payload_decoded,
                })
            }
            _ => Err(JwtError::InvalidToken),
        }
    }
}
