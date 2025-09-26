use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushIdChallengeRequest {
    pub encrypted_push_id_1: String,
    pub encrypted_push_id_2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushIdChallengeResponse {
    pub push_ids_match: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
pub struct AttestationDocumentResponse {
    pub attestation_doc_base64: String,
}
