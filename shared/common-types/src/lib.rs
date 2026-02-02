use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::Display;

/// Enclave track version identifier.
///
/// Each track has its own encryption keys and PCR values.
/// This enables supporting multiple enclave versions simultaneously,
/// allowing gradual app rollouts when enclave code changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, Default)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum EnclaveTrack {
    /// Version 2 enclave track (initial production release)
    #[default]
    V2,
    // TODO: V3 is a placeholder for the next enclave track
    /// Version 3 enclave track (placeholder for future release)
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PushIdChallengeRequest {
    pub encrypted_push_id_1: String,
    pub encrypted_push_id_2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PushIdChallengeResponse {
    pub push_ids_match: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
pub struct AttestationDocumentResponse {
    pub attestation_doc_base64: String,
}
