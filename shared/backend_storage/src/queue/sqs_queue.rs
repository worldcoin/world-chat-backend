//! Generic SQS queue implementation
//!
//! This module provides a generic queue implementation that can be used
//! with any message type that implements the required traits.

use std::collections::HashMap;
use std::sync::Arc;

use aws_sdk_sqs::types::MessageAttributeValue;
use aws_sdk_sqs::Client as SqsClient;
use opentelemetry::propagation::{Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::{de::DeserializeOwned, Serialize};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::queue::{
    error::QueueResult,
    types::{MessageGroupId, QueueConfig, QueueMessage},
};

/// Carrier for injecting trace context into SQS message attributes
struct SqsMessageAttributeInjector {
    attributes: HashMap<String, MessageAttributeValue>,
}

impl SqsMessageAttributeInjector {
    fn new() -> Self {
        Self {
            attributes: HashMap::new(),
        }
    }
}

impl Injector for SqsMessageAttributeInjector {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(attr) = MessageAttributeValue::builder()
            .data_type("String")
            .string_value(value)
            .build()
        {
            self.attributes.insert(key.to_string(), attr);
        }
    }
}

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

        // Inject current trace context into message attributes
        let propagator = TraceContextPropagator::new();
        let mut injector = SqsMessageAttributeInjector::new();
        let context = tracing::Span::current().context();
        propagator.inject_context(&context, &mut injector);

        let mut request = self
            .sqs_client
            .send_message()
            .queue_url(&self.config.queue_url)
            .message_body(body)
            .message_group_id(message.message_group_id());

        // Add trace context attributes (traceparent, tracestate)
        for (key, value) in injector.attributes {
            request = request.message_attributes(key, value);
        }

        let result = request.send().await?;

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
        // Receive messages from SQS, including trace context attributes
        let result = self
            .sqs_client
            .receive_message()
            .queue_url(&self.config.queue_url)
            .max_number_of_messages(self.config.default_max_messages)
            .visibility_timeout(self.config.default_visibility_timeout)
            .wait_time_seconds(self.config.default_wait_time_seconds)
            .message_attribute_names("traceparent")
            .message_attribute_names("tracestate")
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

                // Extract trace context from message attributes
                let trace_context: HashMap<String, String> = msg
                    .message_attributes()
                    .map(|attrs| {
                        attrs
                            .iter()
                            .filter_map(|(k, v)| {
                                v.string_value().map(|s| (k.clone(), s.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                match serde_json::from_str::<T>(body) {
                    Ok(parsed) => Some(QueueMessage {
                        body: parsed,
                        receipt_handle,
                        message_id,
                        trace_context,
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
