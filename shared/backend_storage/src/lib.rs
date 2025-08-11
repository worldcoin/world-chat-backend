//! Backend storage services for World Chat
//!
//! This crate provides storage functionality shared between the backend and enclave-worker,
//! including push notification subscriptions and SQS queue operations.

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

pub mod auth_proof;
pub mod push_notification;
pub mod queue;
