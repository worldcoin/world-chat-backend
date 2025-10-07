use pontifex::Request;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum EnclaveError {
    #[error("Enclave not initialized. Call Initialize first.")]
    NotInitialized,
    #[error("Secure module not initialized")]
    SecureModuleNotInitialized,
    // TODO: Add source pontifex attestation error (it's missing serialization decorator now)
    #[error("Attestation failed")]
    AttestationFailed(),
    #[error("Failed to send request to Braze: {0}")]
    BrazeRequestFailed(String),
    #[error("Failed to decrypt push ID: {0}")]
    DecryptPushIdFailed(String),
    #[error("Failed to create key pair from secret key")]
    KeyPairCreationFailed,
    #[error("Pontifex client error: {0}")]
    PontifexError(String),
}

/// Braze API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveInitializeRequest {
    /// Braze API key
    pub braze_api_key: String,
    /// Braze REST API region (e.g., https://rest.{braze_api_region}.braze.com)
    pub braze_api_region: String,
    /// Enclave HTTP proxy port
    pub braze_http_proxy_port: u32,
    /// Context:
    /// We're deploying enclaves in sets, we only want to the first enclave to generate a key pair.
    /// This secret key can be shared **safely** using cryptographic attestation between trusted enclaves.
    ///
    /// This flag is used to determine if the enclave should generate a key pair on initialization.
    pub generate_key_pair: bool,
    /// Enclave cluster proxy port
    ///
    /// This port is used to proxy pontifex requests to other enclaves in the same cluster.
    /// It's used to request a secret key from other live enclaves.
    pub enclave_cluster_proxy_port: u32,
}

impl Request for EnclaveInitializeRequest {
    const ROUTE_ID: &'static str = "/v1/initialize";
    type Response = Result<(), EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveHealthCheckRequest;

impl Request for EnclaveHealthCheckRequest {
    const ROUTE_ID: &'static str = "/v1/health-check";
    type Response = Result<(), EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveAttestationDocRequest;

impl Request for EnclaveAttestationDocRequest {
    const ROUTE_ID: &'static str = "/v1/attestation-doc";
    type Response = Result<EnclaveAttestationDocResponse, EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveAttestationDocResponse {
    /// Attestation document bytes
    pub attestation: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclavePushIdChallengeRequest {
    pub encrypted_push_id_1: Vec<u8>,
    pub encrypted_push_id_2: Vec<u8>,
}

impl Request for EnclavePushIdChallengeRequest {
    const ROUTE_ID: &'static str = "/v1/push-id-challenge";
    type Response = Result<bool, EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveNotificationRequest {
    /// Topic for the notification
    pub topic: String,
    /// Encrypted Push IDs of the subscribers
    pub subscribed_encrypted_push_ids: Vec<String>,
    /// Encrypted Message Base64 encoded
    pub encrypted_message_base64: String,
}

impl Request for EnclaveNotificationRequest {
    const ROUTE_ID: &'static str = "/v1/notification";
    type Response = Result<(), EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveSecretKeyRequest {
    /// Attestation document
    pub attestation_doc: Vec<u8>,
}

impl Request for EnclaveSecretKeyRequest {
    const ROUTE_ID: &'static str = "/v1/secret-key";
    type Response = Result<Vec<u8>, EnclaveError>;
}
