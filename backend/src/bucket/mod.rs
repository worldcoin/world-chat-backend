//! S3 bucket operations for image storage

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

mod client;
mod error;

use chrono::{DateTime, Utc};
pub use client::BucketClient;
pub use error::{BucketError, BucketResult};

/// Presigned URL with expiration information
#[derive(Debug, Clone)]
pub struct PresignedUrl {
    /// The presigned URL for PUT operations
    pub url: String,
    /// ISO-8601 UTC timestamp when the URL expires
    pub expires_at: DateTime<Utc>,
}
