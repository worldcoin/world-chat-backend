use anyhow::Context;
// enclave-worker/src/notification_consumer/mod.rs
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};

pub struct NotificationConsumer {
    queue: Arc<NotificationQueue>,
    storage: Arc<PushSubscriptionStorage>,
    shutdown: CancellationToken,
}

impl NotificationConsumer {
    pub fn new(
        queue: Arc<NotificationQueue>,
        storage: Arc<PushSubscriptionStorage>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            queue,
            storage,
            shutdown,
        }
    }

    pub async fn start(self) {
        info!("Starting NotificationConsumer");

        // Poll queue until shutdown
        while !self.shutdown.is_cancelled() {
            tokio::select! {
                _ = self.poll_once() => {},
                _ = self.shutdown.cancelled() => {
                    info!("Queue poller shutting down");
                    break;
                }
            }
        }

        info!("NotificationConsumer shutdown complete");
    }

    async fn poll_once(&self) -> anyhow::Result<()> {
        let messages = self
            .queue
            .poll_messages()
            .await
            .context("Failed to poll messages")?;

        // TODO: Make these requests in parallel to improve performance
        for message in messages {
            self.process_and_ack(message).await?;
        }

        Ok(())
    }

    async fn process_and_ack(&self, message: QueueMessage<Notification>) -> anyhow::Result<()> {
        let notification = message.body;
        let receipt_handle = message.receipt_handle;

        //TODO: Replace this with a call to nitro enclave

        self.queue.ack_message(&receipt_handle).await?;

        Ok(())
    }
}
