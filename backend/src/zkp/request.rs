use std::sync::LazyLock;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use tracing::debug;

use super::error::ZkpError;

/// Shared HTTP client with connection pooling for all ZKP requests.
/// This client is initialized once and reused for better performance.
static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .user_agent(format!("world-chat-backend/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to create HTTP client")
});

/// A simple HTTP request wrapper for the World ID sequencer.
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
    pub async fn post<T>(url: String, body: T) -> Result<reqwest::Response, ZkpError>
    where
        T: Serialize + Send + Sync,
    {
        debug!("Sending POST request to: {}", url);

        HTTP_CLIENT
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                debug!("Request failed: {}", e);
                e.into()
            })
    }
}
