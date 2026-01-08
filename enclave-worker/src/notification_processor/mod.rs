use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use enclave_types::EnclaveNotificationRequest;
use futures::future::join_all;
use metrics::{counter, histogram};
use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Extractor for trace context from a `HashMap`
struct HashMapExtractor<'a> {
    map: &'a HashMap<String, String>,
}

impl Extractor for HashMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn keys(&self) -> Vec<&str> {
        self.map.keys().map(String::as_str).collect()
    }
}

pub struct NotificationProcessor {
    queue: Arc<NotificationQueue>,
    #[allow(dead_code)] // Will be used for nitro enclave integration to delete subscriptions
    storage: Arc<PushSubscriptionStorage>,
    pontifex_connection_details: pontifex::client::ConnectionDetails,
    shutdown: CancellationToken,
    /// Maximum number of recipients per batch when sending to pontifex
    recipients_per_batch: usize,
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
        recipients_per_batch: usize,
    ) -> Self {
        Self {
            queue,
            storage,
            pontifex_connection_details,
            shutdown,
            recipients_per_batch,
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

    #[instrument(skip(self, message), fields(
        message_id = %message.message_id,
        queue_wait_ms = tracing::field::Empty,
        e2e_latency_ms = tracing::field::Empty
    ))]
    async fn process_and_ack(&self, message: QueueMessage<Notification>) -> anyhow::Result<()> {
        // Extract and set parent context from upstream trace for distributed tracing
        if !message.trace_context.is_empty() {
            let propagator = TraceContextPropagator::new();
            let extractor = HashMapExtractor {
                map: &message.trace_context,
            };
            let parent_context = propagator.extract(&extractor);
            tracing::Span::current().set_parent(parent_context);
        }

        // Get current time once for all latency calculations
        // Truncation is safe: milliseconds since epoch fits in i64 for ~292 million years
        #[allow(clippy::cast_possible_truncation)]
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .ok();

        // Calculate and record queue wait time for latency monitoring
        if let (Some(sent_ts), Some(now)) = (message.sent_timestamp_ms, now_ms) {
            let queue_wait_ms = now.saturating_sub(sent_ts);

            // Record as span attribute for trace correlation
            tracing::Span::current().record("queue_wait_ms", queue_wait_ms);

            // Emit histogram metric for dashboarding and alerting
            // Precision loss is acceptable for latency metrics (ms resolution)
            #[allow(clippy::cast_precision_loss)]
            histogram!("notification_queue_wait_ms").record(queue_wait_ms as f64);
        }

        // Calculate and record E2E latency (from message creation to now)
        if let (Some(created_at), Some(now)) = (message.body.created_at_ms, now_ms) {
            let e2e_latency_ms = now.saturating_sub(created_at);

            // Record as span attribute for trace correlation
            tracing::Span::current().record("e2e_latency_ms", e2e_latency_ms);

            // Emit histogram metric for E2E latency monitoring
            // Precision loss is acceptable for latency metrics (ms resolution)
            #[allow(clippy::cast_precision_loss)]
            histogram!("notification_e2e_latency_ms").record(e2e_latency_ms as f64);
        }

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
            .chunks(self.recipients_per_batch);

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
