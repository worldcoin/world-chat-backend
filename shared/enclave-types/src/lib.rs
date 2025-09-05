use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main request type sent from the enclave-worker to the secure-enclave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnclaveRequest {
    /// Request to initialize the enclave with Braze configuration
    Initialize(BrazeConfig),
    /// Request to send a notification through Braze
    Notification(Box<NotificationRequest>),
    /// Health check request
    HealthCheck,
}

/// Response type sent from the secure-enclave back to the enclave-worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnclaveResponse {
    /// Initialization successful
    InitializeSuccess,
    /// Notification sent successfully with Braze response details
    NotificationSuccess(NotificationResponse),
    /// Health check response
    HealthCheckOk { initialized: bool },
    /// Error response
    Error(EnclaveError),
}

/// Braze API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrazeConfig {
    /// Braze API key (will be stored securely in the enclave)
    pub api_key: String,
    /// Braze REST endpoint URL (e.g., https://rest.iad-01.braze.com)
    pub api_endpoint: String,
    /// Optional proxy configuration for network access
    pub proxy_config: Option<ProxyConfig>,
}

/// Proxy configuration for network access from the enclave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy host (usually 127.0.0.1 for vsock-proxy)
    pub host: String,
    /// Proxy port
    pub port: u16,
}

/// Notification request to be sent to Braze
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRequest {
    /// Unique request ID for tracking
    pub request_id: String,
    /// User external ID in Braze
    pub external_user_id: String,
    /// Notification title
    pub title: String,
    /// Notification message body
    pub message: String,
    /// Optional custom data to include in the notification
    pub custom_data: Option<HashMap<String, String>>,
    /// Optional trigger properties for Braze campaign/canvas
    pub trigger_properties: Option<HashMap<String, serde_json::Value>>,
}

/// Response from successful notification send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResponse {
    /// Request ID that was sent
    pub request_id: String,
    /// Braze dispatch ID for tracking
    pub dispatch_id: Option<String>,
    /// Number of messages queued
    pub messages_queued: u32,
    /// Any errors from Braze API
    pub errors: Vec<String>,
}

/// Enclave errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum EnclaveError {
    #[error("Enclave not initialized. Call Initialize first.")]
    NotInitialized,
    
    #[error("Failed to send notification: {0}")]
    NotificationFailed(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Braze API error: {0}")]
    BrazeApiError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Proxy error: {0}")]
    ProxyError(String),
}
