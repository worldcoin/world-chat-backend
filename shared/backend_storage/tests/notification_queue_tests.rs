//! Integration tests for NotificationQueue

mod common;

use crate::common::{assert_queue_message, QueueTestContext};
use backend_storage::queue::{Notification, NotificationQueue, QueueConfig, Recipient};
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

    // Create notification with multiple recipients
    let notification = Notification {
        topic: "breaking_news".to_string(),
        recipients: vec![
            Recipient {
                encrypted_braze_id: "enc_user1".to_string(),
                hmac: "hmac1".to_string(),
            },
            Recipient {
                encrypted_braze_id: "enc_user2".to_string(),
                hmac: "hmac2".to_string(),
            },
            Recipient {
                encrypted_braze_id: "enc_user3".to_string(),
                hmac: "hmac3".to_string(),
            },
        ],
        payload: r#"{"title":"Breaking News","content":"Important update","timestamp":"2024-01-01T12:00:00Z"}"#.to_string(),
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
        recipients: vec![Recipient {
            encrypted_braze_id: "enc_1".to_string(),
            hmac: "hmac_1".to_string(),
        }],
        payload: r#"{"id":1,"type":"news"}"#.to_string(),
    };

    let alert1 = Notification {
        topic: "alerts".to_string(),
        recipients: vec![Recipient {
            encrypted_braze_id: "enc_2".to_string(),
            hmac: "hmac_2".to_string(),
        }],
        payload: r#"{"id":2,"type":"alert"}"#.to_string(),
    };

    let news2 = Notification {
        topic: "news".to_string(),
        recipients: vec![Recipient {
            encrypted_braze_id: "enc_3".to_string(),
            hmac: "hmac_3".to_string(),
        }],
        payload: r#"{"id":3,"type":"news"}"#.to_string(),
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
    let news_payloads: Vec<_> = news_messages.iter().map(|m| &m.body.payload).collect();
    assert_eq!(news_payloads[0], r#"{"id":1,"type":"news"}"#);
    assert_eq!(news_payloads[1], r#"{"id":3,"type":"news"}"#);

    // Clean up - acknowledge all messages
    for msg in messages {
        queue.ack_message(&msg.receipt_handle).await.unwrap();
    }

    // Send 2 more news notifications to verify continued ordering
    let news3 = Notification {
        topic: "news".to_string(),
        recipients: vec![Recipient {
            encrypted_braze_id: "enc_4".to_string(),
            hmac: "hmac_4".to_string(),
        }],
        payload: r#"{"id":4,"type":"news"}"#.to_string(),
    };

    let news4 = Notification {
        topic: "news".to_string(),
        recipients: vec![Recipient {
            encrypted_braze_id: "enc_5".to_string(),
            hmac: "hmac_5".to_string(),
        }],
        payload: r#"{"id":5,"type":"news"}"#.to_string(),
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
    assert_eq!(messages[0].body.payload, r#"{"id":4,"type":"news"}"#);
    assert_eq!(messages[1].body.payload, r#"{"id":5,"type":"news"}"#);
}
