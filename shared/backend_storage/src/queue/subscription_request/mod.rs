//! Subscription request queue operations
//!
//! This module handles subscribe and unsubscribe requests via AWS SQS FIFO queue.

use crate::queue::{sqs_queue::SqsQueue, types::SubscriptionRequest};

/// Subscription request queue for handling subscribe/unsubscribe operations
pub type SubscriptionRequestQueue = SqsQueue<SubscriptionRequest>;
