use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Main request type sent from the enclave-worker to the secure-enclave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnclaveRequest {
    /// Request to initialize the enclave with Braze configuration
    Initialize(EnclaveConfig),
    /// Health check request
    HealthCheck,
}

/// Response type sent from the secure-enclave back to the enclave-worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnclaveResponse {
    /// Initialization successful
    InitializeSuccess,
    /// Health check response
    HealthCheck { initialized: bool },
    /// Error response
    Error(EnclaveError),
}

/// Braze API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveConfig {
    /// Braze API key
    pub braze_api_key: String,
    /// Braze REST endpoint URL (e.g., https://rest.iad-01.braze.com)
    pub braze_api_endpoint: String,
    /// Enclave HTTP proxy port
    pub braze_http_proxy_port: u32,
}

/// Enclave errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum EnclaveError {
    #[error("Enclave not initialized. Call Initialize first.")]
    NotInitialized,
    #[error("Unexpected response type")]
    UnexpectedResponse,
}
