use std::time::SystemTime;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use josekit::jwt::JwtPayload;
use sha2::Digest;

/// Token expiration window for issued access tokens.
///
/// Currently set to 7 days. Adjust this constant to tune session lifetime.
pub const TOKEN_EXPIRATION_SECS: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// High‑level payload used by the application when issuing JWTs.
///
/// The fields in this struct are translated into standard JWT claims by
/// [`WorldChatJwtPayload::generate_jwt_payload`]. Today we only include a
/// stable subject (`sub`) which is the `encrypted_push_id`, along with
/// `iat` and `exp` timestamps.
pub struct WorldChatJwtPayload {
    pub encrypted_push_id: String,
}

impl WorldChatJwtPayload {
    /// Convert this high‑level payload into a concrete `josekit::jwt::JwtPayload`.
    ///
    /// Claims set:
    /// - `sub`: the `encrypted_push_id` used by the client
    /// - `iat`: current time
    /// - `exp`: `iat + TOKEN_EXPIRATION_SECS`
    #[must_use]
    pub fn generate_jwt_payload(&self) -> JwtPayload {
        let mut payload = JwtPayload::new();

        payload.set_issued_at(&SystemTime::now());
        payload.set_expires_at(&(SystemTime::now() + TOKEN_EXPIRATION_SECS));
        payload.set_subject(self.encrypted_push_id.clone());
        payload.set_issuer("chat.toolsforhumanity.com");

        payload
    }
}

/// Describes the KMS key used to sign JWTs and the stable `kid` we publish.
///
/// - `arn`: Full KMS key identifier (key ID or alias ARN).
/// - `id` (kid): Deterministic identifier derived from the last ARN segment,
///   hashed with SHA‑224 and base64url‑encoded without padding, prefixed with
///   `key_`. This keeps the `kid` stable, short, and safe to share publicly.
#[derive(Debug, Clone)]
pub struct KmsKeyDefinition {
    pub id: String,
    pub arn: String,
}

impl KmsKeyDefinition {
    /// Build a key definition from a full key ARN (or alias ARN), deriving a stable `id`.
    ///
    /// Rationale for custom `kid` format:
    /// - Stability: only the last path segment (key UUID or alias name) is used.
    /// - Privacy: hashing avoids leaking full ARNs, accounts, or region details.
    /// - Interop: base64url without padding is URL/JWT friendly; the `key_` prefix
    ///   provides an explicit namespace that is easy to filter in logs and JWKS.
    #[must_use]
    pub fn from_arn(arn: String) -> Self {
        let last = arn.split('/').next_back().unwrap_or(&arn);
        let hash = sha2::Sha224::digest(last.as_bytes());
        let encoded = URL_SAFE_NO_PAD.encode(hash);
        let id = format!("key_{encoded}");
        Self { id, arn }
    }
}
