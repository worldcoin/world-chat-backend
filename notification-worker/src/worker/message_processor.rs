use tokio_util::sync::CancellationToken;
use tracing::info;

use super::Message;

/// `MessageProcessor` handles individual message processing
pub struct MessageProcessor {
    worker_id: usize,
}

impl MessageProcessor {
    /// Creates a new `MessageProcessor`
    #[must_use]
    pub const fn new(worker_id: usize) -> Self {
        Self { worker_id }
    }

    /// Runs the message processor loop
    pub async fn run(&self, receiver: flume::Receiver<Message>, shutdown_token: CancellationToken) {
        info!("Message processor {} started", self.worker_id);

        loop {
            tokio::select! {
                () = shutdown_token.cancelled() => {
                    info!("Message processor {} received shutdown signal", self.worker_id);
                    break;
                }
                result = receiver.recv_async() => {
                    match result {
                        Ok(message) => self.process_message(&message),
                        Err(flume::RecvError::Disconnected) => {
                            info!("Message channel closed for processor {}", self.worker_id);
                            break;
                        }
                    }
                }
            }
        }

        info!("Message processor {} stopped", self.worker_id);
    }

    /// Processes a single message
    fn process_message(&self, message: &Message) {
        // Log the message
        info!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            message.content_topic,
            message.timestamp_ns,
            message.message.len()
        );
    }
}
