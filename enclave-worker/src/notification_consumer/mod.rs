// enclave-worker/src/notification_consumer/mod.rs
use backend_storage::{push_subscription::PushSubscriptionStorage, queue::NotificationQueue};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::info;

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
        // Poll SQS queue and send to channel
        // Implementation details...
        tokio::time::sleep(Duration::from_millis(1000)).await;
        Ok(())
    }
}
