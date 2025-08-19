use aws_sdk_kms::{
    primitives::Blob,
    types::{MessageType, SigningAlgorithmSpec},
    Client as KmsClient,
};
use josekit::{
    jws::{alg::ecdsa::EcdsaJwsAlgorithm, JwsAlgorithm, JwsSigner},
    util::der::{DerReader, DerType},
    JoseError,
};
use tokio::runtime::Runtime;

use crate::jwt::KmsKeyDefinition;

/// Synchronous JWS signer that delegates ES256 signing to AWS KMS.
///
/// `josekit`'s `JwsSigner` trait uses a synchronous `sign()` API, while AWS KMS is
/// asynchronous. To bridge this mismatch safely:
/// - `sign()` creates a fresh Tokio runtime per call and `block_on`s an async KMS `Sign`.
/// - Callers invoke `encode_with_signer(...)` inside `spawn_blocking` to avoid blocking the
///   async scheduler thread while we synchronously wait for KMS.
/// - KMS returns a DER‑encoded ECDSA signature which we map into the raw `r || s` format
///   required by JWS for ES256.
///
/// **Note:** `josekit` doesn't support async signers, if we see that switching between sync and async context,
/// we can implement a custom async signer.
#[derive(Clone, Debug)]
pub struct KmsEcdsaJwsSigner {
    pub kms_client: KmsClient,
    pub key: KmsKeyDefinition,
}

impl KmsEcdsaJwsSigner {
    pub const fn new(kms_client: KmsClient, key: KmsKeyDefinition) -> Self {
        Self { kms_client, key }
    }
}

impl JwsSigner for KmsEcdsaJwsSigner {
    fn algorithm(&self) -> &dyn JwsAlgorithm {
        &EcdsaJwsAlgorithm::Es256
    }

    fn signature_len(&self) -> usize {
        64 // 64 bytes for ES256
    }

    fn key_id(&self) -> Option<&str> {
        Some(self.key.id.as_str())
    }

    /// Sign the JWS payload using AWS KMS with ES256.
    ///
    /// Steps:
    /// 1. Create a fresh runtime and `block_on` the KMS `Sign` call.
    /// 2. Convert the DER‑encoded ECDSA signature to the raw concatenated `r || s` form.
    /// 3. Return the 64‑byte signature as required by ES256.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, JoseError> {
        (|| -> Result<Vec<u8>, anyhow::Error> {
            // Create a fresh runtime per call and block on KMS
            let rt = Runtime::new().map_err(|e| JoseError::InvalidSignature(anyhow::anyhow!(e)))?;
            let der_signature = rt.block_on(async {
                sign_with_kms(&self.kms_client, &self.key.arn, message).await
            })?;

            // NOTE: Code below is kept as is from the original `josekit` implementation

            let signature_len = self.signature_len();
            let sep = signature_len / 2;

            let mut signature = Vec::with_capacity(signature_len);
            let mut reader = DerReader::from_bytes(&der_signature);
            match reader.next()? {
                Some(DerType::Sequence) => {}
                _ => unreachable!("A generated signature is invalid."),
            }
            match reader.next()? {
                Some(DerType::Integer) => {
                    signature.extend_from_slice(&reader.to_be_bytes(false, sep));
                }
                _ => unreachable!("A generated signature is invalid."),
            }
            match reader.next()? {
                Some(DerType::Integer) => {
                    signature.extend_from_slice(&reader.to_be_bytes(false, sep));
                }
                _ => unreachable!("A generated signature is invalid."),
            }

            Ok(signature)
        })()
        .map_err(|e| JoseError::InvalidSignature(anyhow::anyhow!(e)))
    }

    fn box_clone(&self) -> Box<dyn JwsSigner> {
        Box::new(self.clone())
    }
}

/// Perform the actual AWS KMS `Sign` request with `ECDSA_SHA_256` over the provided bytes.
///
/// Returns the DER‑encoded signature from KMS. The caller is responsible for converting
/// it into raw `r || s` form for JWS.
async fn sign_with_kms(
    client: &aws_sdk_kms::Client,
    key_id: &str,
    message: &[u8],
) -> Result<Vec<u8>, anyhow::Error> {
    let result = client
        .sign()
        .key_id(key_id)
        .message(Blob::new(message))
        .message_type(MessageType::Raw)
        .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
        .send()
        .await?;

    result.signature.map_or_else(
        || Err(anyhow::anyhow!("No signature returned from KMS")),
        |signature| Ok(signature.as_ref().to_vec()),
    )
}
