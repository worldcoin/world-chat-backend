use std::{collections::HashSet, sync::Arc};

use crate::{xmtp::message_api::v1::Envelope, xmtp_utils::MessageContext};
use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info, instrument, warn};

use crate::xmtp_utils::is_v3_topic;

/// `MessageProcessor` handles individual message processing
pub struct MessageProcessor {
    worker_id: usize,
    notification_queue: Arc<NotificationQueue>,
    subscription_storage: Arc<PushSubscriptionStorage>,
}

impl MessageProcessor {
    /// Creates a new `MessageProcessor`
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        worker_id: usize,
        notification_queue: Arc<NotificationQueue>,
        subscription_storage: Arc<PushSubscriptionStorage>,
    ) -> Self {
        Self {
            worker_id,
            notification_queue,
            subscription_storage,
        }
    }

    /// Runs the message processor loop
    #[allow(clippy::cognitive_complexity)]
    #[instrument(skip(self, receiver, shutdown_token), fields(worker_id = self.worker_id))]
    pub async fn run(
        &self,
        receiver: flume::Receiver<Envelope>,
        shutdown_token: CancellationToken,
    ) {
        info!("Message processor started");

        loop {
            tokio::select! {
                () = shutdown_token.cancelled() => {
                    info!("Message processor received shutdown signal");
                    break;
                }
                result = receiver.recv_async() => {
                    match result {
                        Ok(message) => {
                            if let Err(e) = self.process_message(&message).await {
                                error!("Failed to process message: {}", e);
                            }
                        }
                        Err(flume::RecvError::Disconnected) => {
                            info!("Message channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("Message processor stopped");
    }

    /// Processes a single message
    ///
    /// # Errors
    ///
    /// Returns an error if the message cannot be processed.
    #[instrument(skip(self, envelope), fields(worker_id = self.worker_id, content_topic = %envelope.content_topic))]
    pub async fn process_message(&self, envelope: &Envelope) -> anyhow::Result<()> {
        // Step 1: Filter out messages that are not V3, following example from XMTP
        if !is_v3_topic(&envelope.content_topic) {
            return Ok(());
        }

        debug!(
            "Processing message - Timestamp: {}, Size: {} bytes",
            envelope.timestamp_ns,
            envelope.message.len()
        );

        let message_context = MessageContext::from_xmtp_envelope(envelope)?;

        // Step 2: Filter out messages that should not be pushed
        if Some(false) == message_context.should_push {
            return Ok(());
        }

        // Step 3: Filter out self-notifications, a user should not receive a notification for their own message
        let subscriptions = self
            .subscription_storage
            .get_all_by_topic(&envelope.content_topic)
            .await?;
        let subscribed_encrypted_push_ids = subscriptions
            .into_iter()
            .filter_map(|s| match message_context.is_sender(&s.hmac_key) {
                Ok(true) => None, // Filter out self-notifications (sender matches subscription)
                Ok(false) => Some(s.encrypted_push_id),
                // Don't block notification for valid HMACs but log error
                Err(e) => {
                    error!(
                        "Failed to check sender for subscription {}: {}. Message context: {:?}",
                        s.hmac_key, e, message_context
                    );
                    Some(s.encrypted_push_id) // Include on error to be safe
                }
            })
            .collect::<HashSet<_>>();
        if subscribed_encrypted_push_ids.is_empty() {
            warn!("No subscriptions found for topic");
            return Ok(());
        }

        // Convert XMTP envelope to notification
        let notification = Notification {
            topic: envelope.content_topic.clone(),
            subscribed_encrypted_push_ids: subscribed_encrypted_push_ids.into_iter().collect(),
            encrypted_message_base64: STANDARD.encode(envelope.message.as_slice()),
        };

        // Step 4: Publish to notification queue
        self.notification_queue
            .send_message(&notification)
            .await
            .context("Failed to send message to notification queue")?;

        Ok(())
    }
}
