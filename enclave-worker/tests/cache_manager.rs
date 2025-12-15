mod utils;

use anyhow::Result;
use pretty_assertions::assert_eq;
use redis::AsyncCommands;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_cache_miss_fetches_and_stores_value() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_cache_miss_{}", uuid::Uuid::new_v4());
    let expected_value = b"fresh_data".to_vec();

    // Call cache_with_refresh when no value exists
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, || async { Ok(b"fresh_data".to_vec()) })
        .await?;

    // Verify correct value returned
    assert_eq!(result, expected_value);

    // Verify value was stored in Redis
    let mut conn = ctx.redis_client.conn();
    let cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(cached, Some(expected_value));

    // Verify TTL was set (should be around 60 seconds)
    let ttl: i64 = conn.ttl(&cache_key).await?;
    assert!(
        ttl > 50 && ttl <= 60,
        "TTL should be around 60s, got {}",
        ttl
    );

    Ok(())
}

#[tokio::test]
async fn test_cache_hit_returns_cached_value() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_cache_hit_{}", uuid::Uuid::new_v4());
    let cached_value = b"cached_data".to_vec();

    // Pre-populate Redis with value
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, cached_value.as_slice(), 60)
        .await?;

    // Call cache_with_refresh - fetch function should NOT be called
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, || async {
            // This should never be called
            panic!("Fetch function should not be called when cache hit!");
        })
        .await?;

    // Verify cached value is returned
    assert_eq!(result, cached_value);

    // Verify value unchanged in Redis
    let still_cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(still_cached, Some(cached_value));

    Ok(())
}

#[tokio::test]
async fn test_fetch_error_propagates_on_cache_miss() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_fetch_error_{}", uuid::Uuid::new_v4());

    // Call with a fetch function that returns an error
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, || async {
            Err(anyhow::anyhow!("Simulated fetch error"))
        })
        .await;

    // Error should be propagated
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Simulated fetch error"));

    // Verify nothing was cached
    let mut conn = ctx.redis_client.conn();
    let cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(cached, None);

    Ok(())
}

#[tokio::test]
async fn test_expired_cache_fetches_fresh() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_expired_{}", uuid::Uuid::new_v4());

    // Pre-populate with a value that has very low TTL
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, b"old", 1).await?;

    // Wait for it to expire
    sleep(Duration::from_secs(2)).await;

    // Call should fetch fresh since cache expired
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, || async {
            Ok(b"fresh_after_expiry".to_vec())
        })
        .await?;

    assert_eq!(result, b"fresh_after_expiry");

    Ok(())
}

#[tokio::test]
async fn test_set_with_ttl_safely_stores_value() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_set_safely_{}", uuid::Uuid::new_v4());
    let value = b"test_value".to_vec();

    // Use set_with_ttl_safely to store value
    ctx.cache_manager
        .set_with_ttl_safely(&cache_key, &value, 60)
        .await;

    // Verify value was stored in Redis
    let mut conn = ctx.redis_client.conn();
    let cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(cached, Some(value));

    // Verify TTL was set
    let ttl: i64 = conn.ttl(&cache_key).await?;
    assert!(
        ttl > 50 && ttl <= 60,
        "TTL should be around 60s, got {}",
        ttl
    );

    Ok(())
}

#[tokio::test]
async fn test_set_with_ttl_safely_overwrites_existing() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_overwrite_{}", uuid::Uuid::new_v4());

    // Pre-populate with old value
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, b"old_value", 30)
        .await?;

    // Overwrite with new value using set_with_ttl_safely
    let new_value = b"new_value".to_vec();
    ctx.cache_manager
        .set_with_ttl_safely(&cache_key, &new_value, 60)
        .await;

    // Verify new value was stored
    let cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(cached, Some(new_value));

    // Verify TTL was reset to new value
    let ttl: i64 = conn.ttl(&cache_key).await?;
    assert!(
        ttl > 50 && ttl <= 60,
        "TTL should be reset to ~60s, got {}",
        ttl
    );

    Ok(())
}
