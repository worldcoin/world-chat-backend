use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_sdk_dynamodb::types::{
    AttributeDefinition, GlobalSecondaryIndex, KeySchemaElement, KeyType,
    Projection, ProjectionType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use chrono::{TimeZone, Utc};
use push_notification_storage::{
    PushNotificationStorage, PushNotificationStorageError, PushSubscription,
};
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Creates a test setup with a unique table
async fn setup_test() -> (PushNotificationStorage, String) {
    // Create unique table name
    let table_name = format!("test-push-subscriptions-{}", Uuid::new_v4());
    let gsi_name = "topic-index";

    // Configure AWS SDK for LocalStack
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(LOCALSTACK_ENDPOINT)
        .region(Region::new(TEST_REGION))
        .load()
        .await;

    let dynamodb_client = Arc::new(DynamoDbClient::new(&config));

    // Create table
    dynamodb_client
        .create_table()
        .table_name(&table_name)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("hmac")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("topic")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("hmac")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .global_secondary_indexes(
            GlobalSecondaryIndex::builder()
                .index_name(gsi_name)
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("topic")
                        .key_type(KeyType::Hash)
                        .build()
                        .unwrap(),
                )
                .projection(
                    Projection::builder()
                        .projection_type(ProjectionType::All)
                        .build(),
                )
                .build()
                .unwrap(),
        )
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::PayPerRequest)
        .send()
        .await
        .expect("Failed to create test table");

    // Enable TTL
    dynamodb_client
        .update_time_to_live()
        .table_name(&table_name)
        .time_to_live_specification(
            aws_sdk_dynamodb::types::TimeToLiveSpecification::builder()
                .enabled(true)
                .attribute_name("ttl")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("Failed to enable TTL");

    // Wait a bit for table to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    let storage = PushNotificationStorage::new(
        dynamodb_client,
        table_name.clone(),
        gsi_name.to_string(),
    );

    (storage, table_name)
}

/// Creates a test subscription with unique HMAC
fn create_test_subscription(topic: &str) -> PushSubscription {
    PushSubscription {
        hmac: format!("test-hmac-{}", Uuid::new_v4()),
        topic: topic.to_string(),
        ttl: (Utc::now() + chrono::Duration::hours(24)).timestamp(),
        encrypted_braze_id: format!("encrypted-{}", Uuid::new_v4()),
    }
}

#[tokio::test]
async fn test_basic_crud_operations() {
    let (storage, _table) = setup_test().await;

    // Create subscription
    let subscription = create_test_subscription("test-topic");

    // Insert
    storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");

    // Check exists
    let exists = storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(exists);

    // Get by topic
    let subscriptions = storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get by topic");
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].hmac, subscription.hmac);
    assert_eq!(subscriptions[0].topic, subscription.topic);
    assert_eq!(subscriptions[0].encrypted_braze_id, subscription.encrypted_braze_id);

    // Delete
    storage
        .delete_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to delete subscription");

    // Check doesn't exist
    let exists = storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence after delete");
    assert!(!exists);
}

#[tokio::test]
async fn test_ttl_rounding() {
    let (storage, _table) = setup_test().await;

    // Test rounding down (< 30 seconds)
    let mut subscription = create_test_subscription("test-topic");
    let base_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 30, 20).unwrap();
    subscription.ttl = base_time.timestamp();

    storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");

    let retrieved = storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get by topic");
    
    // Should round down to 12:30:00
    let expected_ttl = Utc.with_ymd_and_hms(2024, 1, 1, 12, 30, 0).unwrap().timestamp();
    assert_eq!(retrieved[0].ttl, expected_ttl);

    // Test rounding up (>= 30 seconds)
    let mut subscription2 = create_test_subscription("test-topic-2");
    let base_time2 = Utc.with_ymd_and_hms(2024, 1, 1, 12, 30, 45).unwrap();
    subscription2.ttl = base_time2.timestamp();

    storage
        .insert(&subscription2)
        .await
        .expect("Failed to insert subscription");

    let retrieved2 = storage
        .get_all_by_topic(&subscription2.topic)
        .await
        .expect("Failed to get by topic");
    
    // Should round up to 12:31:00
    let expected_ttl2 = Utc.with_ymd_and_hms(2024, 1, 1, 12, 31, 0).unwrap().timestamp();
    assert_eq!(retrieved2[0].ttl, expected_ttl2);
}

#[tokio::test]
async fn test_duplicate_prevention() {
    let (storage, _table) = setup_test().await;

    let subscription = create_test_subscription("test-topic");

    // First insert should succeed
    storage
        .insert(&subscription)
        .await
        .expect("First insert should succeed");

    // Second insert with same HMAC should fail
    let result = storage.insert(&subscription).await;
    assert!(matches!(
        result,
        Err(PushNotificationStorageError::PushSubscriptionExists)
    ));

    // Insert with different HMAC but same topic should succeed
    let mut subscription2 = subscription.clone();
    subscription2.hmac = format!("different-hmac-{}", Uuid::new_v4());
    
    storage
        .insert(&subscription2)
        .await
        .expect("Insert with different HMAC should succeed");
}

#[tokio::test]
async fn test_query_by_topic() {
    let (storage, _table) = setup_test().await;

    let topic = "shared-topic";
    let other_topic = "other-topic";

    // Insert multiple subscriptions with same topic
    let mut subscriptions = Vec::new();
    for _ in 0..3 {
        let sub = create_test_subscription(topic);
        storage.insert(&sub).await.expect("Failed to insert");
        subscriptions.push(sub);
    }

    // Insert one with different topic
    let other_sub = create_test_subscription(other_topic);
    storage.insert(&other_sub).await.expect("Failed to insert");

    // Query by shared topic
    let retrieved = storage
        .get_all_by_topic(topic)
        .await
        .expect("Failed to query by topic");
    
    assert_eq!(retrieved.len(), 3);
    
    // Verify all retrieved subscriptions have the correct topic
    for sub in &retrieved {
        assert_eq!(sub.topic, topic);
    }

    // Query by other topic
    let other_retrieved = storage
        .get_all_by_topic(other_topic)
        .await
        .expect("Failed to query by other topic");
    
    assert_eq!(other_retrieved.len(), 1);
    assert_eq!(other_retrieved[0].hmac, other_sub.hmac);

    // Query non-existent topic
    let empty = storage
        .get_all_by_topic("non-existent")
        .await
        .expect("Failed to query non-existent topic");
    
    assert_eq!(empty.len(), 0);
}

#[tokio::test]
async fn test_delete_non_existent() {
    let (storage, _table) = setup_test().await;

    // Deleting non-existent item should not error
    storage
        .delete_by_hmac("non-existent-hmac")
        .await
        .expect("Delete non-existent should not error");
}

#[tokio::test]
async fn test_exists_by_hmac() {
    let (storage, _table) = setup_test().await;

    let subscription = create_test_subscription("test-topic");

    // Should not exist before insert
    let exists_before = storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(!exists_before);

    // Insert
    storage
        .insert(&subscription)
        .await
        .expect("Failed to insert");

    // Should exist after insert
    let exists_after = storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(exists_after);
}