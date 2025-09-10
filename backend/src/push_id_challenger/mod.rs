use common_types::{PushIdChallengeRequest, PushIdChallengeResponse};
use std::time::Duration;

use crate::types::AppError;
use reqwest::Client;

/// Default timeout for push id challenger requests
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Maximum number of idle connections to maintain per host
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 10;

#[async_trait::async_trait]
pub trait PushIdChallenger: Send + Sync {
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
    /// Creates a new push id challenger
    ///
    /// # Panics if the HTTP client fails to be created
    #[must_use]
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

#[async_trait::async_trait]
impl PushIdChallenger for PushIdChallengerImpl {
    async fn challenge_push_ids(
        &self,
        push_id_1: String,
        push_id_2: String,
    ) -> Result<bool, AppError> {
        // If the push ids are the same, we don't need to challenge them
        if push_id_1 == push_id_2 {
            return Ok(true);
        }

        let request = PushIdChallengeRequest {
            push_id_1,
            push_id_2,
        };

        let response = self
            .http_client
            .post(format!("{}/push-id-challenge", self.enclave_url))
            .json(&request)
            .send()
            .await?
            .json::<PushIdChallengeResponse>()
            .await?;

        Ok(response.push_ids_match)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use super::*;

    pub struct MockPushIdChallenger {
        override_push_ids_match: Option<bool>,
    }

    impl MockPushIdChallenger {
        #[must_use]
        pub fn new(override_push_ids_match: Option<bool>) -> Self {
            Self {
                override_push_ids_match,
            }
        }
    }

    #[async_trait::async_trait]
    impl PushIdChallenger for MockPushIdChallenger {
        async fn challenge_push_ids(
            &self,
            push_id_1: String,
            push_id_2: String,
        ) -> Result<bool, AppError> {
            Ok(self
                .override_push_ids_match
                .unwrap_or(push_id_1 == push_id_2))
        }
    }
}
