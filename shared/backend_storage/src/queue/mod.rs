//! Queue operations for World Chat backend
//!
//! This module provides functionality for interacting with AWS SQS FIFO queues,
//! handling subscription requests and notification delivery.

/// Error types for queue operations
pub mod error;
/// Notification queue functionality
pub mod notification;
/// Generic SQS queue implementation
pub mod sqs_queue;
/// Subscription request queue functionality
pub mod subscription_request;
/// Common types for queue operations
pub mod types;

pub use error::{QueueError, QueueResult};
pub use notification::NotificationQueue;
pub use subscription_request::SubscriptionRequestQueue;
pub use types::{Notification, QueueConfig, QueueMessage, Recipient, SubscriptionRequest};
