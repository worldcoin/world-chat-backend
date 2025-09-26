use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use enclave_types::EnclaveNotificationRequest;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

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
    pub fn new(
        queue: Arc<NotificationQueue>,
        storage: Arc<PushSubscriptionStorage>,
        shutdown: CancellationToken,
        pontifex_connection_details: pontifex::client::ConnectionDetails,
    ) -> Self {
        Self {
            queue,
            storage,
            shutdown,
            pontifex_connection_details,
        }
    }

    pub async fn start(self) {
        info!("Starting NotificationProcessor");

        // Poll queue until shutdown
        while !self.shutdown.is_cancelled() {
            tokio::select! {
                result = self.poll_once() => match result {
                    Ok(_) => {}
                    Err(e) => {
                        error!(error = ?e, "Failed to poll messages");
                    }
                },
                _ = self.shutdown.cancelled() => {
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

    async fn process_and_ack(&self, message: QueueMessage<Notification>) -> anyhow::Result<()> {
        let notification = message.body;
        let receipt_handle = message.receipt_handle;

        pontifex::client::send::<EnclaveNotificationRequest>(
            self.pontifex_connection_details,
            &EnclaveNotificationRequest {
                topic: notification.topic,
                subscribed_encrypted_push_ids: notification.subscribed_encrypted_push_ids,
                encrypted_message_base64: notification.encrypted_message_base64,
            },
        )
        .await??;

        // Acknowledge the message after successful processing
        self.queue.ack_message(&receipt_handle).await?;

        Ok(())
    }
}
