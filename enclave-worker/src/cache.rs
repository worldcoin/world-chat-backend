use crate::redis::RedisClient;
use redis::AsyncTypedCommands;
use std::time::Duration;
use tokio::time::timeout;

const REDIS_TIMEOUT: Duration = Duration::from_secs(3);
const REFRESH_LOCK_TTL_SECS: u64 = 10;

#[derive(Clone)]
pub struct CacheManager {
    redis_client: RedisClient,
}

impl CacheManager {
    #[must_use]
    pub const fn new(redis_client: RedisClient) -> Self {
        Self { redis_client }
    }

    /// Get cached value with automatic background refresh if about to expire
    ///
    /// # Errors
    /// Returns an error if:
    /// - Redis operations timeout or fail  
    /// - The fetch function returns an error when cache miss occurs
    pub async fn cache_with_refresh<F, Fut>(
        &self,
        cache_key: &str,
        ttl_secs: u64,
        refresh_threshold_secs: u64,
        fetch_fn: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send + 'static,
    {
        let (cached, ttl) = self.get_value_and_ttl(cache_key).await?;

        // Miss: fetch now and store.
        if cached.is_none() {
            let fresh = fetch_fn().await?;
            self.set_with_ttl(cache_key, &fresh, ttl_secs).await?;
            return Ok(fresh);
        }

        // Hit: maybe refresh in the background; always return the cached value.
        let data = cached.ok_or_else(|| anyhow::anyhow!("Cached data is none when cache hit"))?;
        if ttl > 0 && u64::try_from(ttl).unwrap_or(0) <= refresh_threshold_secs {
            self.spawn_background_refresh(cache_key.to_owned(), ttl_secs, fetch_fn)
                .await;
        }
        Ok(data)
    }

    /// Spawn a background refresh task for a cache key. Task will ONLY be spawned if lock is acquired.
    async fn spawn_background_refresh<F, Fut>(&self, cache_key: String, ttl_secs: u64, fetch_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send + 'static,
    {
        let lock_key = format!("{cache_key}_refresh_lock");
        if !self.try_acquire_lock(&lock_key).await {
            return;
        }

        // We acquired the lock, so we can spawn the task.
        let cm = self.clone();
        tokio::spawn(async move {
            tracing::info!("Background refresh starting for key: {}", cache_key);
            match fetch_fn().await {
                Ok(fresh) => match cm.set_with_ttl(&cache_key, &fresh, ttl_secs).await {
                    Ok(()) => tracing::info!("Successfully refreshed key: {}", cache_key),
                    Err(e) => tracing::warn!("Failed to update cache: {e}"),
                },
                Err(e) => tracing::warn!("Refresh failed for {cache_key}: {e}"),
            }

            cm.release_lock(&lock_key).await;
        });
    }

    // --------------------------
    // Redis Operation helpers
    // --------------------------

    async fn get_value_and_ttl(&self, key: &str) -> anyhow::Result<(Option<Vec<u8>>, i64)> {
        let mut conn = self.redis_client.conn();
        timeout(
            REDIS_TIMEOUT,
            redis::pipe().get(key).ttl(key).query_async(&mut conn),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Redis timeout"))?
        .map_err(|e| anyhow::anyhow!("Redis error: {e}"))
    }

    async fn set_with_ttl(&self, key: &str, data: &[u8], ttl_secs: u64) -> anyhow::Result<()> {
        let mut conn = self.redis_client.conn();
        timeout(REDIS_TIMEOUT, conn.set_ex(key, data, ttl_secs))
            .await
            .map_err(|_| anyhow::anyhow!("Redis timeout"))?
            .map_err(|e| anyhow::anyhow!("Redis error: {e}"))?;
        Ok(())
    }

    /// Try to acquire a lock for a given key. If the lock is already acquired or an error occurs, we return false.
    async fn try_acquire_lock(&self, lock_key: &str) -> bool {
        let mut conn = self.redis_client.conn();

        let lock_value = timeout(
            REDIS_TIMEOUT,
            conn.set_options(
                lock_key,
                "1",
                redis::SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(redis::SetExpiry::EX(REFRESH_LOCK_TTL_SECS)),
            ),
        )
        .await;

        // If we manage to set the lock value, we return true.
        matches!(lock_value, Ok(Ok(Some(_))))
    }

    async fn release_lock(&self, lock_key: &str) {
        let mut conn = self.redis_client.conn();
        let _ = timeout(REDIS_TIMEOUT, conn.del(lock_key)).await;
    }
}
