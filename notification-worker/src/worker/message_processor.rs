use std::{collections::HashSet, sync::Arc};

use crate::{xmtp::message_api::v1::Envelope, xmtp_utils::MessageContext};
use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use metrics::counter;
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info, instrument, Span};
use uuid::Uuid;

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
                                error!("Failed to process message: {:#?}", e);
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
    /// This method performs lightweight filtering before creating a trace span.
    /// Only messages that will actually be queued for delivery create traces,
    /// reducing noise in observability data.
    ///
    /// # Errors
    ///
    /// Returns an error if the message cannot be processed.
    pub async fn process_message(&self, envelope: &Envelope) -> anyhow::Result<()> {
        // Pre-filter: Skip non-V3 topics without creating a trace
        if !is_v3_topic(&envelope.content_topic) {
            return Ok(());
        }

        // Pre-filter: Parse message context and check should_push flag
        let message_context = match MessageContext::from_xmtp_envelope(envelope) {
            Ok(ctx) => ctx,
            Err(e) => {
                // Log parsing errors but don't create a trace for malformed messages
                debug!("Failed to parse message context: {}", e);
                return Ok(());
            }
        };

        if Some(false) == message_context.should_push {
            return Ok(());
        }

        // Pre-filter: Check if there are any recipients
        let subscriptions = self
            .subscription_storage
            .get_all_by_topic(&envelope.content_topic)
            .await?;

        let subscribed_encrypted_push_ids = subscriptions
            .into_iter()
            .filter_map(|s| match message_context.is_sender(&s.hmac_key) {
                Ok(true) => None, // Filter out self-notifications
                Ok(false) => Some(s.encrypted_push_id),
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
            return Ok(());
        }

        // All filters passed - now create the traced span for actual notification processing
        self.queue_notification(envelope, subscribed_encrypted_push_ids)
            .await
    }

    /// Queues a notification for delivery. This is the instrumented portion
    /// that creates traces only for messages that will actually be delivered.
    #[instrument(
        skip(self, envelope, recipients),
        fields(
            worker_id = self.worker_id,
            content_topic = %envelope.content_topic,
            recipient_count = recipients.len(),
            message_id = tracing::field::Empty,
            request_id = %Uuid::new_v4()
        )
    )]
    async fn queue_notification(
        &self,
        envelope: &Envelope,
        recipients: HashSet<String>,
    ) -> anyhow::Result<()> {
        // Convert nanoseconds to milliseconds for E2E latency tracking
        // Wrap is safe: nanoseconds since epoch in u64 fits in i64 for ~292 years
        #[allow(clippy::cast_possible_wrap)]
        let created_at_ms = if envelope.timestamp_ns > 0 {
            Some((envelope.timestamp_ns / 1_000_000) as i64)
        } else {
            None
        };

        debug!(
            "Queueing notification - Timestamp: {}, Size: {} bytes, Recipients: {}",
            envelope.timestamp_ns,
            envelope.message.len(),
            recipients.len()
        );

        // Convert XMTP envelope to notification
        let notification = Notification {
            topic: envelope.content_topic.clone(),
            subscribed_encrypted_push_ids: recipients.into_iter().collect(),
            encrypted_message_base64: STANDARD.encode(envelope.message.as_slice()),
            created_at_ms,
        };

        // Publish to notification queue
        let message_id = self
            .notification_queue
            .send_message(&notification)
            .await
            .context("Failed to send message to notification queue")?;

        Span::current().record("message_id", message_id);
        counter!("notification_queued").increment(1);

        Ok(())
    }
}
