use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use enclave_types::EnclaveNotificationRequest;
use metrics::counter;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

pub struct NotificationProcessor {
    queue: Arc<NotificationQueue>,
    #[allow(dead_code)] // Will be used for nitro enclave integration to delete subscriptions
    storage: Arc<PushSubscriptionStorage>,
    pontifex_connection_details: pontifex::client::ConnectionDetails,
    shutdown: CancellationToken,
}

impl NotificationProcessor {
    /// Creates a new `NotificationProcessor`
    ///
    /// # Panics
    ///
    /// If the HTTP client fails to create, this will panic.
    #[must_use]
    pub const fn new(
        queue: Arc<NotificationQueue>,
        storage: Arc<PushSubscriptionStorage>,
        shutdown: CancellationToken,
        pontifex_connection_details: pontifex::client::ConnectionDetails,
    ) -> Self {
        Self {
            queue,
            storage,
            pontifex_connection_details,
            shutdown,
        }
    }

    pub async fn start(self) {
        info!("Starting NotificationProcessor");

        // Poll queue until shutdown
        while !self.shutdown.is_cancelled() {
            tokio::select! {
                result = self.poll_once() => match result {
                    Ok(()) => {}
                    Err(e) => {
                        error!(error = ?e, "Failed to poll messages");
                    }
                },
                () = self.shutdown.cancelled() => {
                    info!("Queue poller shutting down");
                    break;
                }
            }
        }

        info!("NotificationProcessor shutdown complete");
    }

    async fn poll_once(&self) -> anyhow::Result<()> {
        let messages = self
            .queue
            .poll_messages()
            .await
            .context("Failed to poll messages")?;

        // TODO: Make these requests in parallel to improve performance
        for message in messages {
            tracing::info!("Processing message: {}", message.message_id);
            self.process_and_ack(message).await?;
        }

        Ok(())
    }

    #[instrument(skip(self, message), fields(message_id = %message.message_id))]
    async fn process_and_ack(&self, message: QueueMessage<Notification>) -> anyhow::Result<()> {
        let notification = message.body;
        let receipt_handle = message.receipt_handle;

        let notification_request = EnclaveNotificationRequest {
            encrypted_push_ids: notification.subscribed_encrypted_push_ids,
            apple_push: serde_json::json!({
                // Use fallback copy, until we get Apple entitlement
                "alert": {
                    "title": "New Activity",
                    "body": "This content is temporarily unavailable."
                },
                "badge": 1,
                "sound": "default",
                "mutable_content": true,
                "extra": {
                    "topic": notification.topic,
                    "encryptedMessageBase64": notification.encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            }),
            android_push: serde_json::json!({
                // Title and alert are required fields, Android client decryptes message and shows correct content
                "title": "world_chat_notification",
                "alert": "world_chat_notification",
                "priority": "high",
                "extra": {
                    "topic": notification.topic,
                    "encryptedMessageBase64": notification.encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            }),
        };

        pontifex::client::send::<EnclaveNotificationRequest>(
            self.pontifex_connection_details,
            &notification_request,
        )
        .await??;

        // Acknowledge the message after successful processing
        self.queue.ack_message(&receipt_handle).await?;

        counter!("notification_delivered").increment(1);

        Ok(())
    }
}
