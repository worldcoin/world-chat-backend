use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use enclave_types::EnclaveNotificationRequest;
use futures::future::join_all;
use metrics::counter;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

/// Maximum number of recipients per batch when sending to pontifex
const BATCH_SIZE: usize = 50;

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

        // If there are no recipients, acknowledge and return
        if notification.subscribed_encrypted_push_ids.is_empty() {
            warn!("No recipients found for notification, acknowledging message");
            self.queue.ack_message(&receipt_handle).await?;
            counter!("notification_delivered").increment(1);
            return Ok(());
        }

        // Split recipients into batches
        let batches = notification
            .subscribed_encrypted_push_ids
            .chunks(BATCH_SIZE);

        // Create futures for each batch
        let batch_futures = batches
            .into_iter()
            .enumerate()
            .map(|(batch_idx, batch_recipients)| {
                let topic = notification.topic.clone();
                let message = notification.encrypted_message_base64.clone();
                let connection_details = self.pontifex_connection_details;

                async move {
                    let result = pontifex::client::send::<EnclaveNotificationRequest>(
                        connection_details,
                        &EnclaveNotificationRequest {
                            topic,
                            subscribed_encrypted_push_ids: batch_recipients.to_vec(),
                            encrypted_message_base64: message,
                        },
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Transport error: {}", e))
                    .and_then(|inner| inner.map_err(|e| anyhow::anyhow!("Enclave error: {:?}", e)));

                    (batch_idx, batch_recipients.len(), result)
                }
            });

        // Execute all batches in parallel
        let results = join_all(batch_futures).await;

        // Process results and count failures
        let total_batches = results.len();
        let mut failed_batches = 0;

        for (batch_idx, recipient_count, result) in results {
            match result {
                Ok(()) => {
                    info!(
                        batch_idx,
                        recipient_count, "Successfully delivered notification batch"
                    );
                }
                Err(e) => {
                    failed_batches += 1;
                    warn!(
                        batch_idx,
                        recipient_count,
                        error = ?e,
                        "Transport error while delivering notification batch"
                    );
                }
            }
        }

        // If all batches failed, propagate the error
        if failed_batches == total_batches {
            return Err(anyhow::anyhow!(
                "All notification batches failed to deliver"
            ));
        }

        // Log if we had partial failures
        if failed_batches > 0 {
            error!("{failed_batches} of {total_batches} notification batches failed");
        }

        // Acknowledge the message since we had at least partial success
        self.queue.ack_message(&receipt_handle).await?;

        // Increment the counter for delivered notifications (even for partial success)
        counter!("notification_delivered").increment(1);

        Ok(())
    }
}
