use crate::xmtp::message_api::v1::Envelope;

/// Message type that flows through the worker pipeline
pub type Message = Envelope;

/// Result type for worker operations
pub type WorkerResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Shared state that will be accessible to all workers
/// This will be expanded in the future to include topic cache
#[derive(Clone, Debug)]
pub struct SharedState {
    // Future: topic cache will go here
    // pub topic_cache: Arc<RwLock<HashMap<String, TopicInfo>>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}
