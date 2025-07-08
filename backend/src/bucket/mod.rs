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

#[derive(Debug, Clone)]
pub struct PresignedUrl {
    pub url: String,
    pub expires_at: DateTime<Utc>,
}
