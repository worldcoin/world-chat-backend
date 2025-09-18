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
    #[error("Failed to receive messages from SQS {0:?}")]
    ReceiveMessage(#[from] SdkError<ReceiveMessageError>),

    /// Error sending message to SQS
    #[error("Failed to send message to SQS {0:?}")]
    SendMessage(#[from] SdkError<SendMessageError>),

    /// Error deleting message from SQS
    #[error("Failed to delete message from SQS {0:?}")]
    DeleteMessage(#[from] SdkError<DeleteMessageError>),

    /// Error serializing message to JSON
    #[error("Failed to serialize message")]
    SerializationError(#[from] serde_json::Error),
}
