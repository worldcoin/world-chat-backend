use redis::{aio::ConnectionManager, Client};

#[derive(Clone)]
pub struct RedisClient {
    connection_manager: ConnectionManager,
}

impl RedisClient {
    /// Create a new Redis client with connection manager
    ///
    /// # Errors
    /// Returns an error if:
    /// - The Redis URL is invalid
    /// - Connection to Redis server fails
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let client = Client::open(url)?;
        let connection_manager = ConnectionManager::new(client).await?;

        Ok(Self { connection_manager })
    }

    /// Get a clone of the connection manager
    #[must_use]
    pub fn conn(&self) -> ConnectionManager {
        self.connection_manager.clone()
    }
}
