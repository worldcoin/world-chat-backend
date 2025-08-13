use std::sync::LazyLock;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;

use super::error::ZkpError;

/// Default timeout for World ID sequencer requests
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Maximum number of idle connections to maintain per host
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 10;

/// Shared HTTP client with connection pooling for all ZKP requests.
/// This client is initialized once and reused for better performance.
static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
        .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
        .user_agent(format!("world-chat-backend/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to create HTTP client")
});

/// HTTP request handler for World ID proof verification.
///
/// This struct provides a simple interface for communicating with the World ID
/// sequencer API. It uses a shared HTTP client with connection pooling to
/// efficiently handle multiple verification requests.
pub struct Request;

impl Request {
    /// Makes a POST request to the given URL with a JSON body.
    ///
    /// # Arguments
    /// * `url` - The URL to send the request to
    /// * `body` - The request body to serialize as JSON
    ///
    /// # Returns
    /// The raw response from the server
    ///
    /// # Errors
    /// Returns an error if the request fails or timeout occurs
    pub async fn post<T>(url: &str, body: T) -> Result<reqwest::Response, ZkpError>
    where
        T: Serialize + Send + Sync,
    {
        HTTP_CLIENT
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the HTTP client can be created successfully.
    /// This test ensures the static initialization doesn't panic.
    #[test]
    fn test_http_client_initialization() {
        // Force the lazy initialization of the HTTP client
        let _ = &*HTTP_CLIENT;
    }
}
