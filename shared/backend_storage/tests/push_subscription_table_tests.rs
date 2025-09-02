use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::push_subscription::{
    PushSubscription, PushSubscriptionAttribute, PushSubscriptionStorage,
};
use chrono::Utc;
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Test context that automatically cleans up the table on drop
struct TestContext {
    storage: PushSubscriptionStorage,
    table_name: String,
    dynamodb_client: Arc<DynamoDbClient>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Clean up the table
        let client = self.dynamodb_client.clone();
        let table = self.table_name.clone();

        // Use tokio runtime to delete table
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = client.delete_table().table_name(&table).send().await;
            });
        }
    }
}

/// Creates a test setup with a unique table
async fn setup_test() -> TestContext {
    // Create unique table name
    let table_name = format!("test-push-subscriptions-{}", Uuid::new_v4());

    // Configure AWS SDK for LocalStack
    let credentials = Credentials::from_keys(
        "test", // AWS_ACCESS_KEY_ID
        "test", // AWS_SECRET_ACCESS_KEY
        None,   // no session token
    );
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(LOCALSTACK_ENDPOINT)
        .region(Region::new(TEST_REGION))
        .credentials_provider(credentials)
        .load()
        .await;

    let dynamodb_client = Arc::new(DynamoDbClient::new(&config));

    // Create a table with topic (PK) + hmac_key (SK)
    dynamodb_client
        .create_table()
        .table_name(&table_name)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(PushSubscriptionAttribute::Topic.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(PushSubscriptionAttribute::HmacKey.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name(PushSubscriptionAttribute::Topic.to_string())
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name(PushSubscriptionAttribute::HmacKey.to_string())
                .key_type(KeyType::Range)
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
                .attribute_name(PushSubscriptionAttribute::Ttl.to_string())
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("Failed to enable TTL");

    // Wait a bit for table to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    let storage = PushSubscriptionStorage::new(dynamodb_client.clone(), table_name.clone());

    TestContext {
        storage,
        table_name,
        dynamodb_client,
    }
}

/// Creates a test subscription with unique HMAC key
fn create_test_subscription(topic: &str) -> PushSubscription {
    PushSubscription {
        topic: topic.to_string(),
        hmac_key: format!("test-hmac-{}", Uuid::new_v4()),
        ttl: (Utc::now() + chrono::Duration::hours(24)).timestamp(),
        encrypted_push_id: format!("encrypted-{}", Uuid::new_v4()),
        deletion_request: None,
    }
}

/// Creates a test subscription with deletion request
fn create_test_subscription_with_deletion(
    topic: &str,
    deletion_requests: Vec<String>,
) -> PushSubscription {
    let mut deletion_set = HashSet::new();
    for req in deletion_requests {
        deletion_set.insert(req);
    }

    PushSubscription {
        topic: topic.to_string(),
        hmac_key: format!("test-hmac-{}", Uuid::new_v4()),
        ttl: (Utc::now() + chrono::Duration::hours(24)).timestamp(),
        encrypted_push_id: format!("encrypted-{}", Uuid::new_v4()),
        deletion_request: Some(deletion_set),
    }
}

#[tokio::test]
async fn test_basic_insert_and_get_operations() {
    let context = setup_test().await;

    // Create subscription
    let subscription = create_test_subscription("test-topic");

    // Insert
    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");

    // Get by topic and hmac
    let retrieved = context
        .storage
        .get_one(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to get by topic and hmac");

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.topic, subscription.topic);
    assert_eq!(retrieved.hmac_key, subscription.hmac_key);
    assert_eq!(retrieved.encrypted_push_id, subscription.encrypted_push_id);
    assert_eq!(retrieved.deletion_request, subscription.deletion_request);

    // Get by topic
    let subscriptions = context
        .storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get by topic");
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].topic, subscription.topic);
    assert_eq!(subscriptions[0].hmac_key, subscription.hmac_key);
}

#[tokio::test]
async fn test_insert_duplicate_prevention() {
    let context = setup_test().await;

    let subscription = create_test_subscription("test-topic");

    // First insert should succeed
    context
        .storage
        .insert(&subscription)
        .await
        .expect("First insert should succeed");

    // Second insert with same topic and hmac_key should fail (regardless of encrypted_push_id)
    let result = context.storage.insert(&subscription).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        backend_storage::push_subscription::PushSubscriptionStorageError::PushSubscriptionExists => {
            // Expected error
        }
        other => panic!("Expected PushSubscriptionExists error, got: {:?}", other),
    }

    // Insert with same topic and hmac_key but different encrypted_push_id should also fail
    let mut different_subscription = subscription.clone();
    different_subscription.encrypted_push_id = format!("different-encrypted-{}", Uuid::new_v4());

    let result2 = context.storage.insert(&different_subscription).await;
    assert!(result2.is_err());
    match result2.unwrap_err() {
        backend_storage::push_subscription::PushSubscriptionStorageError::PushSubscriptionExists => {
            // Expected error - same topic+hmac_key combination is not allowed
        }
        other => panic!("Expected PushSubscriptionExists error, got: {:?}", other),
    }

    // Insert with different topic should succeed
    let mut different_topic_subscription = subscription.clone();
    different_topic_subscription.topic = "different-topic".to_string();

    context
        .storage
        .insert(&different_topic_subscription)
        .await
        .expect("Insert with different topic should succeed");

    // Should have one subscription for original topic and one for different topic
    let original_subscriptions = context
        .storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get all by original topic");
    assert_eq!(original_subscriptions.len(), 1);

    let different_subscriptions = context
        .storage
        .get_all_by_topic(&different_topic_subscription.topic)
        .await
        .expect("Failed to get all by different topic");
    assert_eq!(different_subscriptions.len(), 1);
}

#[tokio::test]
async fn test_get_all_by_topic_multiple_subscriptions() {
    let context = setup_test().await;

    let topic = "shared-topic";
    let other_topic = "other-topic";

    // Insert multiple subscriptions with same topic
    let mut subscriptions = Vec::new();
    for i in 0..3 {
        let mut sub = create_test_subscription(topic);
        if i == 1 {
            // Add deletion request to one of them
            sub = create_test_subscription_with_deletion(
                topic,
                vec!["delete1".to_string(), "delete2".to_string()],
            );
        }
        context
            .storage
            .insert(&sub)
            .await
            .expect("Failed to insert");
        subscriptions.push(sub);
    }

    // Insert one with different topic
    let other_sub = create_test_subscription(other_topic);
    context
        .storage
        .insert(&other_sub)
        .await
        .expect("Failed to insert");

    // Query by shared topic
    let retrieved = context
        .storage
        .get_all_by_topic(topic)
        .await
        .expect("Failed to query by topic");

    assert_eq!(retrieved.len(), 3);

    // Verify all retrieved subscriptions have the correct topic
    for sub in &retrieved {
        assert_eq!(sub.topic, topic);
    }

    // Verify one has deletion request
    let with_deletion = retrieved.iter().find(|s| s.deletion_request.is_some());
    assert!(with_deletion.is_some());
    let deletion_requests = with_deletion.unwrap().deletion_request.as_ref().unwrap();
    assert!(deletion_requests.contains("delete1"));
    assert!(deletion_requests.contains("delete2"));

    // Query by other topic
    let other_retrieved = context
        .storage
        .get_all_by_topic(other_topic)
        .await
        .expect("Failed to query by other topic");

    assert_eq!(other_retrieved.len(), 1);
    assert_eq!(other_retrieved[0].hmac_key, other_sub.hmac_key);

    // Query non-existent topic
    let empty = context
        .storage
        .get_all_by_topic("non-existent")
        .await
        .expect("Failed to query non-existent topic");

    assert_eq!(empty.len(), 0);
}

#[tokio::test]
async fn test_get_one_not_found() {
    let context = setup_test().await;

    // Try to get non-existent subscription
    let result = context
        .storage
        .get_one("non-existent-topic", "non-existent-hmac")
        .await
        .expect("Failed to query non-existent subscription");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_deletion_request_serialization() {
    let context = setup_test().await;

    // Create subscription with deletion request
    let subscription = create_test_subscription_with_deletion(
        "test-topic",
        vec!["req1".to_string(), "req2".to_string(), "req3".to_string()],
    );

    // Insert
    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription with deletion request");

    // Retrieve and verify deletion request is preserved
    let retrieved = context
        .storage
        .get_one(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to get subscription")
        .expect("Subscription should exist");

    assert!(retrieved.deletion_request.is_some());
    let deletion_requests = retrieved.deletion_request.unwrap();
    assert_eq!(deletion_requests.len(), 3);
    assert!(deletion_requests.contains("req1"));
    assert!(deletion_requests.contains("req2"));
    assert!(deletion_requests.contains("req3"));
}

#[tokio::test]
async fn test_subscription_without_deletion_request() {
    let context = setup_test().await;

    // Create subscription without deletion request
    let subscription = create_test_subscription("test-topic");

    // Insert
    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription without deletion request");

    // Retrieve and verify deletion request is None
    let retrieved = context
        .storage
        .get_one(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to get subscription")
        .expect("Subscription should exist");

    assert!(retrieved.deletion_request.is_none());
}

#[tokio::test]
async fn test_delete_subscription() {
    let context = setup_test().await;

    // Create and insert subscription
    let subscription = create_test_subscription("test-topic");

    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");

    // Verify it exists
    let retrieved = context
        .storage
        .get_one(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to get subscription");
    assert!(retrieved.is_some());

    // Delete the subscription
    context
        .storage
        .delete(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to delete subscription");

    // Verify it no longer exists
    let retrieved_after_delete = context
        .storage
        .get_one(&subscription.topic, &subscription.hmac_key)
        .await
        .expect("Failed to get subscription after delete");
    assert!(retrieved_after_delete.is_none());

    // Verify topic query returns empty
    let subscriptions = context
        .storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get all by topic after delete");
    assert_eq!(subscriptions.len(), 0);
}

#[tokio::test]
async fn test_delete_nonexistent_subscription() {
    let context = setup_test().await;

    // Delete non-existent subscription should not fail
    context
        .storage
        .delete("non-existent-topic", "non-existent-hmac")
        .await
        .expect("Delete of non-existent subscription should not fail");
}
