use crate::redis::RedisClient;
use redis::AsyncCommands;
use std::time::Duration;
use tokio::time::timeout;

const REDIS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct CacheManager {
    redis_client: RedisClient,
}

impl CacheManager {
    #[must_use]
    pub const fn new(redis_client: RedisClient) -> Self {
        Self { redis_client }
    }

    /// Get cached value or fetch and store it if missing.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Redis operations timeout or fail  
    /// - The fetch function returns an error when cache miss occurs
    pub async fn cache_with_refresh<F, Fut>(
        &self,
        cache_key: &str,
        ttl_secs: u64,
        fetch_fn: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send + 'static,
    {
        // Miss: fetch now and store.
        let cached = self.get(cache_key).await?;
        if cached.is_none() {
            let fresh = fetch_fn().await?;
            self.set_with_ttl(cache_key, &fresh, ttl_secs).await?;
            return Ok(fresh);
        }

        // Hit: maybe refresh in the background; always return the cached value.
        let data = cached.ok_or_else(|| anyhow::anyhow!("Cached data is none when cache hit"))?;
        Ok(data)
    }

    pub async fn set_with_ttl_safely(&self, key: &str, data: &[u8], ttl_secs: u64) -> () {
        if let Err(e) = self.set_with_ttl(key, data, ttl_secs).await {
            tracing::error!("Failed to set cache key {key}: {e:?}");
        }
    }

    // --------------------------
    // Redis Operation helpers
    // --------------------------

    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let mut conn = self.redis_client.conn();
        timeout(REDIS_TIMEOUT, conn.get(key))
            .await
            .map_err(|_| anyhow::anyhow!("Redis timeout"))?
            .map_err(|e| anyhow::anyhow!("Redis error: {e}"))
    }

    async fn set_with_ttl(&self, key: &str, data: &[u8], ttl_secs: u64) -> anyhow::Result<()> {
        let mut conn = self.redis_client.conn();
        timeout(REDIS_TIMEOUT, conn.set_ex::<_, _, ()>(key, data, ttl_secs))
            .await
            .map_err(|_| anyhow::anyhow!("Redis timeout"))?
            .map_err(|e| anyhow::anyhow!("Redis error: {e}"))?;
        Ok(())
    }
}
