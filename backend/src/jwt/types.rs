use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::jwt::error::JwtError;

pub const TOKEN_EXPIRATION: Duration = Duration::days(7);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwsHeader {
    #[serde(rename = "alg")]
    pub alg: String,
    #[serde(rename = "typ")]
    pub typ: String,
    #[serde(rename = "kid")]
    pub kid: String,
}

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

pub struct JwsTokenParts<'a> {
    pub header: &'a str,
    pub payload: &'a str,
    pub signature: &'a str,
}

impl<'a> TryFrom<&'a str> for JwsTokenParts<'a> {
    type Error = JwtError;
    fn try_from(token: &'a str) -> Result<Self, Self::Error> {
        let mut parts = token.split('.');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(h), Some(p), Some(s)) if parts.next().is_none() => Ok(Self {
                header: h,
                payload: p,
                signature: s,
            }),
            _ => Err(JwtError::InvalidToken),
        }
    }
}
