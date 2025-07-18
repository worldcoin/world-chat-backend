use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{error, info, warn};

use crate::generated::xmtp::message_api::v1::{
    message_api_client::MessageApiClient, Envelope, SubscribeAllRequest,
};
use crate::types::environment::Environment;

/// Configuration for the XMTP worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// XMTP node endpoint
    pub xmtp_endpoint: String,
    /// Number of worker tasks to spawn
    pub num_workers: usize,
    /// Initial reconnection delay in milliseconds
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection delay in milliseconds
    pub max_reconnect_delay_ms: u64,
}

impl WorkerConfig {
    /// Creates a new WorkerConfig from the given environment
    pub fn from_environment(env: &Environment) -> Self {
        // Allow override from environment variable
        let xmtp_endpoint = std::env::var("XMTP_GRPC_ADDRESS")
            .unwrap_or_else(|_| env.xmtp_endpoint().to_string());
        
        Self {
            xmtp_endpoint,
            num_workers: env.default_num_workers(),
            reconnect_delay_ms: 100,
            max_reconnect_delay_ms: 30000,
        }
    }
    
    /// Creates a WorkerConfig with custom settings
    pub fn new(xmtp_endpoint: String, num_workers: usize) -> Self {
        Self {
            xmtp_endpoint,
            num_workers,
            reconnect_delay_ms: 100,
            max_reconnect_delay_ms: 30000,
        }
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let env = Environment::from_env();
        Self::from_environment(&env)
    }
}

/// Main worker that manages the gRPC stream and worker pool
pub struct XmtpWorker {
    config: WorkerConfig,
    client: MessageApiClient<Channel>,
    shutdown_token: CancellationToken,
}

impl XmtpWorker {
    /// Creates a new XMTP worker
    pub async fn new(config: WorkerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Connecting to XMTP node at {}", config.xmtp_endpoint);
        
        // Create the channel - tonic will handle TLS automatically
        let channel = Channel::from_shared(config.xmtp_endpoint.clone())?
            .connect()
            .await?;
        
        let client = MessageApiClient::new(channel);
        let shutdown_token = CancellationToken::new();
        
        Ok(Self {
            config,
            client,
            shutdown_token,
        })
    }
    
    /// Returns a clone of the shutdown token for external control
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }
    
    /// Starts the worker with stream listener and message processors
    pub async fn start(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting XMTP worker with {} workers", self.config.num_workers);
        
        // Create the message channel with capacity of 2 * num_workers
        let channel_capacity = self.config.num_workers * 2;
        let (message_tx, message_rx) = flume::bounded::<Envelope>(channel_capacity);
        info!("Created flume channel with capacity: {}", channel_capacity);
        
        // Spawn worker tasks
        let mut worker_handles = Vec::new();
        
        for i in 0..self.config.num_workers {
            let worker_id = i;
            let receiver = message_rx.clone();
            let shutdown_token = self.shutdown_token.clone();
            
            let handle = tokio::spawn(async move {
                message_worker(worker_id, receiver, shutdown_token).await;
            });
            
            worker_handles.push(handle);
        }
        
        // Start the stream listener in the background
        let shutdown_token = self.shutdown_token.clone();
        let shutdown_token_clone = shutdown_token.clone();
        let listener_task = self.start_stream_listener(message_tx.clone(), shutdown_token);
        
        // Run the listener and wait for shutdown
        tokio::select! {
            result = listener_task => {
                if let Err(e) = result {
                    error!("Stream listener error: {}", e);
                }
            }
            _ = shutdown_token_clone.cancelled() => {
                info!("Shutdown signal received, stopping workers...");
            }
        }
        
        // Drop the original sender to close the channel
        drop(message_tx);
        
        // Wait for all workers to complete
        for handle in worker_handles {
            if let Err(e) = handle.await {
                error!("Worker task error: {}", e);
            }
        }
        
        info!("All workers stopped");
        Ok(())
    }
    
    /// Starts listening to the XMTP message stream
    async fn start_stream_listener(
        &mut self, 
        message_tx: flume::Sender<Envelope>,
        shutdown_token: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut reconnect_delay = self.config.reconnect_delay_ms;
        
        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Stream listener shutting down");
                    return Ok(());
                }
                result = self.subscribe_and_process(message_tx.clone()) => {
                    match result {
                        Ok(_) => {
                            warn!("Stream ended unexpectedly, reconnecting...");
                            reconnect_delay = self.config.reconnect_delay_ms; // Reset delay on successful connection
                        }
                        Err(e) => {
                            error!("Stream error: {}, reconnecting in {}ms", e, reconnect_delay);
                            
                            // Wait with cancellation support
                            tokio::select! {
                                _ = shutdown_token.cancelled() => {
                                    info!("Stream listener shutting down during reconnect delay");
                                    return Ok(());
                                }
                                _ = sleep(Duration::from_millis(reconnect_delay)) => {}
                            }
                            
                            // Exponential backoff
                            reconnect_delay = (reconnect_delay * 2).min(self.config.max_reconnect_delay_ms);
                        }
                    }
                }
            }
        }
    }
    
    /// Subscribes to the message stream and processes messages
    async fn subscribe_and_process(&mut self, message_tx: flume::Sender<Envelope>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Subscribing to XMTP message stream");
        
        let request = SubscribeAllRequest {};
        info!("Sending SubscribeAllRequest to gRPC server");
        
        let response = self.client.subscribe_all(request).await?;
        info!("Successfully established subscription stream");
        
        let mut stream = response.into_inner();
        let mut message_count = 0;
        
        info!("Waiting for messages from XMTP stream...");
        
        while let Some(envelope) = stream.message().await? {
            message_count += 1;
            info!(
                "Received message #{} - Topic: {}, Timestamp: {}, Size: {} bytes",
                message_count,
                envelope.content_topic,
                envelope.timestamp_ns,
                envelope.message.len()
            );
            
            // Send the envelope to the worker pool via the channel
            if let Err(e) = message_tx.send_async(envelope).await {
                error!("Failed to send message to workers: {}", e);
                // Channel is closed, exit
                return Err("Message channel closed".into());
            }
        }
        
        info!("Stream ended after {} messages", message_count);
        Ok(())
    }
}

/// Message processor that handles individual messages
pub struct MessageProcessor {
    worker_id: usize,
}

impl MessageProcessor {
    pub fn new(worker_id: usize) -> Self {
        Self { worker_id }
    }
    
    /// Process a single envelope
    pub async fn process_envelope(&self, envelope: Envelope) {
        // For now, just log the message
        info!(
            "Worker {} processing message - Topic: {}, Timestamp: {}, Message size: {} bytes",
            self.worker_id,
            envelope.content_topic,
            envelope.timestamp_ns,
            envelope.message.len()
        );
        
        // In the future, this is where we would:
        // 1. Decode the message
        // 2. Determine if notification is needed
        // 3. Queue notification for delivery
    }
}

/// Worker task that processes messages from the channel
pub async fn message_worker(
    worker_id: usize,
    receiver: flume::Receiver<Envelope>,
    shutdown_token: CancellationToken,
) {
    info!("Message worker {} started", worker_id);
    let processor = MessageProcessor::new(worker_id);
    
    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                info!("Message worker {} received shutdown signal", worker_id);
                break;
            }
            result = receiver.recv_async() => {
                match result {
                    Ok(envelope) => processor.process_envelope(envelope).await,
                    Err(flume::RecvError::Disconnected) => {
                        // Channel closed
                        info!("Message channel closed for worker {}", worker_id);
                        break;
                    }
                }
            }
        }
    }
    
    info!("Message worker {} stopped", worker_id);
}