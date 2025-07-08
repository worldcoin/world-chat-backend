//! Application state management

use std::sync::Arc;

use crate::image_storage::ImageStorageClient;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Image storage client for S3 operations
    pub image_storage_client: Arc<ImageStorageClient>,
}
