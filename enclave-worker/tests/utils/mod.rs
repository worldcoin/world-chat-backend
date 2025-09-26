use anyhow::Result;
use enclave_worker::{cache::CacheManager, redis::RedisClient, types::Environment};

/// Setup test environment variables with all the required configuration
fn setup_test_env() {
    // Load test environment variables if exists, otherwise use defaults
    dotenvy::from_path(".env.test").ok();

    // Set default test environment if not set
    std::env::set_var("APP_ENV", "development");

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();
}

pub struct TestContext {
    pub cache_manager: CacheManager,
    pub redis_client: RedisClient,
}

impl TestContext {
    /// Create a new test context with Redis connection
    pub async fn new() -> Result<Self> {
        setup_test_env();

        let environment = Environment::Development;

        // Initialize Redis client and Cache Manager
        let redis_client = RedisClient::new(&environment.redis_url()).await?;
        let cache_manager = CacheManager::new(redis_client.clone());

        Ok(Self {
            cache_manager,
            redis_client,
        })
    }
}
