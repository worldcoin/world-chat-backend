use common_types::{PushIdChallengeRequest, PushIdChallengeResponse};
use std::time::Duration;

use crate::types::AppError;
use reqwest::Client;

/// Default timeout for push id challenger requests
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Maximum number of idle connections to maintain per host
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 10;

pub trait PushIdChallenger {
    async fn challenge_push_ids(
        &self,
        push_id_1: String,
        push_id_2: String,
    ) -> Result<bool, AppError>;
}

pub struct PushIdChallengerImpl {
    enclave_url: String,
    http_client: Client,
}

impl PushIdChallengerImpl {
    pub fn new(enclave_url: String) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            enclave_url,
            http_client,
        }
    }
}

impl PushIdChallenger for PushIdChallengerImpl {
    async fn challenge_push_ids(
        &self,
        push_id_1: String,
        push_id_2: String,
    ) -> Result<bool, AppError> {
        let request = PushIdChallengeRequest {
            push_id_1,
            push_id_2,
        };

        let response = self
            .http_client
            .post(&self.enclave_url)
            .json(&request)
            .send()
            .await?
            .json::<PushIdChallengeResponse>()
            .await?;

        Ok(response.push_ids_match)
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;

    pub struct MockPushIdChallenger {
        push_ids_match: bool,
    }

    impl PushIdChallenger for MockPushIdChallenger {
        async fn challenge_push_ids(
            &self,
            _push_id_1: String,
            _push_id_2: String,
        ) -> Result<bool, AppError> {
            Ok(self.push_ids_match)
        }
    }
}
