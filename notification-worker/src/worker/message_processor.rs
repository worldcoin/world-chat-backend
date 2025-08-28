use std::sync::Arc;

use crate::{utils::MessageContext, xmtp::message_api::v1::Envelope};
use anyhow::Context;
use backend_storage::{
    push_notification::PushNotificationStorage,
    queue::{Notification, NotificationQueue},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info};

use crate::utils::is_v3_topic;

/// `MessageProcessor` handles individual message processing
pub struct MessageProcessor {
    worker_id: usize,
    notification_queue: Arc<NotificationQueue>,
    subscription_storage: Arc<PushNotificationStorage>,
}

impl MessageProcessor {
    /// Creates a new `MessageProcessor`
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        worker_id: usize,
        notification_queue: Arc<NotificationQueue>,
        subscription_storage: Arc<PushNotificationStorage>,
    ) -> Self {
        Self {
            worker_id,
            notification_queue,
            subscription_storage,
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
                        Ok(message) => {
                            if let Err(e) = self.process_message(&message).await {
                                error!("Worker {} failed to process message: {}", self.worker_id, e);
                            }
                        }
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
    async fn process_message(&self, envelope: &Envelope) -> anyhow::Result<()> {
        // Step 1: Filter out messages that are not V3, following example from XMTP
        if !is_v3_topic(&envelope.content_topic) {
            return Ok(());
        }

        debug!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            envelope.content_topic,
            envelope.timestamp_ns,
            envelope.message.len()
        );

        let message_context = MessageContext::from_xmtp_envelope(envelope)?;

        // Step 2: Filter out messages that should not be pushed
        if !message_context.should_push.unwrap_or(false) {
            return Ok(());
        }

        let subscriptions = self
            .subscription_storage
            .get_all_by_topic(&envelope.content_topic)
            .await?;

        // Step 3: Filter out self-notifications, a user should not receive a notification for their own message
        let subscribed_encrypted_push_ids = subscriptions
            .into_iter()
            .filter_map(|s| match message_context.is_sender(s.hmac.as_bytes()) {
                Ok(true) => Some(s.encrypted_push_id),
                Ok(false) => None,
                // Don't block notification for valid HMACs but log error
                Err(e) => {
                    error!(
                        "Worker {} failed to check sender for subscription {}: {}. Message context: {:?}",
                        self.worker_id,
                        s.hmac,
                        e,
                        message_context
                    );
                    None
                }
            })
            .collect::<Vec<_>>();

        // Convert XMTP envelope to notification
        let notification = Notification {
            topic: envelope.content_topic.clone(),
            subscribed_encrypted_push_ids,
            encrypted_message_base64: STANDARD.encode(envelope.message.as_slice()),
        };

        // Step 4: Publish to notification queue
        self.notification_queue
            .send_message(&notification)
            .await
            .context("Failed to send message to notificationx queue")?;

        Ok(())
    }
}
