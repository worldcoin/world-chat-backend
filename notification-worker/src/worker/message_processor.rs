use std::sync::Arc;

use crate::xmtp::message_api::v1::Envelope;
use backend_storage::queue::{Notification, NotificationQueue};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info};

use crate::utils::is_v3_topic;

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
    pub async fn run(
        &self,
        receiver: flume::Receiver<Envelope>,
        shutdown_token: CancellationToken,
    ) {
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
    async fn process_message(&self, message: &Envelope) {
        // Step 1: Filter out messages that are not V3, following example from XMTP
        if !is_v3_topic(&message.content_topic) {
            return;
        }

        debug!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            message.content_topic,
            message.timestamp_ns,
            message.message.len()
        );

        //step 2: extract message context
        //step 2.5: filter by shouldPush
        //step 3: fetch subscriptions
        //step 4: filter out self-notifications
        //step 5: deliver message

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
    fn convert_to_notification(message: &Envelope) -> Notification {
        // TODO: Finalise type conversion: Include sender hmac and payload
        Notification {
            topic: message.content_topic.clone(),
            sender_hmac: "placeholder_sender_hmac".to_string(),
            payload: format!(
                "{{\"timestamp_ns\":{},\"message_size\":{}}}",
                message.timestamp_ns,
                message.message.len(),
            ),
        }
    }
}
