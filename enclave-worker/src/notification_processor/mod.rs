use anyhow::Context;
use backend_storage::{
    push_subscription::PushSubscriptionStorage,
    queue::{Notification, NotificationQueue, QueueMessage},
};
use reqwest::Client;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct NotificationProcessor {
    queue: Arc<NotificationQueue>,
    #[allow(dead_code)] // Will be used for nitro enclave integration to delete subscriptions
    storage: Arc<PushSubscriptionStorage>,
    shutdown: CancellationToken,
    http_client: Client,
    braze_api_key: String,
    braze_api_url: String,
}

impl NotificationProcessor {
    pub fn new(
        queue: Arc<NotificationQueue>,
        storage: Arc<PushSubscriptionStorage>,
        shutdown: CancellationToken,
    ) -> Self {
        // Initialize HTTP client with default settings
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        // Get Braze configuration from environment variables, with defaults for now
        let braze_api_key =
            std::env::var("BRAZE_API_KEY").unwrap_or_else(|_| "YOUR_BRAZE_API_KEY".to_string());
        let braze_api_url = std::env::var("BRAZE_API_URL")
            .unwrap_or_else(|_| "https://rest.iad-05.braze.com/messages/send".to_string());

        Self {
            queue,
            storage,
            shutdown,
            http_client,
            braze_api_key,
            braze_api_url,
        }
    }

    pub async fn start(self) {
        info!("Starting NotificationProcessor");

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
            self.process_and_ack(message).await?;
        }

        Ok(())
    }

    async fn process_and_ack(&self, message: QueueMessage<Notification>) -> anyhow::Result<()> {
        let notification = message.body;
        let receipt_handle = message.receipt_handle;

        //TODO: This is temporary to test the e2e flow. This will be replaced with a call to the nitro enclave.
        {
            // Build Braze API request body - scrappy, no validation
            // Using the encrypted push IDs as external user IDs for Braze
            let body = json!({
                "external_user_ids": notification.subscribed_encrypted_push_ids,
                "messages": {
                    "apple_push": {
                        "alert": {
                            "title": "New Message",
                            "body": "You have a new message"
                        },
                        "badge": 1,
                        "sound": "default",
                        "extra": {
                            "topic": notification.topic,
                            "encrypted_message": notification.encrypted_message_base64
                        }
                    },
                    "android_push": {
                        "title": "New Message",
                        "alert": "You have a new message",
                        "priority": "high",
                        "extra": {
                            "topic": notification.topic,
                            "encrypted_message": notification.encrypted_message_base64
                        }
                    }
                }
            });

            // Send to Braze API
            let response = self
                .http_client
                .post(format!("{}/messages/send", self.braze_api_url))
                .header("Authorization", format!("Bearer {}", self.braze_api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .context("Failed to send request to Braze")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                error!("Braze API error - Status: {}, Body: {}", status, error_body);
                return Err(anyhow::anyhow!("Braze API returned error: {}", status));
            }

            info!(
                "Successfully sent notification to {} recipients via Braze",
                notification.subscribed_encrypted_push_ids.len()
            );
        }

        // Acknowledge the message after successful processing
        self.queue.ack_message(&receipt_handle).await?;

        Ok(())
    }
}
