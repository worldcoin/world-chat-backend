//! JWT token management using AWS KMS (ES256) and a stable `kid` derived from the key ARN.
//!
//! This module signs JWTs via AWS KMS using ECDSA P‑256 + SHA‑256 (ES256). The private
//! key never leaves KMS. The JWT header includes:
//! - `alg = "ES256"`
//! - `typ = "JWT"`
//! - `kid = <derived from KMS key ARN>`
//!
//! The `kid` is deterministic and safe to publish. It’s computed from the last path segment
//! of the KMS key identifier (key ID or alias) using SHA‑224 and base64url, prefixed with
//! `key_`. This enables easy key rotation and JWKS publication later on.

pub mod error;
mod signer;
mod types;

use aws_sdk_kms::Client as KmsClient;
use josekit::{jws::JwsHeader, jwt};

use types::KmsKeyDefinition;
// Export payload type for use in routes
pub use types::WorldChatJwtPayload;

use crate::types::Environment;

use self::signer::KmsEcdsaJwsSigner;
use error::JwtError;

/// JWT manager backed by AWS KMS (asymmetric ES256)
#[derive(Clone)]
pub struct JwtManager {
    kms_client: KmsClient,
    key: KmsKeyDefinition,
}

impl JwtManager {
    /// Creates a new JWT manager: derives kid from key ARN and prepares the signer.
    ///
    /// # Panics
    /// Panics if the KMS public key cannot be fetched or parsed
    pub fn new(kms_client: KmsClient, environment: &Environment) -> Self {
        let key_arn = environment.jwt_kms_key_arn();
        let key = KmsKeyDefinition::from_arn(key_arn);

        tracing::info!("KMS JWT manager initialized with key {}", key.id);

        Self { kms_client, key }
    }

    /// Issues a compact JWT signed by KMS.
    ///
    /// - Header: `alg=ES256`, `typ=JWT`, `kid=<derived>`
    /// - Claims: `{ sub, exp, iat }`, where `iat` is set to current time
    /// - Signing: Uses KMS `Sign(ECDSA_SHA_256)` over the `base64url(header).base64url(payload)`
    ///   bytes. KMS returns a DER-encoded ECDSA signature, which we convert to raw `r||s`.
    ///
    /// # Errors
    /// Returns `JwtError::SigningError` if KMS signing fails or the signature cannot be
    /// converted from DER to raw.
    pub async fn issue_token(&self, payload: WorldChatJwtPayload) -> Result<String, JwtError> {
        // Prepare JWS header and payload
        let payload = payload.generate_jwt_payload();
        let mut header = JwsHeader::new();
        header.set_token_type("JWT");

        let signer = KmsEcdsaJwsSigner::new(self.kms_client.clone(), self.key.clone());

        let token = tokio::task::spawn_blocking(move || {
            jwt::encode_with_signer(&payload, &header, &signer)
        })
        .await??;

        Ok(token)
    }

    // validate_token intentionally removed for now (not used)
}
