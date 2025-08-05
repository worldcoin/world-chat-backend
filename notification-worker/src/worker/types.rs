use crate::xmtp::message_api::v1::Envelope;

/// Message type that flows through the worker pipeline
pub type Message = Envelope;

/// Result type for worker operations
pub type WorkerResult<T> = anyhow::Result<T>;
