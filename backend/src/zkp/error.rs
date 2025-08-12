use thiserror::Error;

/// Error types for World ID ZKP verification operations
#[derive(Debug, Error)]
pub enum ZkpError {
    /// The proof verification failed - returned when sequencer says proof is invalid
    #[error("Invalid proof")]
    InvalidProof,

    /// The merkle root is invalid or not found in the World ID tree
    #[error("Invalid merkle root")]
    InvalidMerkleRoot,

    /// The merkle root is too old and has been pruned from the tree
    #[error("Root too old")]
    RootTooOld,

    /// Error occurred in the ZKP prover service
    #[error("Prover error")]
    ProverError,

    /// Failed to parse proof data (e.g., invalid hex string or packed proof format)
    #[error("Invalid proof data: {0}")]
    InvalidProofData(String),

    /// Network error when communicating with the sequencer
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Unexpected response format or status from the sequencer
    #[error("Sequencer error: {0}")]
    InvalidSequencerResponse(String),
}
