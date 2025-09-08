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

/// Type-safe enclave request-response pairs.
///
/// This trait solves a fundamental problem: when sending `EnclaveRequest::HealthCheck`,
/// there's no compile-time guarantee you'll receive `EnclaveResponse::HealthCheck` back.
/// This leads to error-prone pattern matching with unreachable branches.
///
/// By associating each request with its expected response type at compile time, we:
/// - Eliminate runtime errors from mismatched request/response variants
/// - Remove boilerplate pattern matching in client code
/// - Make the API self-documenting through types
///
/// ## Example
/// ```rust,ignore
/// // Before: Must handle impossible cases
/// let response = client.send_request(EnclaveRequest::HealthCheck).await?;
/// match response {
///     EnclaveResponse::HealthCheck { initialized } => Ok(initialized),
///     _ => unreachable!(), // This shouldn't happen but we must handle it
/// }
///
/// // After: Direct, type-safe
/// let initialized = client.send(HealthCheckRequest).await?;
/// // Returns bool directly, no pattern matching needed
/// ```
pub trait EnclaveRequestType: Serialize + Send {
    /// The type of response this request expects to receive
    type Response: DeserializeOwned + Send;

    /// Convert this typed request into the generic EnclaveRequest enum
    fn into_request(self) -> EnclaveRequest;

    /// Extract the typed response from the generic EnclaveResponse enum
    ///
    /// Returns an error if the response variant doesn't match what this request expects
    fn from_response(response: EnclaveResponse) -> Result<Self::Response, EnclaveError>;
}

/// Health check request - returns whether the enclave is initialized.
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheckRequest;

impl EnclaveRequestType for HealthCheckRequest {
    type Response = bool; // represents `initialized` field

    fn into_request(self) -> EnclaveRequest {
        EnclaveRequest::HealthCheck
    }

    fn from_response(response: EnclaveResponse) -> Result<Self::Response, EnclaveError> {
        match response {
            EnclaveResponse::HealthCheck { initialized } => Ok(initialized),
            EnclaveResponse::Error(e) => Err(e),
            _ => Err(EnclaveError::UnexpectedResponse),
        }
    }
}

/// Initialize request - configures the enclave with Braze API credentials.
#[derive(Debug, Clone, Serialize)]
pub struct InitializeRequest(pub EnclaveConfig);

impl EnclaveRequestType for InitializeRequest {
    type Response = (); // InitializeSuccess has no data

    fn into_request(self) -> EnclaveRequest {
        EnclaveRequest::Initialize(self.0)
    }

    fn from_response(response: EnclaveResponse) -> Result<Self::Response, EnclaveError> {
        match response {
            EnclaveResponse::InitializeSuccess => Ok(()),
            EnclaveResponse::Error(e) => Err(e),
            _ => Err(EnclaveError::UnexpectedResponse),
        }
    }
}
