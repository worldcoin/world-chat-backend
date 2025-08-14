//! World ID integration module for zero-knowledge proof verification.
//!
//! This module provides functionality to verify World ID proofs, which are
//! zero-knowledge proofs that prove a user is a unique human without revealing
//! their identity. The module interfaces with the World ID sequencer to validate
//! proofs submitted by users.
//!
//! # Components
//! - `error`: Custom error types for World ID verification failures
//! - `verifier`: Core verification logic that communicates with the sequencer
//! - `request`: HTTP client utilities for sequencer communication (internal)

pub mod error;
pub mod verifier;

/// HTTP request utilities for communicating with the World ID sequencer.
/// This module provides a reusable HTTP client with connection pooling.
mod request;
