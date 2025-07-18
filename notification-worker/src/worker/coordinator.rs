use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

use super::config::WorkerConfig;
use super::processor::MessageProcessor;
use super::stream_listener::StreamListener;
use super::types::{Message, SharedState, WorkerResult};
use tonic::transport::Channel;

/// Coordinator manages the lifecycle of all worker components
pub struct Coordinator {
    config: WorkerConfig,
    shared_state: Arc<SharedState>,
    shutdown_token: CancellationToken,
}

impl Coordinator {
    /// Creates a new Coordinator
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            shared_state: Arc::new(SharedState::new()),
            shutdown_token: CancellationToken::new(),
        }
    }

    /// Returns a clone of the shutdown token for external control
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Returns a reference to the shared state
    pub fn shared_state(&self) -> Arc<SharedState> {
        self.shared_state.clone()
    }

    /// Starts the coordinator and all worker components
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
        let processor_handles = self.spawn_processors(message_rx);

        // Create and start stream listener
        let listener = StreamListener::new(
            client,
            message_tx,
            self.config.clone(),
            self.shutdown_token.clone(),
        );

        // Run the stream listener
        let listener_result = listener.run().await;

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
    fn spawn_processors(&self, receiver: flume::Receiver<Message>) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for i in 0..self.config.num_workers {
            let processor = MessageProcessor::new(i, self.shared_state.clone());
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
