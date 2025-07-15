mod common;

use backend_storage::{
    push_notification::{PushNotificationStorage, PushSubscription},
    queue::{QueueMessage, Recipient, SubscriptionRequest},
};
use chrono::Utc;
use common::*;
use http::StatusCode;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

// Helper function to create a valid unsubscribe request JSON
fn create_unsubscribe_request(
    encrypted_braze_id: &str,
    hmac: &str,
    topic: &str,
) -> serde_json::Value {
    json!({
        "encrypted_braze_id": encrypted_braze_id,
        "hmac": hmac,
        "topic": topic
    })
}

// Helper function to create a subscription in the database
async fn create_subscription_in_db(
    push_storage: &Arc<PushNotificationStorage>,
    hmac: &str,
    topic: &str,
    ttl: i64,
    encrypted_braze_id: &str,
) -> PushSubscription {
    let subscription = PushSubscription {
        hmac: hmac.to_string(),
        topic: topic.to_string(),
        ttl,
        encrypted_braze_id: encrypted_braze_id.to_string(),
    };

    push_storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription in DB");

    subscription
}

/// Assert that the unsubscribe message matches expected values
fn assert_unsubscribe_message(
    messages: &[QueueMessage<SubscriptionRequest>],
    expected_hmac: &str,
    expected_topic: &str,
    expected_braze_id: &str,
    expected_topic_members: &[PushSubscription],
) {
    assert_eq!(messages.len(), 1, "Should have exactly one message");

    match &messages[0].body {
        SubscriptionRequest::Unsubscribe {
            hmac,
            topic,
            encrypted_braze_id,
            topic_members,
        } => {
            assert_eq!(hmac, expected_hmac);
            assert_eq!(topic, expected_topic);
            assert_eq!(encrypted_braze_id, expected_braze_id);

            // Convert expected PushSubscriptions to Recipients for comparison
            let expected_recipients: Vec<Recipient> = expected_topic_members
                .iter()
                .map(|m| Recipient {
                    encrypted_braze_id: m.encrypted_braze_id.clone(),
                    hmac: m.hmac.clone(),
                })
                .collect();

            assert_eq!(
                topic_members.len(),
                expected_recipients.len(),
                "Topic members count mismatch. Got: {:?}, Expected: {:?}",
                topic_members,
                expected_recipients
            );

            // Check that all expected recipients are present
            for expected in &expected_recipients {
                assert!(
                    topic_members.iter().any(|m| {
                        m.encrypted_braze_id == expected.encrypted_braze_id
                            && m.hmac == expected.hmac
                    }),
                    "Expected recipient not found: {:?}",
                    expected
                );
            }
        }
        _ => panic!("Expected Unsubscribe variant"),
    }
}

// Happy path test
#[tokio::test]
async fn test_unsubscribe_happy_path() {
    let setup = TestContext::new(None).await;
    let push_storage = &setup.push_notification_storage;

    // Create topic with multiple members - use unique topic name to avoid conflicts
    let topic = &format!("tech_news_unsub_test_{}", Uuid::new_v4());
    let ttl = (Utc::now() + chrono::Duration::hours(24)).timestamp();

    // Create 3 subscriptions for the same topic
    let sub1 = create_subscription_in_db(
        push_storage,
        &format!("hmac1_{}", Uuid::new_v4()),
        topic,
        ttl,
        &format!("braze_id1_{}", Uuid::new_v4()),
    )
    .await;

    let sub2 = create_subscription_in_db(
        push_storage,
        &format!("hmac2_{}", Uuid::new_v4()),
        topic,
        ttl,
        &format!("braze_id2_{}", Uuid::new_v4()),
    )
    .await;

    let sub3 = create_subscription_in_db(
        push_storage,
        &format!("hmac3_{}", Uuid::new_v4()),
        topic,
        ttl,
        &format!("braze_id3_{}", Uuid::new_v4()),
    )
    .await;

    // Unsubscribe the second user
    let payload = create_unsubscribe_request(&sub2.encrypted_braze_id, &sub2.hmac, topic);

    let response = setup
        .send_post_request("/v1/notifications/unsubscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll the queue to verify message was sent
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Verify the message contains all topic members
    assert_unsubscribe_message(
        &messages,
        &sub2.hmac,
        topic,
        &sub2.encrypted_braze_id,
        &[sub1, sub2.clone(), sub3],
    );
}

// Test unsubscribe when user doesn't exist in DB
#[tokio::test]
async fn test_unsubscribe_user_not_in_db() {
    let setup = TestContext::new(None).await;
    let push_storage = &setup.push_notification_storage;

    // Create topic with some members - use unique topic name to avoid conflicts
    let topic = &format!("sports_unsub_test_{}", Uuid::new_v4());
    let ttl = (Utc::now() + chrono::Duration::hours(24)).timestamp();

    // Create 2 subscriptions for the topic
    let sub1 = create_subscription_in_db(
        push_storage,
        &format!("hmac1_{}", Uuid::new_v4()),
        topic,
        ttl,
        &format!("braze_id1_{}", Uuid::new_v4()),
    )
    .await;

    let sub2 = create_subscription_in_db(
        push_storage,
        &format!("hmac2_{}", Uuid::new_v4()),
        topic,
        ttl,
        &format!("braze_id2_{}", Uuid::new_v4()),
    )
    .await;

    // Try to unsubscribe with a non-existent HMAC
    let non_existent_hmac = format!("non_existent_{}", Uuid::new_v4());
    let non_existent_braze_id = format!("non_existent_braze_{}", Uuid::new_v4());

    let payload = create_unsubscribe_request(&non_existent_braze_id, &non_existent_hmac, topic);

    let response = setup
        .send_post_request("/v1/notifications/unsubscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll the queue to verify message was sent
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Message should still be sent with the existing topic members
    assert_unsubscribe_message(
        &messages,
        &non_existent_hmac,
        topic,
        &non_existent_braze_id,
        &[sub1, sub2],
    );
}

// Test unsubscribe when topic members list is empty
#[tokio::test]
async fn test_unsubscribe_empty_topic_members() {
    let setup = TestContext::new(None).await;

    // Try to unsubscribe from a topic with no members - use unique topic name
    let empty_topic = &format!("empty_topic_unsub_test_{}", Uuid::new_v4());
    let hmac = format!("hmac_{}", Uuid::new_v4());
    let braze_id = format!("braze_{}", Uuid::new_v4());

    let payload = create_unsubscribe_request(&braze_id, &hmac, empty_topic);

    let response = setup
        .send_post_request("/v1/notifications/unsubscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll the queue to verify message was sent
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Message should still be sent with empty topic members
    assert_unsubscribe_message(
        &messages,
        &hmac,
        empty_topic,
        &braze_id,
        &[], // Empty topic members
    );
}
