//! Generic SQS queue implementation
//!
//! This module provides a generic queue implementation that can be used
//! with any message type that implements the required traits.

use crate::queue::{
    error::QueueResult,
    types::{MessageGroupId, QueueConfig, QueueMessage},
};
use aws_sdk_sqs::Client as SqsClient;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

/// Generic SQS queue for handling any message type
pub struct SqsQueue<T> {
    sqs_client: Arc<SqsClient>,
    config: QueueConfig,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> SqsQueue<T>
where
    T: Serialize + DeserializeOwned + MessageGroupId + Send + Sync,
{
    /// Creates a new generic SQS queue
    ///
    /// # Arguments
    ///
    /// * `sqs_client` - Pre-configured SQS client
    /// * `config` - Queue configuration including URL and default parameters
    #[must_use]
    pub const fn new(sqs_client: Arc<SqsClient>, config: QueueConfig) -> Self {
        Self {
            sqs_client,
            config,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Sends a message to the queue
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    ///
    /// # Returns
    ///
    /// The message ID if successful or an empty string
    ///
    /// # Errors
    ///
    /// Returns `QueueError` if the send operation fails
    pub async fn send_message(&self, message: &T) -> QueueResult<String> {
        // Serialize the message
        let body = serde_json::to_string(message)?;

        // Send to SQS
        let result = self
            .sqs_client
            .send_message()
            .queue_url(&self.config.queue_url)
            .message_body(body)
            .message_group_id(message.message_group_id())
            .send()
            .await?;

        Ok(result
            .message_id()
            .map(std::string::ToString::to_string)
            .unwrap_or_default())
    }

    /// Polls messages from the queue
    ///
    /// # Returns
    ///
    /// A vector of messages with metadata
    ///
    /// # Errors
    ///
    /// Returns `QueueError` if the poll operation fails
    pub async fn poll_messages(&self) -> QueueResult<Vec<QueueMessage<T>>> {
        // Receive messages from SQS
        let result = self
            .sqs_client
            .receive_message()
            .queue_url(&self.config.queue_url)
            .max_number_of_messages(self.config.default_max_messages)
            .visibility_timeout(self.config.default_visibility_timeout)
            .wait_time_seconds(self.config.default_wait_time_seconds)
            .send()
            .await?;

        // Parse messages
        let messages = result
            .messages()
            .iter()
            .filter_map(|msg| {
                let body = msg.body()?;
                let receipt_handle = msg.receipt_handle()?.to_string();
                let message_id = msg.message_id()?.to_string();

                match serde_json::from_str::<T>(body) {
                    Ok(parsed) => Some(QueueMessage {
                        body: parsed,
                        receipt_handle,
                        message_id,
                    }),
                    Err(e) => {
                        tracing::error!("Failed to deserialize message: {}", e);
                        None
                    }
                }
            })
            .collect();

        Ok(messages)
    }

    /// Acknowledges receipt of a message by deleting it from the queue
    ///
    /// # Arguments
    ///
    /// * `receipt_handle` - The receipt handle from the received message
    ///
    /// # Errors
    ///
    /// Returns `QueueError` if the acknowledgment fails
    pub async fn ack_message(&self, receipt_handle: &str) -> QueueResult<()> {
        self.sqs_client
            .delete_message()
            .queue_url(&self.config.queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await?;

        Ok(())
    }
}
