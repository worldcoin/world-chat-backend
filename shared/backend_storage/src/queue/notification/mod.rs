//! Notification queue operations
//!
//! This module handles notification delivery to subscribers via AWS SQS FIFO queue.

use crate::queue::{sqs_queue::SqsQueue, types::Notification};

/// Notification queue for delivering notifications to subscribers
pub type NotificationQueue = SqsQueue<Notification>;
