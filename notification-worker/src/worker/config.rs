use crate::types::environment::Environment;

/// Configuration for the XMTP worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// XMTP node endpoint
    pub xmtp_endpoint: String,
    /// Whether to use TLS for the connection
    pub use_tls: bool,
    /// Client version to send in metadata
    pub client_version: String,
    /// Number of worker tasks to spawn
    pub num_workers: usize,
    /// Initial reconnection delay in milliseconds
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection delay in milliseconds
    pub max_reconnect_delay_ms: u64,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Connect timeout in milliseconds
    pub connect_timeout_ms: u64,
}

impl WorkerConfig {
    /// Creates a new WorkerConfig from the given environment
    pub fn from_environment(env: &Environment) -> Self {
        Self {
            xmtp_endpoint: env.xmtp_grpc_address(),
            use_tls: env.use_tls_override(),
            client_version: "notification-worker-rust/0.1.0".to_string(),
            num_workers: env.default_num_workers(),
            reconnect_delay_ms: env.reconnect_delay_ms(),
            max_reconnect_delay_ms: env.max_reconnect_delay_ms(),
            connection_timeout_ms: env.connection_timeout_ms(),
            connect_timeout_ms: env.connect_timeout_ms(),
        }
    }

    // /// Creates a WorkerConfig with custom settings
    // pub fn new(xmtp_endpoint: String, num_workers: usize) -> Self {
    //     // Determine TLS based on endpoint
    //     let use_tls = xmtp_endpoint.starts_with("https://");

    //     Self {
    //         xmtp_endpoint,
    //         use_tls,
    //         client_version: "notification-worker-rust/0.1.0".to_string(),
    //         num_workers,
    //         reconnect_delay_ms: 100,
    //         max_reconnect_delay_ms: 30000,
    //         connection_timeout_ms: 30000,
    //         connect_timeout_ms: 5000,
    //     }
    // }

    /// Returns the channel capacity (2 * num_workers)
    pub fn channel_capacity(&self) -> usize {
        self.num_workers * 2
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let env = Environment::from_env();
        Self::from_environment(&env)
    }
}
