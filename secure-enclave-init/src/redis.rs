use anyhow::Result;
use redis::{
    aio::ConnectionManager, AsyncTypedCommands, Client, ExistenceCheck, SetExpiry, SetOptions,
};
use std::time::Duration;
use tracing::{info, warn};

const LOCK_TTL_SECS: u64 = 60; // 1 minute for key generation
const REDIS_TIMEOUT: Duration = Duration::from_secs(600); // 10 minutes

#[derive(Clone)]
pub struct RedisKeyManager {
    connection_manager: ConnectionManager,
    track: String,
}

/// Key Manager powered by Redis
///
/// This key manager is used to coordinate key generation between enclaves.
/// When a new enclave track is created, it will will acquire a lock in Redis to generate the key.
/// Subsequent enclaves will check if the lock is acquired and if not, they will wait for the lock to be released.
impl RedisKeyManager {
    /// Create a new Redis key manager with connection manager
    pub async fn new(redis_url: &str, track: &str) -> Result<Self> {
        let client = Client::open(redis_url)?;
        let connection_manager = ConnectionManager::new(client).await?;

        Ok(Self {
            connection_manager,
            track: track.to_string(),
        })
    }

    /// Check if we should generate a key for this track
    /// Returns true if we successfully acquired the lock (key generation needed)
    pub async fn should_generate_key(&self) -> Result<bool> {
        let key = format!("enclave-key:{}", self.track);
        let mut conn = self.connection_manager.clone();

        // Check current state
        let current_state: Option<String> =
            tokio::time::timeout(REDIS_TIMEOUT, conn.get(&key)).await??;

        match current_state.as_deref() {
            None => {
                // Key doesn't exist, try to acquire lock
                info!(
                    "No key exists for track {}, attempting to acquire lock",
                    self.track
                );
                self.acquire_generation_lock().await
            }
            Some("in-progress") => {
                info!(
                    "Key generation already in progress for track {}",
                    self.track
                );
                Ok(false)
            }
            Some("loaded") => {
                info!("Key already loaded for track {}", self.track);
                Ok(false)
            }
            Some(state) => {
                warn!("Unknown key state '{}' for track {}", state, self.track);
                Ok(false)
            }
        }
    }

    /// Try to acquire the lock for key generation
    async fn acquire_generation_lock(&self) -> Result<bool> {
        let key = format!("enclave-key:{}", self.track);
        let mut conn = self.connection_manager.clone();

        // Try to set "in-progress" only if key doesn't exist (NX)
        let result: Option<String> = tokio::time::timeout(
            REDIS_TIMEOUT,
            conn.set_options(
                &key,
                "in-progress",
                SetOptions::default()
                    .conditional_set(ExistenceCheck::NX)
                    .with_expiration(SetExpiry::EX(LOCK_TTL_SECS)),
            ),
        )
        .await??;

        let acquired = result.is_some();
        if acquired {
            info!(
                "Successfully acquired key generation lock for track {}",
                self.track
            );
        } else {
            info!(
                "Failed to acquire lock - another enclave is generating key for track {}",
                self.track
            );
        }

        Ok(acquired)
    }

    /// Mark key as successfully loaded
    pub async fn mark_key_loaded(&self) -> Result<()> {
        let key = format!("enclave-key:{}", self.track);
        let mut conn = self.connection_manager.clone();

        // Set to "loaded" without expiration (permanent)
        tokio::time::timeout(REDIS_TIMEOUT, conn.set(&key, "loaded")).await??;

        info!("Marked key as loaded for track {}", self.track);
        Ok(())
    }

    /// Release the lock in case of failure
    pub async fn release_lock(&self) -> Result<()> {
        let key = format!("enclave-key:{}", self.track);
        let mut conn = self.connection_manager.clone();

        // Delete the key to allow another enclave to try
        tokio::time::timeout(REDIS_TIMEOUT, conn.del(&key)).await??;

        warn!(
            "Released key generation lock for track {} due to failure",
            self.track
        );
        Ok(())
    }
}
