use common_types::{AttestationDocumentResponse, PushIdChallengeRequest, PushIdChallengeResponse};
use std::time::Duration;

use crate::types::AppError;
use reqwest::Client;

/// Default request timeout in seconds
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Maximum number of idle connections to maintain per host
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 10;

/// Trait for the Enclave Worker API
#[async_trait::async_trait]
pub trait EnclaveWorkerApi: Send + Sync {
    /// Challenge 2 encrypted push ids by sending them to the enclave that can decrypt them
    /// returns true if they match.
    async fn challenge_push_ids(
        &self,
        encrypted_push_id_1: String,
        encrypted_push_id_2: String,
    ) -> Result<bool, AppError>;

    /// Get the attestation document from the enclave
    async fn get_attestation_document(&self) -> Result<AttestationDocumentResponse, AppError>;
}

pub struct EnclaveWorkerApiClient {
    enclave_worker_url: String,
    http_client: Client,
}

/// Implements an HTTP client to the Enclave Worker API
///
/// For more details see `enclave-worker` crate in this repository.
impl EnclaveWorkerApiClient {
    /// Creates a new Enclave Worker API client
    ///
    /// # Panics
    ///
    /// If the HTTP client fails to be created
    #[must_use]
    pub fn new(enclave_worker_url: String) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            enclave_worker_url,
            http_client,
        }
    }
}

#[async_trait::async_trait]
impl EnclaveWorkerApi for EnclaveWorkerApiClient {
    async fn challenge_push_ids(
        &self,
        encrypted_push_id_1: String,
        encrypted_push_id_2: String,
    ) -> Result<bool, AppError> {
        // If the push ids are the same, we don't need to challenge them
        if encrypted_push_id_1 == encrypted_push_id_2 {
            return Ok(true);
        }

        let request = PushIdChallengeRequest {
            encrypted_push_id_1,
            encrypted_push_id_2,
        };

        let response = self
            .http_client
            .post(format!("{}/v1/push-id-challenge", self.enclave_worker_url))
            .json(&request)
            .send()
            .await?
            .json::<PushIdChallengeResponse>()
            .await?;

        Ok(response.push_ids_match)
    }

    async fn get_attestation_document(&self) -> Result<AttestationDocumentResponse, AppError> {
        let response = self
            .http_client
            .get(format!(
                "{}/v1/attestation-document",
                self.enclave_worker_url
            ))
            .send()
            .await?
            .json::<AttestationDocumentResponse>()
            .await?;

        Ok(response)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use common_types::AttestationDocumentResponse;

    use super::{AppError, EnclaveWorkerApi};

    pub struct MockEnclaveWorkerApiClient {
        override_push_ids_match: Option<bool>,
        override_attestation_document: Option<AttestationDocumentResponse>,
    }

    impl MockEnclaveWorkerApiClient {
        #[must_use]
        pub const fn new(
            override_push_ids_match: Option<bool>,
            override_attestation_document: Option<AttestationDocumentResponse>,
        ) -> Self {
            Self {
                override_push_ids_match,
                override_attestation_document,
            }
        }
    }

    #[async_trait::async_trait]
    impl EnclaveWorkerApi for MockEnclaveWorkerApiClient {
        async fn challenge_push_ids(
            &self,
            encrypted_push_id_1: String,
            encrypted_push_id_2: String,
        ) -> Result<bool, AppError> {
            Ok(self
                .override_push_ids_match
                .unwrap_or(encrypted_push_id_1 == encrypted_push_id_2))
        }

        async fn get_attestation_document(&self) -> Result<AttestationDocumentResponse, AppError> {
            Ok(self
                .override_attestation_document
                .clone()
                .unwrap_or(AttestationDocumentResponse {
                    attestation_doc_base64: String::new(),
                }))
        }
    }
}
