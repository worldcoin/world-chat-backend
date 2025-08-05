use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{error, info};

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

use super::message_processor::MessageProcessor;
use super::xmtp_listener::XmtpListener;
use super::{Message, WorkerConfig, WorkerResult};

/// `Coordinator` manages the lifecycle of all worker components
pub struct Coordinator {
    config: WorkerConfig,
    shutdown_token: CancellationToken,
}

impl Coordinator {
    /// Creates a new `Coordinator`
    #[must_use]
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            shutdown_token: CancellationToken::new(),
        }
    }

    /// Returns a clone of the shutdown token for external control
    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Starts the coordinator and all worker components
    ///
    /// # Errors
    ///
    /// Returns an error if stream listening fails or processor tasks panic.
    pub async fn start(self, client: MessageApiClient<Channel>) -> WorkerResult<()> {
        info!(
            "Starting coordinator with {} workers",
            self.config.num_workers
        );

        // Create the message channel
        let (message_tx, message_rx) = flume::bounded::<Message>(self.config.channel_capacity());
        info!(
            "Created flume channel with capacity: {}",
            self.config.channel_capacity()
        );

        // Spawn message processors
        let processor_handles = self.spawn_processors(&message_rx);

        // Create and start XMTP listener
        let listener_result = XmtpListener::new(
            client,
            message_tx,
            self.config.clone(),
            self.shutdown_token.clone(),
        )
        .run()
        .await;

        // Stream listener has stopped (either shutdown or error)
        if let Err(e) = listener_result {
            error!("Stream listener error: {}", e);
        }

        // Wait for shutdown signal
        self.shutdown_token.cancel();
        info!("Coordinator shutdown initiated");

        // Wait for all processors to complete
        for handle in processor_handles {
            if let Err(e) = handle.await {
                error!("Processor task error: {}", e);
            }
        }

        info!("All workers stopped");
        Ok(())
    }

    /// Spawns message processor tasks
    fn spawn_processors(&self, receiver: &flume::Receiver<Message>) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for i in 0..self.config.num_workers {
            let processor = MessageProcessor::new(i);
            let rx = receiver.clone();
            let shutdown_token = self.shutdown_token.clone();

            let handle = tokio::spawn(async move {
                processor.run(rx, shutdown_token).await;
            });

            handles.push(handle);
        }

        handles
    }
}
