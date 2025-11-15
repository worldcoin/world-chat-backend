use common_types::{AttestationDocumentResponse, PushIdChallengeRequest, PushIdChallengeResponse};
use std::time::Duration;

use crate::types::AppError;
use axum::http::StatusCode;
use reqwest::{Client, header};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::TracingMiddleware;
use serde_json;

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
    http_client: ClientWithMiddleware,
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
        let reqwest_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
            .build()
            .expect("Failed to create HTTP client");

        let http_client = ClientBuilder::new(reqwest_client)
            .with(TracingMiddleware::default())
            .build();

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

        let url = format!("{}/v1/push-id-challenge", self.enclave_worker_url);
        let json_body = serde_json::to_string(&request)
            .map_err(|_e| AppError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Failed to serialize request",
                false,
            ))?;

        let response = self
            .http_client
            .post(url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(json_body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::new(
                StatusCode::BAD_GATEWAY,
                "enclave_error",
                "Enclave worker service error",
                false,
            ));
        }

        let response_data = response
            .json::<PushIdChallengeResponse>()
            .await?;

        Ok(response_data.push_ids_match)
    }

    async fn get_attestation_document(&self) -> Result<AttestationDocumentResponse, AppError> {
        let url = format!("{}/v1/attestation-document", self.enclave_worker_url);
        let response = self
            .http_client
            .get(url)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::new(
                StatusCode::BAD_GATEWAY,
                "enclave_error",
                "Enclave worker service error",
                false,
            ));
        }

        let response_data = response
            .json::<AttestationDocumentResponse>()
            .await?;

        Ok(response_data)
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
