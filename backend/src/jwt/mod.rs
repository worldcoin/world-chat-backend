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
//! `key_`. This enables easy key rotation and potential JWKS publication later on.

pub mod error;
mod signer;
mod types;

use aws_sdk_kms::Client as KmsClient;
use josekit::{jws::JwsHeader, jwt};

use types::KmsKeyDefinition;
// Export payload type for use in routes
pub use types::WorldChatJwtPayload;

use crate::types::Environment;

use error::JwtError;
use signer::KmsEcdsaJwsSigner;

/// High‑level JWT manager backed by AWS KMS (ES256).
///
/// Responsibilities:
/// - Holds the KMS client and the selected key (`KmsKeyDefinition`).
/// - Issues compact JWS/JWT tokens using `josekit` and a custom signer that delegates to KMS.
/// - Ensures headers are set consistently (`alg`, `typ`, `kid`).
#[derive(Clone)]
pub struct JwtManager {
    kms_client: KmsClient,
    key: KmsKeyDefinition,
}

impl JwtManager {
    /// Creates a new JWT manager from the provided environment.
    ///
    /// - Reads the JWT KMS key ARN from `Environment`.
    /// - Derives a stable, publishable `kid` from that ARN via `KmsKeyDefinition::from_arn`.
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
    /// - Signing: Uses KMS `Sign(ECDSA_SHA_256)` via a custom `JwsSigner`. Because `JwsSigner`
    ///   is synchronous, we call `encode_with_signer` from `spawn_blocking` to avoid blocking
    ///   the async runtime while the signer internally bridges to async (KMS) for each sign.
    ///
    /// # Errors
    /// - `JwtError::JoseKitError` for JOSE/JWS encoding or signature mapping issues.
    /// - `JwtError::JoinError` if the blocking task fails to join.
    /// - `JwtError::ValidationError` is reserved for decode/verify flows.
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
}
