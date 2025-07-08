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

/// Types
pub mod types;

/// Handler modules
pub mod handlers;

/// Application state
pub mod state;
