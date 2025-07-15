mod common;

use backend_storage::{
    push_notification::{PushNotificationStorage, PushSubscription},
    queue::{QueueMessage, SubscriptionRequest},
};
use chrono::Utc;
use common::*;
use http::StatusCode;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

// Helper function to create a valid subscribe request JSON
fn create_subscribe_request(
    encrypted_braze_id: &str,
    subscriptions: &[serde_json::Value],
) -> serde_json::Value {
    json!({
        "encrypted_braze_id": encrypted_braze_id,
        "subscriptions": subscriptions
    })
}

// Helper function to create a subscription in the database
async fn create_subscription_in_db(
    push_storage: &Arc<PushNotificationStorage>,
    hmac: &str,
    topic: &str,
    ttl: i64,
) -> PushSubscription {
    let subscription = PushSubscription {
        hmac: hmac.to_string(),
        topic: topic.to_string(),
        ttl,
        encrypted_braze_id: format!("encrypted_{}", Uuid::new_v4()),
    };

    push_storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription in DB");

    subscription
}

/// Assert that messages match expected subscriptions, handling any order
fn assert_messages_equal(
    messages: &[QueueMessage<SubscriptionRequest>],
    expected_subscriptions: &[serde_json::Value],
    expected_braze_id: &str,
) {
    assert_eq!(
        messages.len(),
        expected_subscriptions.len(),
        "Number of messages doesn't match expected"
    );

    // Extract actual subscription data from messages
    let mut actual: Vec<(String, String, String, i64)> = messages
        .iter()
        .map(|msg| {
            if let SubscriptionRequest::Subscribe {
                hmac,
                encrypted_braze_id,
                topic,
                ttl,
            } = &msg.body
            {
                (
                    hmac.clone(),
                    encrypted_braze_id.clone(),
                    topic.clone(),
                    *ttl,
                )
            } else {
                panic!("expected Subscribe");
            }
        })
        .collect();

    // Extract expected subscription data
    let mut expected: Vec<(String, String, String, i64)> = expected_subscriptions
        .iter()
        .map(|s| {
            (
                s["hmac"].as_str().unwrap().to_string(),
                expected_braze_id.to_string(),
                s["topic"].as_str().unwrap().to_string(),
                s["ttl"].as_i64().unwrap(),
            )
        })
        .collect();

    // canonicalize order and compare
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

// Happy path tests

#[tokio::test]
async fn test_subscribe_happy_path_nothing_exists() {
    let setup = TestContext::new(None).await;

    // Create 3 subscriptions, none exist in DB
    let subscriptions = vec![
        json!({
            "topic": "news",
            "hmac": format!("hmac_{}", Uuid::new_v4()),
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "sports",
            "hmac": format!("hmac_{}", Uuid::new_v4()),
            "ttl": (Utc::now() + chrono::Duration::hours(48)).timestamp()
        }),
        json!({
            "topic": "weather",
            "hmac": format!("hmac_{}", Uuid::new_v4()),
            "ttl": (Utc::now() + chrono::Duration::days(7)).timestamp()
        }),
    ];

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll the queue to verify messages were sent
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Use the utility function to assert messages match expected subscriptions
    assert_messages_equal(&messages, &subscriptions, &encrypted_braze_id);
}

#[tokio::test]
async fn test_subscribe_happy_path_some_exist() {
    let setup = TestContext::new(None).await;

    // Pre-create 2 subscriptions in DB
    let existing_hmac1 = format!("existing_hmac_{}", Uuid::new_v4());
    let existing_hmac2 = format!("existing_hmac_{}", Uuid::new_v4());

    let push_storage = &setup.push_notification_storage;

    create_subscription_in_db(
        push_storage,
        &existing_hmac1,
        "news",
        (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    )
    .await;

    create_subscription_in_db(
        push_storage,
        &existing_hmac2,
        "sports",
        (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    )
    .await;

    // Send 4 subscriptions (2 existing, 2 new)
    let new_hmac1 = format!("new_hmac_{}", Uuid::new_v4());
    let new_hmac2 = format!("new_hmac_{}", Uuid::new_v4());

    let subscriptions = vec![
        json!({
            "topic": "news",
            "hmac": existing_hmac1,
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "weather",
            "hmac": new_hmac1.clone(),
            "ttl": (Utc::now() + chrono::Duration::hours(48)).timestamp()
        }),
        json!({
            "topic": "sports",
            "hmac": existing_hmac2,
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "tech",
            "hmac": new_hmac2.clone(),
            "ttl": (Utc::now() + chrono::Duration::days(7)).timestamp()
        }),
    ];

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll the queue - should only have 2 new subscriptions
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    assert_eq!(
        messages.len(),
        2,
        "Should only have 2 new messages in queue"
    );

    // Filter to get only the new subscriptions that should have been queued
    let new_subscriptions = vec![
        subscriptions[1].clone(), // new_hmac1
        subscriptions[3].clone(), // new_hmac2
    ];

    // Use the utility function to assert messages match expected new subscriptions
    assert_messages_equal(&messages, &new_subscriptions, &encrypted_braze_id);
}

#[tokio::test]
async fn test_subscribe_multiple_subscriptions() {
    let setup = TestContext::new(None).await;

    // Create 10 subscriptions
    let mut subscriptions = Vec::new();
    for i in 0..10 {
        subscriptions.push(json!({
            "topic": format!("topic_{}", i),
            "hmac": format!("hmac_{}_{}", i, Uuid::new_v4()),
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }));
    }

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Verify all 10 were queued
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Use the utility function to assert messages match expected subscriptions
    assert_messages_equal(&messages, &subscriptions, &encrypted_braze_id);
}

// Validation tests

#[tokio::test]
async fn test_subscribe_missing_encrypted_braze_id() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        // Missing encrypted_braze_id
        "subscriptions": [{
            "topic": "news",
            "hmac": "test_hmac",
            "ttl": 12345
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_missing_subscriptions() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id"
        // Missing subscriptions
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_empty_subscriptions_array() {
    let setup = TestContext::new(None).await;

    let payload = create_subscribe_request("test_encrypted_id", &[]);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    // Empty array is technically valid, should return ACCEPTED
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_subscribe_missing_topic() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            // Missing topic
            "hmac": "test_hmac",
            "ttl": 12345
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_missing_hmac() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            "topic": "news",
            // Missing hmac
            "ttl": 12345
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_missing_ttl() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            "topic": "news",
            "hmac": "test_hmac"
            // Missing ttl
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_negative_ttl() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            "topic": "news",
            "hmac": "test_hmac",
            "ttl": -100
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_zero_ttl() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            "topic": "news",
            "hmac": "test_hmac",
            "ttl": 0
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_invalid_json_types() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": 12345, // Should be string
        "subscriptions": [{
            "topic": 123, // Should be string
            "hmac": true, // Should be string
            "ttl": "not_a_number" // Should be number
        }]
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_extra_fields() {
    let setup = TestContext::new(None).await;

    let payload = json!({
        "encrypted_braze_id": "test_encrypted_id",
        "subscriptions": [{
            "topic": "news",
            "hmac": "test_hmac",
            "ttl": 12345,
            "extra_field": "should_be_rejected"
        }],
        "another_extra": "also_rejected"
    });

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Edge case tests

#[tokio::test]
async fn test_subscribe_all_already_exist() {
    let setup = TestContext::new(None).await;

    let push_storage = &setup.push_notification_storage;

    // Create all subscriptions in DB first
    let hmac1 = format!("existing_hmac_{}", Uuid::new_v4());
    let hmac2 = format!("existing_hmac_{}", Uuid::new_v4());
    let hmac3 = format!("existing_hmac_{}", Uuid::new_v4());

    create_subscription_in_db(
        push_storage,
        &hmac1,
        "news",
        (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    )
    .await;

    create_subscription_in_db(
        push_storage,
        &hmac2,
        "sports",
        (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    )
    .await;

    create_subscription_in_db(
        push_storage,
        &hmac3,
        "weather",
        (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    )
    .await;

    // Try to subscribe with all existing HMACs
    let subscriptions = vec![
        json!({
            "topic": "news",
            "hmac": hmac1,
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "sports",
            "hmac": hmac2,
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "weather",
            "hmac": hmac3,
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
    ];

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Queue should be empty
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    assert_eq!(
        messages.len(),
        0,
        "Queue should be empty when all subscriptions exist"
    );
}

#[tokio::test]
async fn test_subscribe_duplicate_hmacs_in_request() {
    let setup = TestContext::new(None).await;

    let duplicate_hmac = format!("duplicate_hmac_{}", Uuid::new_v4());

    // Send request with duplicate HMACs
    let subscriptions = vec![
        json!({
            "topic": "news",
            "hmac": duplicate_hmac.clone(),
            "ttl": (Utc::now() + chrono::Duration::hours(24)).timestamp()
        }),
        json!({
            "topic": "sports",
            "hmac": duplicate_hmac.clone(),
            "ttl": (Utc::now() + chrono::Duration::hours(48)).timestamp()
        }),
        json!({
            "topic": "weather",
            "hmac": duplicate_hmac.clone(),
            "ttl": (Utc::now() + chrono::Duration::days(7)).timestamp()
        }),
    ];

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // All should be queued since they don't exist in DB
    let messages = setup
        .subscription_queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");

    // Use the utility function to assert messages match expected subscriptions
    assert_messages_equal(&messages, &subscriptions, &encrypted_braze_id);

    // Additionally verify all have the same HMAC
    for message in &messages {
        match &message.body {
            backend_storage::queue::SubscriptionRequest::Subscribe { hmac, .. } => {
                assert_eq!(hmac, &duplicate_hmac);
            }
            _ => panic!("Expected Subscribe variant"),
        }
    }
}

#[tokio::test]
async fn test_subscribe_large_batch() {
    let setup = TestContext::new(None).await;

    // Create 110 subscriptions
    let mut subscriptions = Vec::new();
    for i in 0..110 {
        subscriptions.push(json!({
            "topic": format!("topic_{}", i % 10), // 10 different topics
            "hmac": format!("hmac_{}_{}", i, Uuid::new_v4()),
            "ttl": (Utc::now() + chrono::Duration::hours(24 + i)).timestamp()
        }));
    }

    let encrypted_braze_id = format!("encrypted_{}", Uuid::new_v4());
    let payload = create_subscribe_request(&encrypted_braze_id, &subscriptions);

    let response = setup
        .send_post_request("/v1/notifications/subscribe", payload)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Poll multiple times to get all messages (SQS has limits)
    let mut total_messages = Vec::new();

    // Poll up to 11 times (110 messages each)
    for _ in 0..11 {
        let messages = setup
            .subscription_queue
            .poll_messages()
            .await
            .expect("Failed to poll messages");

        if messages.is_empty() {
            break;
        }

        total_messages.extend(messages);
    }

    // Use the utility function to assert messages match expected subscriptions
    assert_messages_equal(&total_messages, &subscriptions, &encrypted_braze_id);
}
