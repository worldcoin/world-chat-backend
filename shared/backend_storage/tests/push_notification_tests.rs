use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, GlobalSecondaryIndex, KeySchemaElement, KeyType, Projection,
    ProjectionType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::push_notification::{
    PushNotificationStorage, PushNotificationStorageError, PushSubscription,
    PushSubscriptionAttribute,
};
use chrono::Utc;
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Test context that automatically cleans up the table on drop
struct TestContext {
    storage: PushNotificationStorage,
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
    let gsi_name = "topic-index";

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

    // Create a table to avoid race conditions among tests
    dynamodb_client
        .create_table()
        .table_name(&table_name)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(PushSubscriptionAttribute::Hmac.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(PushSubscriptionAttribute::Topic.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name(PushSubscriptionAttribute::Hmac.to_string())
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .global_secondary_indexes(
            GlobalSecondaryIndex::builder()
                .index_name(gsi_name)
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name(PushSubscriptionAttribute::Topic.to_string())
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
                .attribute_name(PushSubscriptionAttribute::Ttl.to_string())
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("Failed to enable TTL");

    // Wait a bit for table to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    let storage = PushNotificationStorage::new(
        dynamodb_client.clone(),
        table_name.clone(),
        gsi_name.to_string(),
    );

    TestContext {
        storage,
        table_name,
        dynamodb_client,
    }
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
    let context = setup_test().await;

    // Create subscription
    let subscription = create_test_subscription("test-topic");

    // Insert
    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");

    // Check exists
    let exists = context
        .storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(exists);

    // Get by topic
    let subscriptions = context
        .storage
        .get_all_by_topic(&subscription.topic)
        .await
        .expect("Failed to get by topic");
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].hmac, subscription.hmac);
    assert_eq!(subscriptions[0].topic, subscription.topic);
    assert_eq!(
        subscriptions[0].encrypted_braze_id,
        subscription.encrypted_braze_id
    );

    // Delete
    context
        .storage
        .delete_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to delete subscription");

    // Check doesn't exist
    let exists = context
        .storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence after delete");
    assert!(!exists);
}

#[tokio::test]
async fn test_duplicate_prevention() {
    let context = setup_test().await;

    let subscription = create_test_subscription("test-topic");

    // First insert should succeed
    context
        .storage
        .insert(&subscription)
        .await
        .expect("First insert should succeed");

    // Second insert with same HMAC should fail
    let result = context.storage.insert(&subscription).await;
    assert!(matches!(
        result,
        Err(PushNotificationStorageError::PushSubscriptionExists)
    ));

    // Insert with different HMAC but same topic should succeed
    let mut subscription2 = subscription.clone();
    subscription2.hmac = format!("different-hmac-{}", Uuid::new_v4());

    context
        .storage
        .insert(&subscription2)
        .await
        .expect("Insert with different HMAC should succeed");
}

#[tokio::test]
async fn test_query_by_topic() {
    let context = setup_test().await;

    let topic = "shared-topic";
    let other_topic = "other-topic";

    // Insert multiple subscriptions with same topic
    let mut subscriptions = Vec::new();
    for _ in 0..3 {
        let sub = create_test_subscription(topic);
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

    // Query by other topic
    let other_retrieved = context
        .storage
        .get_all_by_topic(other_topic)
        .await
        .expect("Failed to query by other topic");

    assert_eq!(other_retrieved.len(), 1);
    assert_eq!(other_retrieved[0].hmac, other_sub.hmac);

    // Query non-existent topic
    let empty = context
        .storage
        .get_all_by_topic("non-existent")
        .await
        .expect("Failed to query non-existent topic");

    assert_eq!(empty.len(), 0);
}

#[tokio::test]
async fn test_delete_non_existent() {
    let context = setup_test().await;

    // Deleting non-existent item should not error
    context
        .storage
        .delete_by_hmac("non-existent-hmac")
        .await
        .expect("Delete non-existent should not error");
}

#[tokio::test]
async fn test_exists_by_hmac() {
    let context = setup_test().await;

    let subscription = create_test_subscription("test-topic");

    // Should not exist before insert
    let exists_before = context
        .storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(!exists_before);

    // Insert
    context
        .storage
        .insert(&subscription)
        .await
        .expect("Failed to insert");

    // Should exist after insert
    let exists_after = context
        .storage
        .exists_by_hmac(&subscription.hmac)
        .await
        .expect("Failed to check existence");
    assert!(exists_after);
}

#[tokio::test]
async fn test_get_by_hmacs_duplicate_filtering() {
    let context = setup_test().await;

    // Create and insert 3 unique subscriptions
    let sub1 = create_test_subscription("topic-1");
    let sub2 = create_test_subscription("topic-2");
    let sub3 = create_test_subscription("topic-3");

    context
        .storage
        .insert(&sub1)
        .await
        .expect("Failed to insert sub1");
    context
        .storage
        .insert(&sub2)
        .await
        .expect("Failed to insert sub2");
    context
        .storage
        .insert(&sub3)
        .await
        .expect("Failed to insert sub3");

    // Create a list with duplicates and non-existent HMACs
    let hmacs_with_duplicates = vec![
        sub1.hmac.clone(),
        sub2.hmac.clone(),
        sub1.hmac.clone(), // duplicate
        sub3.hmac.clone(),
        sub2.hmac.clone(), // duplicate
        "non-existent-1".to_string(),
        sub3.hmac.clone(), // duplicate
        "non-existent-2".to_string(),
    ];

    // Call get_by_hmacs
    let existing_hmacs = context
        .storage
        .get_by_hmacs(&hmacs_with_duplicates)
        .await
        .expect("Failed to get existing HMACs");

    // Verify results
    assert_eq!(existing_hmacs.len(), 3, "Should return exactly 3 HMACs");

    // Check that all existing HMACs are returned
    let existing_set: std::collections::HashSet<_> = existing_hmacs.into_iter().collect();
    assert!(existing_set.contains(&sub1.hmac));
    assert!(existing_set.contains(&sub2.hmac));
    assert!(existing_set.contains(&sub3.hmac));

    // Non-existent HMACs should not be in the result
    assert!(!existing_set.contains("non-existent-1"));
    assert!(!existing_set.contains("non-existent-2"));
}

#[tokio::test]
async fn test_get_by_hmacs_large_batch() {
    let context = setup_test().await;

    // Create and insert 150 subscriptions (exceeds BATCH_SIZE of 100)
    let mut subscriptions = Vec::new();
    let mut hmacs = Vec::new();

    for i in 0..150 {
        let sub = create_test_subscription(&format!("topic-{}", i));
        hmacs.push(sub.hmac.clone());
        subscriptions.push(sub);
    }

    // Insert all subscriptions
    for sub in &subscriptions {
        context
            .storage
            .insert(sub)
            .await
            .expect("Failed to insert subscription");
    }

    // Add some non-existent HMACs to the query
    let mut query_hmacs = hmacs.clone();
    query_hmacs.push("non-existent-large-1".to_string());
    query_hmacs.push("non-existent-large-2".to_string());
    query_hmacs.push("non-existent-large-3".to_string());

    // Call get_by_hmacs with 153 HMACs (150 existing + 3 non-existing)
    let existing_hmacs = context
        .storage
        .get_by_hmacs(&query_hmacs)
        .await
        .expect("Failed to get existing HMACs");

    // Verify that all 150 existing HMACs are returned
    assert_eq!(
        existing_hmacs.len(),
        150,
        "Should return exactly 150 existing HMACs"
    );

    // Convert to set for efficient lookup
    let existing_set: std::collections::HashSet<_> = existing_hmacs.into_iter().collect();

    // Verify all inserted HMACs are in the result
    for hmac in &hmacs {
        assert!(
            existing_set.contains(hmac),
            "HMAC {} should be in the result",
            hmac
        );
    }

    // Verify non-existent HMACs are not returned
    assert!(!existing_set.contains("non-existent-large-1"));
    assert!(!existing_set.contains("non-existent-large-2"));
    assert!(!existing_set.contains("non-existent-large-3"));
}
