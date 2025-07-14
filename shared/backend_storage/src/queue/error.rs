use aws_sdk_sqs::error::SdkError;
use aws_sdk_sqs::operation::delete_message::DeleteMessageError;
use aws_sdk_sqs::operation::receive_message::ReceiveMessageError;
use aws_sdk_sqs::operation::send_message::SendMessageError;
use thiserror::Error;

/// Result type alias for queue operations
pub type QueueResult<T> = Result<T, QueueError>;

/// Error types for queue operations
#[derive(Error, Debug)]
pub enum QueueError {
    /// Error receiving messages from SQS
    #[error("Failed to receive messages from SQS")]
    ReceiveMessage(#[from] SdkError<ReceiveMessageError>),

    /// Error sending message to SQS
    #[error("Failed to send message to SQS")]
    SendMessage(#[from] SdkError<SendMessageError>),

    /// Error deleting message from SQS
    #[error("Failed to delete message from SQS")]
    DeleteMessage(#[from] SdkError<DeleteMessageError>),

    /// Error serializing message to JSON
    #[error("Failed to serialize message: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Error deserializing message from JSON
    #[error("Failed to deserialize message: {0}")]
    DeserializationError(String),

    /// Invalid message format
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    /// Message group ID is required for FIFO queues
    #[error("Message group ID required for FIFO queue")]
    MissingMessageGroupId,

    /// Upstream service error (5xx)
    #[error("Upstream service error")]
    UpstreamError,
}

impl QueueError {
    /// Checks if this error represents an upstream (5xx) error
    #[must_use]
    pub fn is_upstream_error(&self) -> bool {
        match self {
            Self::ReceiveMessage(sdk_err) => Self::check_sdk_error_status(sdk_err),
            Self::SendMessage(sdk_err) => Self::check_sdk_error_status(sdk_err),
            Self::DeleteMessage(sdk_err) => Self::check_sdk_error_status(sdk_err),
            Self::UpstreamError => true,
            _ => false,
        }
    }

    fn check_sdk_error_status<E>(sdk_err: &SdkError<E>) -> bool {
        if let SdkError::ServiceError(err) = sdk_err {
            let raw = err.raw();
            let status = raw.status();
            return status.as_u16() >= 500;
        }
        false
    }
}
