use std::sync::Arc;

use backend_storage::queue::{Notification, NotificationQueue};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::Message;

/// `MessageProcessor` handles individual message processing
pub struct MessageProcessor {
    worker_id: usize,
    notification_queue: Arc<NotificationQueue>,
}

impl MessageProcessor {
    /// Creates a new `MessageProcessor`
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(worker_id: usize, notification_queue: Arc<NotificationQueue>) -> Self {
        Self {
            worker_id,
            notification_queue,
        }
    }

    /// Runs the message processor loop
    #[allow(clippy::cognitive_complexity)]
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
                        Ok(message) => self.process_message(&message).await,
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
    async fn process_message(&self, message: &Message) {
        // Log the message
        info!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            message.content_topic,
            message.timestamp_ns,
            message.message.len()
        );

        // Convert XMTP message to notification
        let notification = Self::convert_to_notification(message);

        // Publish to notification queue
        match self.notification_queue.send_message(&notification).await {
            Ok(_) => {
                info!(
                    "Worker {} successfully published notification for topic: {}",
                    self.worker_id, notification.topic
                );
            }
            Err(e) => {
                error!(
                    "Worker {} failed to publish notification for topic {}: {}",
                    self.worker_id, notification.topic, e
                );
            }
        }
    }

    /// Converts an XMTP message to a notification
    fn convert_to_notification(message: &Message) -> Notification {
        Notification {
            topic: message.content_topic.clone(),
            sender_hmac: "placeholder_sender_hmac".to_string(), // TODO: Extract from message
            payload: format!(
                "{{\"timestamp_ns\":{},\"message_size\":{}}}",
                message.timestamp_ns,
                message.message.len(),
            ),
        }
    }
}
