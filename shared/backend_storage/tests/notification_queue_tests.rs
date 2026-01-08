//! Integration tests for NotificationQueue

mod common;

use crate::common::{assert_queue_message, QueueTestContext};
use backend_storage::queue::{Notification, NotificationQueue, QueueConfig};
use pretty_assertions::assert_eq;

#[tokio::test]
async fn test_send_consume_ack_happy_path() {
    let ctx = QueueTestContext::new("notification-happy-path").await;

    // Create queue with test config
    let config = QueueConfig {
        queue_url: ctx.queue_url.clone(),
        default_max_messages: 10,
        default_visibility_timeout: 60,
        default_wait_time_seconds: 0, // No wait for tests
    };
    let queue = NotificationQueue::new(ctx.sqs_client.clone(), config);

    // Create notification
    let notification = Notification {
        topic: "breaking_news".to_string(),
        subscribed_encrypted_push_ids: vec![
            "encrypted_push_id_1".to_string(),
            "encrypted_push_id_2".to_string(),
        ],
        encrypted_message_base64: "eyJ0aXRsZSI6IkJyZWFraW5nIE5ld3MiLCJjb250ZW50IjoiSW1wb3J0YW50IHVwZGF0ZSIsInRpbWVzdGFtcCI6IjIwMjQtMDEtMDFUMTI6MDA6MDBaIn0=".to_string(),
        created_at_ms: None,
    };

    // Send message
    let message_id = queue
        .send_message(&notification)
        .await
        .expect("Failed to send notification");
    assert!(!message_id.is_empty(), "Message ID should not be empty");

    // Poll messages
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 1, "Should receive exactly one message");

    // Verify message content
    let received = &messages[0];
    assert_queue_message(received, &notification);

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
async fn test_fifo_topic_based_grouping() {
    let ctx = QueueTestContext::new("notification-fifo-topics").await;

    let config = QueueConfig {
        queue_url: ctx.queue_url.clone(),
        default_max_messages: 10,
        default_visibility_timeout: 60,
        default_wait_time_seconds: 0,
    };
    let queue = NotificationQueue::new(ctx.sqs_client.clone(), config);

    // Send 3 messages: 2 for topic "news", 1 for topic "alerts"
    let news1 = Notification {
        topic: "news".to_string(),
        subscribed_encrypted_push_ids: vec!["enc_push_news_1".to_string()],
        encrypted_message_base64: "encoded_news_1_base64".to_string(),
        created_at_ms: None,
    };

    let alert1 = Notification {
        topic: "alerts".to_string(),
        subscribed_encrypted_push_ids: vec!["enc_push_alert_1".to_string()],
        encrypted_message_base64: "encoded_alert_1_base64".to_string(),
        created_at_ms: None,
    };

    let news2 = Notification {
        topic: "news".to_string(),
        subscribed_encrypted_push_ids: vec!["enc_push_news_2".to_string()],
        encrypted_message_base64: "encoded_news_2_base64".to_string(),
        created_at_ms: None,
    };

    // Send messages
    queue
        .send_message(&news1)
        .await
        .expect("Failed to send news1");
    queue
        .send_message(&alert1)
        .await
        .expect("Failed to send alert1");
    queue
        .send_message(&news2)
        .await
        .expect("Failed to send news2");

    // Poll with max_messages=10 - should get all 3
    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 3, "Should receive all 3 messages");

    // Verify different topics don't block each other
    let news_messages: Vec<_> = messages.iter().filter(|m| m.body.topic == "news").collect();
    let alert_messages: Vec<_> = messages
        .iter()
        .filter(|m| m.body.topic == "alerts")
        .collect();

    assert_eq!(news_messages.len(), 2, "Should have 2 news messages");
    assert_eq!(alert_messages.len(), 1, "Should have 1 alert message");

    // Verify order within news topic
    let news_payloads: Vec<_> = news_messages
        .iter()
        .map(|m| &m.body.encrypted_message_base64)
        .collect();
    assert_eq!(news_payloads[0], "encoded_news_1_base64");
    assert_eq!(news_payloads[1], "encoded_news_2_base64");

    // Clean up - acknowledge all messages
    for msg in messages {
        queue.ack_message(&msg.receipt_handle).await.unwrap();
    }

    // Send 2 more news notifications to verify continued ordering
    let news3 = Notification {
        topic: "news".to_string(),
        subscribed_encrypted_push_ids: vec!["enc_push_news_3".to_string()],
        encrypted_message_base64: "encoded_news_3_base64".to_string(),
        created_at_ms: None,
    };

    let news4 = Notification {
        topic: "news".to_string(),
        subscribed_encrypted_push_ids: vec!["enc_push_news_4".to_string()],
        encrypted_message_base64: "encoded_news_4_base64".to_string(),
        created_at_ms: None,
    };

    queue
        .send_message(&news3)
        .await
        .expect("Failed to send news3");
    queue
        .send_message(&news4)
        .await
        .expect("Failed to send news4");

    let messages = queue
        .poll_messages()
        .await
        .expect("Failed to poll messages");
    assert_eq!(messages.len(), 2, "Should receive 2 new messages");

    // Verify they maintain order
    assert_eq!(
        messages[0].body.encrypted_message_base64,
        "encoded_news_3_base64"
    );
    assert_eq!(
        messages[1].body.encrypted_message_base64,
        "encoded_news_4_base64"
    );
}
