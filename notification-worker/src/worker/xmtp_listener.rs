use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{error, info, warn};

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;
use crate::xmtp::message_api::v1::SubscribeAllRequest;

use super::{Message, WorkerConfig, WorkerResult};

/// `XmtpListener` handles the connection to XMTP and message streaming
pub struct XmtpListener {
    client: MessageApiClient<Channel>,
    message_tx: flume::Sender<Message>,
    config: WorkerConfig,
    shutdown_token: CancellationToken,
}

impl XmtpListener {
    /// Creates a new `XmtpListener`
    pub const fn new(
        client: MessageApiClient<Channel>,
        message_tx: flume::Sender<Message>,
        config: WorkerConfig,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            client,
            message_tx,
            config,
            shutdown_token,
        }
    }

    /// Runs the stream listener with automatic reconnection
    ///
    /// # Errors
    ///
    /// Returns an error if the stream connection fails or message processing encounters errors.
    pub async fn run(mut self) -> WorkerResult<()> {
        let mut reconnect_delay = self.config.reconnect_delay_ms;

        loop {
            // Check for shutdown first
            if self.shutdown_token.is_cancelled() {
                info!("Stream listener shutting down");
                return Ok(());
            }

            // Try to subscribe and process
            match self.subscribe_and_process().await {
                Ok(()) => {
                    warn!("Stream ended unexpectedly, reconnecting...");
                    reconnect_delay = self.config.reconnect_delay_ms;
                }
                Err(e) => {
                    error!("Stream error: {}, reconnecting in {}ms", e, reconnect_delay);

                    // Wait with cancellation support
                    tokio::select! {
                        () = self.shutdown_token.cancelled() => {
                            info!("Stream listener shutting down during reconnect delay");
                            return Ok(());
                        }
                        () = sleep(Duration::from_millis(reconnect_delay)) => {}
                    }

                    // Exponential backoff
                    reconnect_delay = (reconnect_delay * 2).min(self.config.max_reconnect_delay_ms);
                }
            }
        }
    }

    /// Subscribes to the message stream and processes messages
    async fn subscribe_and_process(&mut self) -> WorkerResult<()> {
        let request = SubscribeAllRequest {};
        let response = self.client.subscribe_all(request).await?;
        let mut stream = response.into_inner();

        while let Some(envelope) = stream.message().await? {
            if let Err(e) = self.message_tx.send_async(envelope).await {
                error!("Failed to send message to workers: {}", e);
                return Err(anyhow::anyhow!("Message channel closed"));
            }
        }

        Ok(())
    }
}
