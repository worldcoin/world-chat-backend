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
        .cache_with_refresh(&cache_key, 60, 30, || async { Ok(b"fresh_data".to_vec()) })
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
async fn test_cache_hit_above_threshold_no_refresh() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_above_threshold_{}", uuid::Uuid::new_v4());
    let cached_value = b"cached_data".to_vec();

    // Pre-populate Redis with value that has high TTL (60s, above 30s threshold)
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, cached_value.as_slice(), 60)
        .await?;

    // Call cache_with_refresh - fetch function should NOT be called
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, 30, || async {
            // This should never be called
            panic!("Fetch function should not be called when cache is fresh!");
        })
        .await?;

    // Verify cached value is returned
    assert_eq!(result, cached_value);

    // Wait briefly to ensure no background refresh
    sleep(Duration::from_millis(100)).await;

    // Verify value unchanged in Redis
    let still_cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(still_cached, Some(cached_value));

    Ok(())
}

#[tokio::test]
async fn test_cache_hit_within_threshold_triggers_refresh() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_within_threshold_{}", uuid::Uuid::new_v4());
    let old_value = b"old_data".to_vec();
    let new_value = b"refreshed_data".to_vec();

    // Pre-populate with value that has TTL below threshold (20s < 30s threshold)
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, old_value.as_slice(), 20)
        .await?;

    // Call cache_with_refresh - should return old value and trigger background refresh
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, 30, || async {
            // Add small delay to simulate work
            sleep(Duration::from_millis(50)).await;
            Ok(b"refreshed_data".to_vec())
        })
        .await?;

    // Should immediately return the old cached value
    assert_eq!(result, old_value);

    // Wait for background refresh to complete
    sleep(Duration::from_millis(200)).await;

    // Verify cache was updated with new value
    let updated: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(updated, Some(new_value));

    // Verify TTL was reset to full duration
    let ttl: i64 = conn.ttl(&cache_key).await?;
    assert!(
        ttl > 50 && ttl <= 60,
        "TTL should be reset to ~60s, got {}",
        ttl
    );

    Ok(())
}

#[tokio::test]
async fn test_fetch_error_propagates_on_cache_miss() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_fetch_error_{}", uuid::Uuid::new_v4());

    // Call with a fetch function that returns an error
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, 30, || async {
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
async fn test_stale_value_returned_when_refresh_fails() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_refresh_failure_{}", uuid::Uuid::new_v4());
    let stale_value = b"stale_but_available".to_vec();

    // Pre-populate with value that needs refresh
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, stale_value.as_slice(), 20)
        .await?;

    // Call with fetch that fails - should still return stale value
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, 30, || async {
            Err(anyhow::anyhow!("Refresh failed"))
        })
        .await?;

    // Should still return the stale value (graceful degradation)
    assert_eq!(result, stale_value);

    // Wait briefly for background refresh attempt
    sleep(Duration::from_millis(100)).await;

    // Verify stale value is still in cache (not deleted on refresh failure)
    let still_cached: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(still_cached, Some(stale_value));

    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests_with_refresh() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_concurrent_{}", uuid::Uuid::new_v4());
    let old_value = b"old".to_vec();
    let new_value = b"new".to_vec();

    // Pre-populate with value needing refresh
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, old_value.as_slice(), 20)
        .await?;

    // Launch multiple concurrent requests
    let mut handles = vec![];
    for _ in 0..3 {
        let cache_manager = ctx.cache_manager.clone();
        let key = cache_key.clone();
        let handle = tokio::spawn(async move {
            cache_manager
                .cache_with_refresh(&key, 60, 30, || async {
                    sleep(Duration::from_millis(100)).await;
                    Ok(b"new".to_vec())
                })
                .await
        });
        handles.push(handle);
    }

    // All concurrent requests should succeed
    for handle in handles {
        let result = handle.await??;
        // All should get the old value immediately
        assert_eq!(result, old_value);
    }

    // Wait for background refresh
    sleep(Duration::from_millis(300)).await;

    // Verify cache was updated (only once despite multiple requests)
    let updated: Option<Vec<u8>> = conn.get(&cache_key).await?;
    assert_eq!(updated, Some(new_value));

    Ok(())
}

#[tokio::test]
async fn test_zero_ttl_always_fetches_fresh() -> Result<()> {
    let ctx = utils::TestContext::new().await?;
    let cache_key = format!("test_zero_ttl_{}", uuid::Uuid::new_v4());

    // Pre-populate with a value that has very low TTL
    let mut conn = ctx.redis_client.conn();
    conn.set_ex::<_, _, ()>(&cache_key, b"old", 1).await?;

    // Wait for it to expire
    sleep(Duration::from_secs(2)).await;

    // Call should fetch fresh since TTL is 0 (expired)
    let result = ctx
        .cache_manager
        .cache_with_refresh(&cache_key, 60, 30, || async {
            Ok(b"fresh_after_expiry".to_vec())
        })
        .await?;

    assert_eq!(result, b"fresh_after_expiry");

    Ok(())
}
