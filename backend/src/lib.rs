//! World Chat Backend service

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

/// S3 bucket operations
pub mod bucket;

/// Universal error handling
pub mod error;

/// Custom extractors
pub mod extractors;

/// Handler modules
pub mod handlers;

/// Application state
pub mod state;
