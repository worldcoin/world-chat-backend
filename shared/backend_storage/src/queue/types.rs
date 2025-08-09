use serde::{Deserialize, Serialize};

/// Subscription request message types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SubscriptionRequest {
    /// Subscribe to a topic
    Subscribe {
        /// HMAC derrived from user, topic and is installation specific
        hmac: String,
        /// Encrypted Braze ID
        encrypted_braze_id: String,
        /// Topic to subscribe to
        topic: String,
        /// Time-to-live duration (unix timestamp in seconds)
        ttl: i64,
    },
    /// Unsubscribe from a topic
    Unsubscribe {
        /// HMAC derrived from user, topic and is installation specific
        hmac: String,
        /// Encrypted Braze ID of the user who is unsubscribing
        encrypted_braze_id: String,
        /// Topic to unsubscribe from
        topic: String,
        /// Subscribers of the same topic
        topic_members: Vec<TopicMember>,
    },
}

/// Notification message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notification {
    /// Topic for the notification
    pub topic: String,
    /// HMAC of the sender
    pub sender_hmac: String,
    /// Notification payload
    /// TODO: This is a placeholder type
    pub payload: String,
}

/// Notification recipient
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopicMember {
    /// Encrypted Braze ID
    pub encrypted_braze_id: String,
    /// HMAC identifier
    pub hmac: String,
}

/// Wrapper for queue messages with metadata
#[derive(Debug, Clone)]
pub struct QueueMessage<T> {
    /// The message body
    pub body: T,
    /// Receipt handle for acknowledging the message
    pub receipt_handle: String,
    /// Message ID
    pub message_id: String,
}

/// Configuration for queue operations
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Queue URL
    pub queue_url: String,
    /// Default maximum number of messages to retrieve
    pub default_max_messages: i32,
    /// Default visibility timeout for messages (in seconds)
    pub default_visibility_timeout: i32,
    /// Default wait time for long polling
    pub default_wait_time_seconds: i32,
}

/// Trait for extracting message group ID for FIFO queues
pub trait MessageGroupId {
    /// Returns the message group ID for FIFO queue ordering
    fn message_group_id(&self) -> String;
}

impl MessageGroupId for SubscriptionRequest {
    fn message_group_id(&self) -> String {
        match self {
            Self::Subscribe { hmac, .. } | Self::Unsubscribe { hmac, .. } => hmac.clone(),
        }
    }
}

impl MessageGroupId for Notification {
    fn message_group_id(&self) -> String {
        self.topic.clone()
    }
}
