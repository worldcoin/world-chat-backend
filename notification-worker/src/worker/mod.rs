pub mod config;
pub mod coordinator;
pub mod processor;
pub mod stream_listener;
pub mod types;

pub use config::WorkerConfig;
pub use coordinator::Coordinator;

// Legacy adapter for backward compatibility
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::info;

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

/// Legacy XmtpWorker adapter for backward compatibility
/// This wraps the new Coordinator to maintain the old API
pub struct XmtpWorker {
    coordinator: Coordinator,
    client: MessageApiClient<Channel>,
}

impl XmtpWorker {
    /// Creates a new XMTP worker (legacy API)
    pub async fn new(config: WorkerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Connecting to XMTP node at {}", config.xmtp_endpoint);

        // Create the channel - tonic will handle TLS automatically
        let channel = Channel::from_shared(config.xmtp_endpoint.clone())?
            .connect()
            .await?;

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
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        self.coordinator.start(self.client).await
    }
}
