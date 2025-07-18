use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::types::{Message, SharedState};

/// MessageProcessor handles individual message processing
pub struct MessageProcessor {
    worker_id: usize,
    #[allow(dead_code)] // will be used in the future
    shared_state: Arc<SharedState>,
}

impl MessageProcessor {
    /// Creates a new MessageProcessor
    pub fn new(worker_id: usize, shared_state: Arc<SharedState>) -> Self {
        Self {
            worker_id,
            shared_state,
        }
    }

    /// Runs the message processor loop
    pub async fn run(&self, receiver: flume::Receiver<Message>, shutdown_token: CancellationToken) {
        info!("Message processor {} started", self.worker_id);

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Message processor {} received shutdown signal", self.worker_id);
                    break;
                }
                result = receiver.recv_async() => {
                    match result {
                        Ok(message) => self.process_message(message).await,
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
    async fn process_message(&self, message: Message) {
        // Log the message
        info!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            message.content_topic,
            message.timestamp_ns,
            message.message.len()
        );

        // Future processing steps:
        // 1. Check topic cache in shared_state to see if we should process this topic
        // 2. Decode the message
        // 3. Determine if notification is needed
        // 4. Queue notification for delivery

        // For now, we just log it
        self.handle_v3_message(&message).await;
    }

    /// Handles V3 messages (groups and welcome messages)
    async fn handle_v3_message(&self, message: &Message) {
        // Check if it's a V3 topic
        if message.content_topic.starts_with("/xmtp/mls/1/g-") {
            info!(
                "Worker {} processing group message on topic: {}",
                self.worker_id, message.content_topic
            );
            // Future: Process group message
        } else if message.content_topic.starts_with("/xmtp/mls/1/w-") {
            info!(
                "Worker {} processing welcome message on topic: {}",
                self.worker_id, message.content_topic
            );
            // Future: Process welcome message
        } else {
            // Not a V3 topic we care about
            info!(
                "Worker {} ignoring non-V3 message on topic: {}",
                self.worker_id, message.content_topic
            );
        }
    }

    /// Returns the worker ID for testing
    #[cfg(test)]
    pub fn worker_id(&self) -> usize {
        self.worker_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmtp::message_api::v1::Envelope;

    #[tokio::test]
    async fn test_processor_creation() {
        let shared_state = Arc::new(SharedState::new());
        let processor = MessageProcessor::new(1, shared_state);
        assert_eq!(processor.worker_id(), 1);
    }

    #[tokio::test]
    async fn test_v3_topic_detection() {
        let shared_state = Arc::new(SharedState::new());
        let processor = MessageProcessor::new(0, shared_state);

        // Test group message
        let group_msg = Envelope {
            content_topic: "/xmtp/mls/1/g-123".to_string(),
            timestamp_ns: 123456789,
            message: vec![1, 2, 3],
        };
        processor.process_message(group_msg).await;

        // Test welcome message
        let welcome_msg = Envelope {
            content_topic: "/xmtp/mls/1/w-456".to_string(),
            timestamp_ns: 123456789,
            message: vec![4, 5, 6],
        };
        processor.process_message(welcome_msg).await;

        // Test non-V3 message
        let other_msg = Envelope {
            content_topic: "/xmtp/0/dm-789".to_string(),
            timestamp_ns: 123456789,
            message: vec![7, 8, 9],
        };
        processor.process_message(other_msg).await;
    }
}
