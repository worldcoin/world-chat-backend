use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{error, info, instrument, warn};

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;
use crate::xmtp::message_api::v1::Envelope;
use crate::xmtp::message_api::v1::SubscribeAllRequest;

use super::WorkerResult;

pub struct XmtpListenerConfig {
    pub reconnect_delay_ms: u64,
    pub max_reconnect_delay_ms: u64,
}

/// `XmtpListener` handles the connection to XMTP and message streaming
pub struct XmtpListener {
    client: MessageApiClient<Channel>,
    message_tx: flume::Sender<Envelope>,
    shutdown_token: CancellationToken,
    config: XmtpListenerConfig,
}

impl XmtpListener {
    /// Creates a new `XmtpListener`
    pub const fn new(
        client: MessageApiClient<Channel>,
        message_tx: flume::Sender<Envelope>,
        shutdown_token: CancellationToken,
        config: XmtpListenerConfig,
    ) -> Self {
        Self {
            client,
            message_tx,
            shutdown_token,
            config,
        }
    }

    /// Runs the stream listener with automatic reconnection
    ///
    /// # Errors
    ///
    /// Returns an error if the stream connection fails or message processing encounters errors.
    #[instrument(skip(self))]
    pub async fn run(mut self) -> WorkerResult<()> {
        let mut reconnect_delay = self.config.reconnect_delay_ms;

        loop {
            if self.shutdown_token.is_cancelled() {
                info!("Stream listener shutting down");
                return Ok(());
            }

            match self.subscribe_and_process().await {
                Ok(()) => {
                    warn!("Stream ended unexpectedly, reconnecting...");
                    reconnect_delay = self.config.reconnect_delay_ms;
                }
                Err(e) => {
                    if let Some(new_delay) = self.handle_stream_error(e, reconnect_delay).await? {
                        reconnect_delay = new_delay;
                    } else {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Handles stream errors and returns new delay, or None if shutting down
    async fn handle_stream_error(
        &self,
        e: anyhow::Error,
        reconnect_delay: u64,
    ) -> WorkerResult<Option<u64>> {
        error!("Stream error: {}, reconnecting in {}ms", e, reconnect_delay);

        tokio::select! {
            () = self.shutdown_token.cancelled() => {
                info!("Stream listener shutting down during reconnect delay");
                Ok(None)
            }
            () = sleep(Duration::from_millis(reconnect_delay)) => {
                let new_delay = (reconnect_delay * 2).min(self.config.max_reconnect_delay_ms);
                Ok(Some(new_delay))
            }
        }
    }

    /// Subscribes to the message stream and processes messages
    async fn subscribe_and_process(&mut self) -> WorkerResult<()> {
        let request = SubscribeAllRequest {};
        let response = self.client.subscribe_all(request).await?;
        let mut stream = response.into_inner();

        loop {
            tokio::select! {
                () = self.shutdown_token.cancelled() => {
                    info!("Stream listener shutting down during message processing");
                    return Ok(());
                }
                result = stream.message() => {
                    if !self.handle_stream_message_result(result).await? {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Handles stream message result and returns whether to continue processing
    async fn handle_stream_message_result(
        &self,
        result: Result<Option<Envelope>, tonic::Status>,
    ) -> WorkerResult<bool> {
        match result {
            Ok(Some(envelope)) => {
                self.send_message_to_workers(envelope).await?;
                Ok(true)
            }
            Ok(None) => {
                info!("Stream ended");
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Sends message to worker processes
    async fn send_message_to_workers(&self, envelope: Envelope) -> WorkerResult<()> {
        if let Err(e) = self.message_tx.send_async(envelope).await {
            error!("Failed to send message to workers: {}", e);
            return Err(anyhow::anyhow!("Message channel closed"));
        }
        Ok(())
    }
}
