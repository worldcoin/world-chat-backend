pub mod coordinator;
pub mod message_processor;
pub mod xmtp_listener;

use crate::types::environment::Environment;
use crate::xmtp::message_api::v1::Envelope;
pub use coordinator::Coordinator;

// Type aliases
/// Message type that flows through the worker pipeline
pub type Message = Envelope;

/// Result type for worker operations
pub type WorkerResult<T> = anyhow::Result<T>;

// Configuration
/// Configuration for the XMTP worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// XMTP node endpoint
    pub xmtp_endpoint: String,
    /// Whether to use TLS for the connection
    pub use_tls: bool,
    /// Client version to send in metadata
    pub client_version: String,
    /// Number of worker tasks to spawn
    pub num_workers: usize,
    /// Initial reconnection delay in milliseconds
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection delay in milliseconds
    pub max_reconnect_delay_ms: u64,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Connect timeout in milliseconds
    pub connect_timeout_ms: u64,
}

impl WorkerConfig {
    /// Creates a new `WorkerConfig` from the given environment
    #[must_use]
    pub fn from_environment(env: &Environment) -> Self {
        Self {
            xmtp_endpoint: env.xmtp_grpc_address(),
            use_tls: env.use_tls_override(),
            client_version: "notification-worker-rust/0.1.0".to_string(),
            num_workers: env.default_num_workers(),
            reconnect_delay_ms: env.reconnect_delay_ms(),
            max_reconnect_delay_ms: env.max_reconnect_delay_ms(),
            connection_timeout_ms: env.connection_timeout_ms(),
            connect_timeout_ms: env.connect_timeout_ms(),
        }
    }

    /// Returns the channel capacity (2 * `num_workers`)
    #[must_use]
    pub const fn channel_capacity(&self) -> usize {
        self.num_workers * 2
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let env = Environment::from_env();
        Self::from_environment(&env)
    }
}

// Legacy adapter for backward compatibility
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, ClientTlsConfig};
use tracing::info;

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

/// Legacy `XmtpWorker` adapter for backward compatibility
/// This wraps the new Coordinator to maintain the old API
pub struct XmtpWorker {
    coordinator: Coordinator,
    client: MessageApiClient<Channel>,
}

impl XmtpWorker {
    /// Creates a new XMTP worker (legacy API)
    ///
    /// # Errors
    ///
    /// Returns an error if connection to XMTP fails or TLS configuration is invalid.
    pub async fn new(config: WorkerConfig) -> anyhow::Result<Self> {
        info!("Connecting to XMTP node at {}", config.xmtp_endpoint);
        info!(
            "TLS enabled: {}, Client version: {}",
            config.use_tls, config.client_version
        );

        // Create the endpoint with proper configuration
        let mut endpoint = Channel::from_shared(config.xmtp_endpoint.clone())?;

        // Configure TLS if needed
        if config.use_tls {
            // Create TLS config with webpki roots
            let tls_config = ClientTlsConfig::new().with_webpki_roots();
            endpoint = endpoint.tls_config(tls_config)?;
        }

        // Add timeouts
        endpoint = endpoint
            .timeout(std::time::Duration::from_millis(
                config.connection_timeout_ms,
            ))
            .connect_timeout(std::time::Duration::from_millis(config.connect_timeout_ms));

        let channel = endpoint.connect().await?;

        // For now, create client without interceptor (will add metadata support later)
        let client = MessageApiClient::new(channel);

        let coordinator = Coordinator::new(config);

        Ok(Self {
            coordinator,
            client,
        })
    }

    /// Returns a clone of the shutdown token for external control
    pub fn shutdown_token(&self) -> CancellationToken {
        self.coordinator.shutdown_token()
    }

    /// Starts the worker (legacy API)
    ///
    /// # Errors
    ///
    /// Returns an error if the worker fails to start or encounters runtime errors.
    pub async fn start(self) -> anyhow::Result<()> {
        self.coordinator.start(self.client).await
    }
}
