//! Integration tests for SubscriptionRequestQueue

mod common;

use backend_storage::queue::{QueueConfig, SubscriptionRequest, SubscriptionRequestQueue};
use common::{assert_queue_message, QueueTestContext};
use pretty_assertions::assert_eq;
use std::time::Duration;

#[tokio::test]
async fn test_send_consume_ack_happy_path() {
    let ctx = QueueTestContext::new("subscription-happy-path").await;

    // Create queue with test config
    let config = QueueConfig {
        queue_url: ctx.queue_url.clone(),
        default_max_messages: 10,
        default_visibility_timeout: 30,
        default_wait_time_seconds: 0, // No wait for tests
    };
    let queue = SubscriptionRequestQueue::new(ctx.sqs_client.clone(), config);

    // Create a Subscribe request
    let subscribe_request = SubscriptionRequest::Subscribe {
        hmac: "user123".to_string(),
        encrypted_braze_id: "encrypted_abc123".to_string(),
        topic: "news_updates".to_string(),
        ttl: Duration::from_secs(86400), // 24 hours
    };

    // Send message
    let message_id = queue
        .send_message(&subscribe_request)
        .await
        .expect("Failed to send message");
    assert!(!message_id.is_empty(), "Message ID should not be empty");

    // Poll messages
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 1, "Should receive exactly one message");

    // Verify message content
    let received = &messages[0];
    assert_queue_message(received, &subscribe_request);
    assert!(
        !received.receipt_handle.is_empty(),
        "Receipt handle should not be empty"
    );
    assert!(
        !received.message_id.is_empty(),
        "Message ID should not be empty"
    );

    // Acknowledge message
    queue
        .ack_message(&received.receipt_handle)
        .await
        .expect("Failed to acknowledge message");

    // Poll again - should be empty
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(
        messages.len(),
        0,
        "Queue should be empty after acknowledgment"
    );
}

#[tokio::test]
async fn test_fifo_message_group_ordering() {
    let ctx = QueueTestContext::new("subscription-fifo-ordering").await;

    let config = QueueConfig {
        queue_url: ctx.queue_url.clone(),
        default_max_messages: 10,
        default_visibility_timeout: 30,
        default_wait_time_seconds: 0,
    };
    let queue = SubscriptionRequestQueue::new(ctx.sqs_client.clone(), config);

    // Send 3 messages: 2 with HMAC "user1", 1 with HMAC "user2"
    let msg1_user1 = SubscriptionRequest::Subscribe {
        hmac: "user1".to_string(),
        encrypted_braze_id: "enc_1".to_string(),
        topic: "topic1".to_string(),
        ttl: Duration::from_secs(3600),
    };

    let msg2_user2 = SubscriptionRequest::Subscribe {
        hmac: "user2".to_string(),
        encrypted_braze_id: "enc_2".to_string(),
        topic: "topic2".to_string(),
        ttl: Duration::from_secs(3600),
    };

    let msg3_user1 = SubscriptionRequest::Unsubscribe {
        hmac: "user1".to_string(),
        encrypted_braze_id: "enc_1".to_string(),
        topic: "topic1".to_string(),
    };

    // Send messages
    queue
        .send_message(&msg1_user1)
        .await
        .expect("Failed to send msg1");
    queue
        .send_message(&msg2_user2)
        .await
        .expect("Failed to send msg2");
    queue
        .send_message(&msg3_user1)
        .await
        .expect("Failed to send msg3");

    // Poll with max_messages=10 - should get all 3
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 3, "Should receive all 3 messages");

    // Verify different message groups don't block each other
    let user1_messages: Vec<_> = messages
        .iter()
        .filter(|m| match &m.body {
            SubscriptionRequest::Subscribe { hmac, .. }
            | SubscriptionRequest::Unsubscribe { hmac, .. } => hmac == "user1",
        })
        .collect();
    let user2_messages: Vec<_> = messages
        .iter()
        .filter(|m| match &m.body {
            SubscriptionRequest::Subscribe { hmac, .. }
            | SubscriptionRequest::Unsubscribe { hmac, .. } => hmac == "user2",
        })
        .collect();

    assert_eq!(user1_messages.len(), 2, "Should have 2 messages for user1");
    assert_eq!(user2_messages.len(), 1, "Should have 1 message for user2");

    // Verify order within user1 group
    assert!(matches!(
        user1_messages[0].body,
        SubscriptionRequest::Subscribe { .. }
    ));
    assert!(matches!(
        user1_messages[1].body,
        SubscriptionRequest::Unsubscribe { .. }
    ));

    // Clean up - acknowledge all messages
    for msg in messages {
        queue.ack_message(&msg.receipt_handle).await.unwrap();
    }

    // Send 2 more messages for user1 to verify continued ordering
    let msg4_user1 = SubscriptionRequest::Subscribe {
        hmac: "user1".to_string(),
        encrypted_braze_id: "enc_1".to_string(),
        topic: "topic3".to_string(),
        ttl: Duration::from_secs(3600),
    };

    let msg5_user1 = SubscriptionRequest::Subscribe {
        hmac: "user1".to_string(),
        encrypted_braze_id: "enc_1".to_string(),
        topic: "topic4".to_string(),
        ttl: Duration::from_secs(3600),
    };

    queue
        .send_message(&msg4_user1)
        .await
        .expect("Failed to send msg4");
    queue
        .send_message(&msg5_user1)
        .await
        .expect("Failed to send msg5");

    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 2, "Should receive 2 new messages");

    // Verify they maintain order
    match (&messages[0].body, &messages[1].body) {
        (
            SubscriptionRequest::Subscribe { topic: topic1, .. },
            SubscriptionRequest::Subscribe { topic: topic2, .. },
        ) => {
            assert_eq!(topic1, "topic3");
            assert_eq!(topic2, "topic4");
        }
        _ => panic!("Unexpected message order"),
    }
}

#[tokio::test]
async fn test_unsubscribe_request_type() {
    let ctx = QueueTestContext::new("subscription-unsubscribe").await;

    let config = QueueConfig {
        queue_url: ctx.queue_url.clone(),
        default_max_messages: 10,
        default_visibility_timeout: 30,
        default_wait_time_seconds: 0,
    };
    let queue = SubscriptionRequestQueue::new(ctx.sqs_client.clone(), config);

    // Test Unsubscribe variant
    let unsubscribe_request = SubscriptionRequest::Unsubscribe {
        hmac: "user456".to_string(),
        encrypted_braze_id: "encrypted_xyz789".to_string(),
        topic: "daily_digest".to_string(),
    };

    // Send message
    let message_id = queue
        .send_message(&unsubscribe_request)
        .await
        .expect("Failed to send unsubscribe message");
    assert!(!message_id.is_empty());

    // Poll and verify
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 1);

    // Verify it's an Unsubscribe variant
    assert_queue_message(&messages[0], &unsubscribe_request);

    // Clean up
    queue
        .ack_message(&messages[0].receipt_handle)
        .await
        .unwrap();
}
