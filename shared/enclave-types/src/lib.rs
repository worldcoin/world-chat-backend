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
}

impl Request for EnclaveInitializeRequest {
    const ROUTE_ID: &'static str = "/v1/initialize";
    type Response = ();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveHealthCheckRequest;

impl Request for EnclaveHealthCheckRequest {
    const ROUTE_ID: &'static str = "/v1/health-check";
    type Response = Result<(), EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclavePublicKeyRequest;

impl Request for EnclavePublicKeyRequest {
    const ROUTE_ID: &'static str = "/v1/public-key";
    type Response = Result<EnclavePublicKeyResponse, EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclavePublicKeyResponse {
    /// Attestation document bytes
    pub attestation: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclavePushIdChallengeRequest {
    pub encrypted_push_id_1: String,
    pub encrypted_push_id_2: String,
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
