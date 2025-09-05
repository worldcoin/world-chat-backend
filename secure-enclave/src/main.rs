use anyhow::{anyhow, Context, Result};
use enclave_types::{
    BrazeConfig, EnclaveError, EnclaveRequest, EnclaveResponse, NotificationRequest,
    NotificationResponse,
};
use hyper::client::HttpConnector;
use hyper::client::connect::{Connected, Connection};
use hyper::service::Service;
use hyper::{Body, Client, Method, Request, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use webpki_roots;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::RwLock;
use tokio_vsock::{VsockAddr, VsockStream};
use tracing::{debug, error, info};

/// Port where the enclave will listen for vsock connections
const ENCLAVE_PORT: u32 = 5000;

/// Shared state for the enclave
struct EnclaveState {
    braze_config: Option<BrazeConfig>,
    http_client: Option<Client<hyper_rustls::HttpsConnector<HttpConnector>>>,
    vsock_client: Option<Client<HttpsConnector<VsockConnector>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with enhanced debug output
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_target(true)  // Show module path
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .pretty()
        .init();

    info!("🔐 Starting Secure Enclave");
    info!("📡 Listening on port {}", ENCLAVE_PORT);

    // Create HTTP client with rustls for HTTPS support (for non-proxy mode)
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let http_client = Client::builder()
        .http2_only(false) // Allow both HTTP/1.1 and HTTP/2
        .build(https);

    // Initialize enclave state
    let state = Arc::new(RwLock::new(EnclaveState {
        braze_config: None,
        http_client: Some(http_client),
        vsock_client: None,
    }));

    // Start listening for requests from the enclave worker
    if let Err(e) = pontifex::listen(ENCLAVE_PORT, move |request| {
        let state = state.clone();
        async move { handle_request(state, request).await }
    })
    .await
    {
        error!("Failed to start server: {}", e);
        return Err(anyhow!("Failed to start enclave server: {}", e));
    }

    Ok(())
}

/// Handle incoming requests from the enclave worker
async fn handle_request(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclaveRequest,
) -> EnclaveResponse {
    let request_type = match &request {
        EnclaveRequest::Initialize(_) => "Initialize",
        EnclaveRequest::Notification(_) => "Notification",
        EnclaveRequest::HealthCheck => "HealthCheck",
    };
    
    debug!("Received request type: {}", request_type);
    debug!("Request details: {:#?}", request);
    
    let start = std::time::Instant::now();
    
    let response = match request {
        EnclaveRequest::Initialize(config) => handle_initialize(state, config).await,
        EnclaveRequest::Notification(notification) => {
            handle_notification(state, *notification).await
        }
        EnclaveRequest::HealthCheck => handle_health_check(state).await,
    };
    
    let elapsed = start.elapsed();
    debug!("Request {} processed in {:?}", request_type, elapsed);
    debug!("Response: {:#?}", response);
    
    response
}

/// Initialize the enclave with Braze configuration
async fn handle_initialize(
    state: Arc<RwLock<EnclaveState>>,
    config: BrazeConfig,
) -> EnclaveResponse {
    info!("🔧 Initializing enclave with Braze configuration");
    debug!("API Endpoint: {}", config.api_endpoint);
    
    let mut state = state.write().await;
    
    // If proxy is configured, create a vsock client with TLS
    if let Some(ref proxy) = config.proxy_config {
        info!(
            "🔗 Configuring vsock proxy with TLS on port {}",
            proxy.port
        );
        
        // Create vsock connector for connecting to parent instance
        // Parent CID is always 3 from enclave perspective
        let vsock_connector = VsockConnector::new(3, proxy.port as u32);
        
        // Create rustls config for TLS
        let rustls_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS
                    .0
                    .iter()
                    .map(|ta| {
                        rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                            ta.subject,
                            ta.spki,
                            ta.name_constraints,
                        )
                    })
                    .collect(),
            })
            .with_no_client_auth();
        
        // Wrap vsock connector with HTTPS connector for TLS
        let https_connector = HttpsConnector::from((vsock_connector, rustls_config));
        
        let vsock_client = Client::builder()
            .http2_only(false)
            .build(https_connector);
            // Enable HTTP/2 which allows multiplexing
// let vsock_client = Client::builder()
//     .http2_only(true)  // Force HTTP/2
//     .http2_keep_alive_interval(Some(Duration::from_secs(30)))
//     .http2_keep_alive_timeout(Duration::from_secs(10))
//     .pool_idle_timeout(Duration::from_secs(90))  // Keep connections alive
//     .pool_max_idle_per_host(2)  // Keep some connections warm
//     .build(https_connector);
        
        state.vsock_client = Some(vsock_client);
        info!("✅ Vsock client with TLS configured for proxy communication");
    }
    
    state.braze_config = Some(config);

    info!("✅ Enclave initialized successfully");
    EnclaveResponse::InitializeSuccess
}

/// Handle health check requests
async fn handle_health_check(state: Arc<RwLock<EnclaveState>>) -> EnclaveResponse {
    let state = state.read().await;
    let initialized = state.braze_config.is_some();

    debug!("Health check - Initialized: {}", initialized);
    EnclaveResponse::HealthCheckOk { initialized }
}

/// Handle notification requests and forward them to Braze API
async fn handle_notification(
    state: Arc<RwLock<EnclaveState>>,
    notification: NotificationRequest,
) -> EnclaveResponse {
    info!(
        "📨 Processing notification request: {}",
        notification.request_id
    );
    debug!("Notification details: {:?}", notification);

    let state = state.read().await;

    // Check if enclave is initialized
    let config = match &state.braze_config {
        Some(config) => config,
        None => {
            error!("Enclave not initialized - Initialize must be called first");
            error!("Received notification request without prior initialization");
            return EnclaveResponse::Error(EnclaveError::NotInitialized);
        }
    };

    // Prepare Braze API request
    let braze_request = match prepare_braze_request(&notification, config) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to prepare Braze request: {}", e);
            return EnclaveResponse::Error(EnclaveError::SerializationError(e.to_string()));
        }
    };

    // Send request to Braze using appropriate client
    let result = if config.proxy_config.is_some() {
        // Use vsock client for proxy mode
        if let Some(ref vsock_client) = state.vsock_client {
            send_to_braze_with_client(vsock_client, braze_request, config).await
        } else {
            error!("Vsock client not initialized for proxy mode");
            return EnclaveResponse::Error(EnclaveError::NotInitialized);
        }
    } else {
        // Use regular HTTPS client for direct mode
        if let Some(ref http_client) = state.http_client {
            send_to_braze_with_client(http_client, braze_request, config).await
        } else {
            error!("HTTP client not initialized");
            return EnclaveResponse::Error(EnclaveError::NotInitialized);
        }
    };
    
    match result {
        Ok(response) => {
            info!(
                "✅ Notification sent successfully for request: {}",
                notification.request_id
            );
            debug!("Braze response details: {:#?}", response);
            EnclaveResponse::NotificationSuccess(response)
        }
        Err(e) => {
            error!("Failed to send notification: {:?}", e);
            error!("Error context: {:#?}", e);
            error!("Request ID: {}", notification.request_id);
            error!("User ID: {}", notification.external_user_id);
            
            // Extract detailed error message with context
            let error_chain = format!("{:#}", e);
            EnclaveResponse::Error(EnclaveError::NotificationFailed(error_chain))
        }
    }
}

/// Prepare a request to the Braze API
fn prepare_braze_request(
    notification: &NotificationRequest,
    config: &BrazeConfig,
) -> Result<BrazeApiRequest> {
    // Create Braze API request payload
    // This follows the Braze /messages/send API format from:
    // https://www.braze.com/docs/api/endpoints/messaging/send_messages/post_send_messages/
    
    let mut messages = serde_json::Map::new();

    // Apple Push Notification
    let mut apple_push = serde_json::Map::new();
    apple_push.insert(
        "alert".to_string(),
        serde_json::json!({
            "title": notification.title,
            "body": notification.message,
        }),
    );
    apple_push.insert("badge".to_string(), serde_json::Value::Number(1.into()));
    
    // Add custom data to apple push if provided
    if let Some(ref custom_data) = notification.custom_data {
        let mut extra = serde_json::Map::new();
        for (key, value) in custom_data {
            extra.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        apple_push.insert("extra".to_string(), serde_json::Value::Object(extra));
    }
    
    messages.insert("apple_push".to_string(), serde_json::Value::Object(apple_push));

    // Android Push Notification
    let mut android_push = serde_json::Map::new();
    android_push.insert(
        "title".to_string(),
        serde_json::Value::String(notification.title.clone()),
    );
    android_push.insert(
        "alert".to_string(),
        serde_json::Value::String(notification.message.clone()),
    );
    
    // Add custom data to android push if provided
    if let Some(ref custom_data) = notification.custom_data {
        let mut extra = serde_json::Map::new();
        for (key, value) in custom_data {
            extra.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        android_push.insert("extra".to_string(), serde_json::Value::Object(extra));
    }
    
    messages.insert(
        "android_push".to_string(),
        serde_json::Value::Object(android_push),
    );

    let request = BrazeApiRequest {
        api_key: config.api_key.clone(),
        external_user_id: notification.external_user_id.clone(),
        messages,
        campaign_id: None,  // Can be set from notification if needed
        send_id: None,// Some(notification.request_id.clone()),  // Use request_id as send_id for tracking
    };

    Ok(request)
}

/// Send request to Braze API through the proxy or direct
async fn send_to_braze_with_client<C>(
    client: &Client<C>,
    braze_request: BrazeApiRequest,
    config: &BrazeConfig,
) -> Result<NotificationResponse>
where
    C: hyper::client::connect::Connect + Clone + Send + Sync + 'static,
{
    // Always use the actual Braze endpoint URL
    // vsock-proxy will handle the forwarding
    let url = format!("{}/messages/send", config.api_endpoint);
    debug!("Sending request to: {}", url);

    // Serialize the request body
    let body = serde_json::to_string(&braze_request.to_json())
        .context("Failed to serialize Braze request")?;

    debug!("Request body: {}", body);

    // Create HTTP request
    let req = Request::builder()
        .method(Method::POST)
        .uri(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", braze_request.api_key))
        .header("Host", "rest.iad-05.braze.com") // Ensure proper Host header
        .body(Body::from(body))
        .context("Failed to build HTTP request")?;

    // Send the request with detailed error handling
    debug!("Sending HTTP request via {}", 
        if config.proxy_config.is_some() { "vsock proxy" } else { "direct connection" }
    );
    
    let resp = client
        .request(req)
        .await
        .map_err(|e| {
            error!("HTTP request failed: {:?}", e);
            error!("Target URL: {}", url);
            if let Some(proxy) = &config.proxy_config {
                error!("Using vsock proxy on port {}", proxy.port);
            }
            e
        })
        .context("Failed to send HTTP request to Braze")?;

    let status = resp.status();
    debug!("Response status: {}", status);

    // Read response body
    let body_bytes = hyper::body::to_bytes(resp.into_body())
        .await
        .context("Failed to read response body")?;

    let body_str = String::from_utf8_lossy(&body_bytes);
    debug!("Response body: {}", body_str);

    // Parse response with detailed error handling
    if status.is_success() {
        let braze_response: BrazeApiResponse = serde_json::from_slice(&body_bytes)
            .map_err(|e| {
                error!("Failed to parse Braze response: {:?}", e);
                error!("Response body: {}", body_str);
                e
            })
            .context("Failed to parse Braze response JSON")?;

        Ok(NotificationResponse {
            request_id: braze_request.send_id.clone().unwrap_or_else(|| "unknown".to_string()),
            dispatch_id: braze_response.dispatch_id,
            messages_queued: braze_response.messages.len() as u32,
            errors: braze_response.errors.unwrap_or_default(),
        })
    } else {
        error!("Braze API error - Status: {}", status);
        error!("Response body: {}", body_str);
        error!("Request URL: {}", url);
        
        // Try to parse error response for more details
        if let Ok(error_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            error!("Parsed error response: {:#?}", error_json);
        }
        
        Err(anyhow!(
            "Braze API returned error status {}: {}",
            status,
            body_str
        ))
    }
}

/// Internal structure for Braze API requests
/// Based on: https://www.braze.com/docs/api/endpoints/messaging/send_messages/post_send_messages/
#[derive(Debug, Serialize)]
struct BrazeApiRequest {
    api_key: String,  // Used for Authorization header, not in body
    external_user_id: String,  // The user to send to
    messages: serde_json::Map<String, serde_json::Value>,
    campaign_id: Option<String>,  // Optional campaign ID for tracking
    send_id: Option<String>,  // Optional send ID for deduplication
}

impl BrazeApiRequest {
    /// Convert to JSON for the API call (without the api_key in body)
    /// Follows Braze API specification for /messages/send endpoint
    fn to_json(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "external_user_ids": [self.external_user_id],
            "messages": self.messages,
        });
        
        // Add optional fields if present
        if let Some(ref campaign_id) = self.campaign_id {
            body["campaign_id"] = serde_json::Value::String(campaign_id.clone());
        }
        
        if let Some(ref send_id) = self.send_id {
            body["send_id"] = serde_json::Value::String(send_id.clone());
        }
        
        body
    }
}

/// Braze API response structure
/// Based on: https://www.braze.com/docs/api/endpoints/messaging/send_messages/post_send_messages/#response-details
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BrazeApiResponse {
    dispatch_id: Option<String>,  // Unique ID for each transmission
    message: Option<String>,       // Success or error message
    errors: Option<Vec<String>>,   // Array of error messages if any
    #[serde(default)]
    messages: Vec<String>,         // Additional messages from API
}

// ============================================================================
// VSOCK CONNECTOR IMPLEMENTATION
// ============================================================================

/// Wrapper around VsockStream to implement hyper Connection trait
struct VsockStreamWrapper {
    stream: VsockStream,
}

impl AsyncRead for VsockStreamWrapper {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for VsockStreamWrapper {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

impl Connection for VsockStreamWrapper {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

/// Vsock connector for Hyper client
#[derive(Clone)]
struct VsockConnector {
    cid: u32,
    port: u32,
}

impl VsockConnector {
    fn new(cid: u32, port: u32) -> Self {
        Self { cid, port }
    }
}

impl Service<Uri> for VsockConnector {
    type Response = VsockStreamWrapper;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let cid = self.cid;
        let port = self.port;
        
        // Extract the host for logging (the actual connection goes through vsock)
        let host = uri.host().unwrap_or("unknown").to_string();
        
        Box::pin(async move {
            debug!("Connecting to {} via vsock CID {} port {}", host, cid, port);
            
            let addr = VsockAddr::new(cid, port);
            let stream = VsockStream::connect(addr).await.map_err(|e| {
                error!("Failed to connect to vsock: {}", e);
                io::Error::new(io::ErrorKind::ConnectionRefused, e)
            })?;
            
            debug!("Vsock connection established for {}", host);
            Ok(VsockStreamWrapper { stream })
        })
    }
}
