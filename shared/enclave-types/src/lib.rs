use pontifex::Request;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum EnclaveError {
    #[error("Enclave not initialized. Call Initialize first.")]
    NotInitialized,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveInitializeResponse {
    /// Success message
    pub success: bool,
}

impl Request for EnclaveInitializeRequest {
    const ROUTE_ID: &'static str = "/v1/initialize";
    type Response = Result<EnclaveInitializeResponse, EnclaveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveHealthCheckRequest;

impl Request for EnclaveHealthCheckRequest {
    const ROUTE_ID: &'static str = "/v1/health-check";
    type Response = Result<(), EnclaveError>;
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
