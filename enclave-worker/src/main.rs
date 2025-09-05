use anyhow::{anyhow, Context, Result};
use enclave_types::{
    BrazeConfig, EnclaveRequest, EnclaveResponse, NotificationRequest, ProxyConfig,
};
use pontifex::ConnectionDetails;
use std::collections::HashMap;
use std::env;
use tracing::{debug, error, info, warn};

/// Default enclave CID (Context ID) - typically 16 for the first enclave
const DEFAULT_ENCLAVE_CID: u32 = 16;
/// Default enclave port for vsock communication
const DEFAULT_ENCLAVE_PORT: u32 = 5000;

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
        .with_ansi(true)
        .pretty()  // Pretty formatting for better readability
        .init();

    info!("🚀 Starting Enclave Worker");

    // Get configuration from environment variables
    let enclave_cid = env::var("ENCLAVE_CID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_ENCLAVE_CID);

    let enclave_port = env::var("ENCLAVE_PORT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_ENCLAVE_PORT);

    let braze_api_key = env::var("BRAZE_API_KEY")
        .context("BRAZE_API_KEY environment variable is required")?;

    let braze_api_endpoint = env::var("BRAZE_API_ENDPOINT")
        .unwrap_or_else(|_| "https://rest.iad-05.braze.com".to_string());

    // Proxy configuration for enclave network access
    let proxy_host = env::var("PROXY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let proxy_port = env::var("PROXY_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);

    info!(
        "📡 Connecting to enclave at CID: {}, Port: {}",
        enclave_cid, enclave_port
    );
    info!("🔗 Braze endpoint: {}", braze_api_endpoint);
    info!("🔗 Proxy configuration: {}:{}", proxy_host, proxy_port);

    let connection = ConnectionDetails::new(enclave_cid, enclave_port);

    // Initialize the enclave with Braze configuration
    info!("🔐 Initializing secure enclave with Braze configuration...");
    
    let init_request = EnclaveRequest::Initialize(BrazeConfig {
        api_key: braze_api_key,
        api_endpoint: braze_api_endpoint,
        proxy_config: Some(ProxyConfig {
            host: proxy_host,
            port: proxy_port,
        }),
    });

    match send_to_enclave(&connection, &init_request).await? {
        EnclaveResponse::InitializeSuccess => {
            info!("✅ Enclave initialized successfully");
        }
        EnclaveResponse::Error(e) => {
            error!("Failed to initialize enclave: {:?}", e);
            error!("Error details: {:#?}", e);
            return Err(anyhow!("Enclave initialization failed: {:?}", e))
                .context("Failed during enclave initialization phase");
        }
        unexpected => {
            warn!("Unexpected response from enclave during initialization: {:#?}", unexpected);
        }
    }

    // Health check
    info!("🏥 Performing health check...");
    match send_to_enclave(&connection, &EnclaveRequest::HealthCheck).await? {
        EnclaveResponse::HealthCheckOk { initialized } => {
            info!("✅ Health check passed. Enclave initialized: {}", initialized);
        }
        _ => {
            warn!("Health check returned unexpected response");
        }
    }

    // Example notification loop - in production, this would process from a queue
    info!("📬 Starting notification processing loop...");
    
    // Send an example notification
    let example_notification = NotificationRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        external_user_id: "test_user_123".to_string(),
        title: "Test Notification".to_string(),
        message: "This is a test notification from the enclave worker".to_string(),
        custom_data: Some({
            let mut data = HashMap::new();
            data.insert("source".to_string(), "enclave-worker".to_string());
            data.insert("environment".to_string(), "development".to_string());
            data
        }),
        trigger_properties: None,
    };

    // Send the notification request
    info!("📨 Sending test notification with ID: {}", example_notification.request_id);
    debug!("Notification details: {:?}", example_notification);

    let notification_request = EnclaveRequest::Notification(Box::new(example_notification.clone()));
    
    match send_to_enclave(&connection, &notification_request).await? {
        EnclaveResponse::NotificationSuccess(response) => {
            info!(
                "✅ Notification sent successfully! Request ID: {}, Messages queued: {}",
                response.request_id, response.messages_queued
            );
            if let Some(dispatch_id) = response.dispatch_id {
                info!("   Dispatch ID: {}", dispatch_id);
            }
            if !response.errors.is_empty() {
                warn!("   Errors reported: {:?}", response.errors);
            }
        }
        EnclaveResponse::Error(e) => {
            error!("❌ Failed to send notification: {}", e);
        }
        _ => {
            warn!("Unexpected response from enclave for notification request");
        }
    }

    // Simulate periodic notification sending
    info!("💤 Worker is running. Press Ctrl+C to shutdown...");
    
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    let mut notification_count = 1;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Send periodic test notifications
                let notification = NotificationRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    external_user_id: "139b3fea5833c1461524f965ceedb1adc6a657a79780df951db26cf2b171bc3d".to_string(),
                    title: format!("Periodic Notification #{}", notification_count),
                    message: format!("This is periodic test notification number {}", notification_count),
                    custom_data: None,
                    trigger_properties: None,
                };

                info!("📨 Sending periodic notification #{}", notification_count);
                
                let request = EnclaveRequest::Notification(Box::new(notification));
                match send_to_enclave(&connection, &request).await {
                    Ok(EnclaveResponse::NotificationSuccess(response)) => {
                        info!("✅ Periodic notification #{} sent successfully", notification_count);
                        debug!("Response: {:?}", response);
                    }
                    Ok(EnclaveResponse::Error(e)) => {
                        error!("❌ Failed to send periodic notification: {}", e);
                    }
                    Err(e) => {
                        error!("❌ Communication error with enclave: {}", e);
                    }
                    _ => {
                        warn!("Unexpected response from enclave");
                    }
                }
                
                notification_count += 1;
            }
            _ = tokio::signal::ctrl_c() => {
                info!("📍 Received shutdown signal");
                break;
            }
        }
    }

    info!("👋 Shutting down Enclave Worker gracefully");
    Ok(())
}

/// Send a request to the secure enclave and receive the response
async fn send_to_enclave(
    connection: &ConnectionDetails,
    request: &EnclaveRequest,
) -> Result<EnclaveResponse> {
    debug!("Sending request to enclave: {:#?}", request);
    debug!("Using vsock connection to enclave");
    
    let start = std::time::Instant::now();
    
    let response = pontifex::send::<EnclaveRequest, EnclaveResponse>(connection.clone(), request)
        .await
        .map_err(|e| {
            error!("Pontifex communication error: {:?}", e);
            error!("Failed to communicate with enclave via vsock");
            error!("Ensure enclave is running and listening on the configured port");
            e
        })
        .context("Failed to communicate with secure enclave")?;
    
    let elapsed = start.elapsed();
    debug!("Received response from enclave in {:?}: {:#?}", elapsed, response);
    
    Ok(response)
}
