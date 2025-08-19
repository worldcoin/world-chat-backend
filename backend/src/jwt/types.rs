use std::time::SystemTime;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use josekit::jwt::JwtPayload;
use sha2::Digest;

/// Token expiration time in seconds (7 days)
pub const TOKEN_EXPIRATION_SECS: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

pub struct WorldChatJwtPayload {
    pub encrypted_push_id: String,
}

impl WorldChatJwtPayload {
    #[must_use]
    pub fn generate_jwt_payload(&self) -> JwtPayload {
        let mut payload = JwtPayload::new();

        payload.set_issued_at(&SystemTime::now());
        payload.set_expires_at(&(SystemTime::now() + TOKEN_EXPIRATION_SECS));
        payload.set_subject(self.encrypted_push_id.clone());

        payload
    }
}

#[derive(Debug, Clone)]
pub struct KmsKeyDefinition {
    pub id: String,
    pub arn: String,
}

impl KmsKeyDefinition {
    /// Build a key definition from a full key ARN (or alias ARN), deriving a stable `id`.
    #[must_use]
    pub fn from_arn(arn: String) -> Self {
        let last = arn.split('/').next_back().unwrap_or(&arn);
        let hash = sha2::Sha224::digest(last.as_bytes());
        let encoded = URL_SAFE_NO_PAD.encode(hash);
        let id = format!("key_{encoded}");
        Self { id, arn }
    }
}
