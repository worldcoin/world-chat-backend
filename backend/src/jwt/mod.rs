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
mod jwk_ext;
mod signer;
mod types;

use aws_sdk_kms::Client as KmsClient;
use josekit::{jws::JwsHeader, jwt};

use types::KmsKeyDefinition;
// Export payload type for use in routes
pub use types::WorldChatJwtPayload;

use crate::types::Environment;
use serde_json::{Map, Value};

use error::JwtError;
use openssl::pkey::PKey;
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
    #[must_use]
    pub fn new(kms_client: KmsClient, environment: &Environment) -> Self {
        let key_arn = environment.jwt_kms_key_arn();
        let key = KmsKeyDefinition::from_arn(key_arn);

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
        header.set_key_id(self.key.id.clone());

        let signer = KmsEcdsaJwsSigner::new(self.kms_client.clone(), self.key.clone());

        let token = tokio::task::spawn_blocking(move || {
            jwt::encode_with_signer(&payload, &header, &signer)
        })
        .await??;

        Ok(token)
    }

    /// Build a single JWK (ES256, P-256) from the KMS public key using a custom
    /// `josekit::jwk::Jwk` extension.
    ///
    /// # Errors
    /// - `JwtError::JwksRetrievalError` that abstracts various errors due to KMS, OpenSSL, or josekit.
    // #[allow(clippy::redundant_closure_call)]
    pub async fn current_jwk(&self) -> Result<Map<String, Value>, JwtError> {
        // Use an async block to map all the errors to an anyhow::Error
        // to avoid mapping every error to JwtError manually, this would only fail if we can't retrive the public key from KMS
        let res: anyhow::Result<Map<String, Value>> = (async {
            let der = self
                .kms_client
                .get_public_key()
                .key_id(self.key.arn.clone())
                .send()
                .await?
                .public_key()
                .ok_or_else(|| anyhow::anyhow!("missing public key in KMS response"))?
                .as_ref()
                .to_vec();

            // Parse the DER public key
            let pkey = PKey::public_key_from_der(&der)?;

            // Use JWK extension to construct the JWK
            let jwk = {
                use crate::jwt::jwk_ext::JwkExt;
                josekit::jwk::Jwk::new_ec_p256_from_openssl(&pkey, self.key.id.clone())
            }?;

            Ok(jwk.into())
        })
        .await;

        res.map_err(JwtError::JwksRetrievalError)
    }
}
