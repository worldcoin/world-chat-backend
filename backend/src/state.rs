//! Application state management

use std::sync::Arc;

use crate::bucket::BucketClient;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// S3 bucket client for image operations
    pub bucket_client: Arc<BucketClient>,
}
