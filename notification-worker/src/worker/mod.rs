pub mod message_processor;
pub mod xmtp_listener;

use std::sync::Arc;
use std::time::Duration;

use crate::types::environment::Environment;
use crate::worker::xmtp_listener::XmtpListenerConfig;
use crate::xmtp::message_api::v1::Envelope;

/// Result type for worker operations
pub type WorkerResult<T> = anyhow::Result<T>;

use backend_storage::push_subscription::PushSubscriptionStorage;
use backend_storage::queue::NotificationQueue;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, ClientTlsConfig};
use tracing::{error, info, instrument};

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

use self::message_processor::MessageProcessor;
use self::xmtp_listener::XmtpListener;

/// XMTP worker that manages message streaming and processing
pub struct XmtpWorker {
    env: Environment,
    client: MessageApiClient<Channel>,
    shutdown_token: CancellationToken,
    notification_queue: Arc<NotificationQueue>,
    subscription_storage: Arc<PushSubscriptionStorage>,
}

impl XmtpWorker {
    /// Creates a new XMTP worker (legacy API)
    ///
    /// # Errors
    ///
    /// Returns an error if connection to XMTP fails or TLS configuration is invalid.
    pub async fn new(
        env: Environment,
        notification_queue: Arc<NotificationQueue>,
        subscription_storage: Arc<PushSubscriptionStorage>,
    ) -> anyhow::Result<Self> {
        info!(
            "Connecting to XMTP node at {}, TLS enabled: {}",
            env.xmtp_endpoint(),
            env.use_tls()
        );

        // Create the endpoint with proper configuration
        let endpoint = {
            let mut ep = Channel::from_shared(env.xmtp_endpoint())?;

            if env.use_tls() {
                let tls_config = ClientTlsConfig::new().with_webpki_roots();
                ep = ep.tls_config(tls_config)?;
            }

            ep.timeout(Duration::from_millis(env.request_timeout_ms()))
                .connect_timeout(Duration::from_millis(env.connection_timeout_ms()))
        };
        let channel = endpoint.connect().await?;
        let client = MessageApiClient::new(channel);

        Ok(Self {
            env,
            client,
            shutdown_token: CancellationToken::new(),
            notification_queue,
            subscription_storage,
        })
    }

    /// Returns a clone of the shutdown token for external control
    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Starts the worker and all components
    ///
    /// # Errors
    ///
    /// Returns an error if stream listening fails or processor tasks panic.
    pub async fn start(self) -> anyhow::Result<()> {
        info!(
            "Starting XMTP worker with {} processors",
            self.env.num_workers()
        );

        let (message_tx, message_rx) = self.create_message_channel();
        let processor_handles = self.spawn_processors(&message_rx);

        self.run_xmtp_listener(message_tx).await;
        self.shutdown_and_cleanup(processor_handles).await;

        Ok(())
    }

    /// Creates and logs the message channel
    fn create_message_channel(&self) -> (flume::Sender<Envelope>, flume::Receiver<Envelope>) {
        let (message_tx, message_rx) = flume::bounded::<Envelope>(self.env.channel_capacity());
        info!(
            "Created flume channel with capacity: {}",
            self.env.channel_capacity()
        );
        (message_tx, message_rx)
    }

    /// Runs the XMTP listener and handles results
    async fn run_xmtp_listener(&self, message_tx: flume::Sender<Envelope>) {
        let listener_result = XmtpListener::new(
            self.client.clone(),
            message_tx,
            self.shutdown_token.clone(),
            XmtpListenerConfig {
                reconnect_delay_ms: self.env.reconnect_delay_ms(),
                max_reconnect_delay_ms: self.env.max_reconnect_delay_ms(),
            },
        )
        .run()
        .await;

        if let Err(e) = listener_result {
            error!("XMTP listener error: {}", e);
        }
    }

    /// Shuts down and cleans up all worker components
    async fn shutdown_and_cleanup(&self, processor_handles: Vec<JoinHandle<()>>) {
        self.shutdown_token.cancel();
        info!("XMTP worker shutdown initiated");

        for handle in processor_handles {
            if let Err(e) = handle.await {
                error!("Processor task error: {}", e);
            }
        }
        info!("All XMTP worker components stopped");
    }

    /// Spawns message processor tasks
    fn spawn_processors(&self, receiver: &flume::Receiver<Envelope>) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for i in 0..self.env.num_workers() {
            let processor = MessageProcessor::new(
                i,
                Arc::clone(&self.notification_queue),
                Arc::clone(&self.subscription_storage),
            );
            let rx = receiver.clone();
            let shutdown_token = self.shutdown_token.clone();

            let handle = tokio::spawn(async move {
                processor.run(rx, shutdown_token).await;
            });

            handles.push(handle);
        }

        handles
    }
}
