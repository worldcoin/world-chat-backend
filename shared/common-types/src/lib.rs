use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushIdChallengeRequest {
    pub push_id_1: String,
    pub push_id_2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushIdChallengeResponse {
    pub push_ids_match: bool,
}
