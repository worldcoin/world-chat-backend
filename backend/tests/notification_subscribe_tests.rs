mod common;

use chrono::Utc;
use common::TestSetup;
use http::StatusCode;
use rand::{distributions::Alphanumeric, Rng};
use serde_json::json;
use uuid::Uuid;

fn generate_hmac_key() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

async fn subscription_exists(
    context: &TestSetup,
    topic: &str,
    hmac_key: &str,
    encrypted_push_id: &str,
) -> bool {
    let subscription = context
        .push_subscription_storage
        .get_one(topic, hmac_key)
        .await
        .expect("Failed to get subscription");

    subscription.is_some() && subscription.unwrap().encrypted_push_id == encrypted_push_id
}

#[tokio::test]
async fn test_subscribe_happy_path_single_subscription() {
    let context = TestSetup::default().await;

    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    let subscription_request = json!([{
        "topic": topic,
        "hmac_key": hmac_key,
        "ttl": Utc::now().timestamp() + 3600, // 1 hour from now
    }]);

    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            subscription_request,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    assert!(subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);
}

#[tokio::test]
async fn test_subscribe_with_duplicate_subscriptions() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();
    let ttl = Utc::now().timestamp() + 3600;

    // First subscription request - should succeed
    let subscription = json!([{
        "topic": topic.clone(),
        "hmac_key": hmac_key.clone(),
        "ttl": ttl,
    }]);

    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            subscription.clone(),
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);

    let other_encrypted_push_id = format!("some_other_encrypted_push_id-{}", Uuid::new_v4());
    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            subscription,
            vec![(
                "Authorization",
                &format!("Bearer {}", other_encrypted_push_id),
            )],
        )
        .await
        .expect("Failed to send request");

    // Should still return 201 CREATED even though the subscription already exists
    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(!subscription_exists(&context, &topic, &hmac_key, &other_encrypted_push_id).await);
}

#[tokio::test]
async fn test_subscribe_without_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let subscription_request = json!([{
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
        "ttl": Utc::now().timestamp() + 3600,
    }]);

    let response = context
        .send_post_request("/v1/notifications", subscription_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_subscribe_with_invalid_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let subscription_request = json!([{
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
        "ttl": Utc::now().timestamp() + 3600,
    }]);

    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            subscription_request,
            vec![("Authorization", "Bearer invalid.jwt.encrypted_push_id")],
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_subscribe_empty_request_body() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let empty_request = json!([]);

    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            empty_request,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    // Empty array should fail with bad request error
    assert_eq!(response.status(), StatusCode::BAD_REQUEST,);
}

#[tokio::test]
async fn test_subscribe_missing_required_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            json!([{
                // Missing topic
                "hmac_key": generate_hmac_key(),
                "ttl": Utc::now().timestamp() + 3600,
            }]),
            "missing topic",
        ),
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                // Missing hmac_key
                "ttl": Utc::now().timestamp() + 3600,
            }]),
            "missing hmac_key",
        ),
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": generate_hmac_key(),
                // Missing ttl
            }]),
            "missing ttl",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_post_request_with_headers(
                "/v1/notifications",
                request,
                vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
            )
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Request with {} should return 400",
            case_name
        );
    }
}

#[tokio::test]
async fn test_subscribe_invalid_field_types() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            json!([{
                "topic": 12345, // Should be string
                "hmac_key": generate_hmac_key(),
                "ttl": Utc::now().timestamp() + 3600,
            }]),
            "invalid topic type",
        ),
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": 12345, // Should be string
                "ttl": Utc::now().timestamp() + 3600,
            }]),
            "invalid hmac_key type",
        ),
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": generate_hmac_key(),
                "ttl": "not_a_number", // Should be i64
            }]),
            "invalid ttl type",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_post_request_with_headers(
                "/v1/notifications",
                request,
                vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
            )
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Request with {} should return 400",
            case_name
        );
    }
}

#[tokio::test]
async fn test_subscribe_invalid_ttl_values() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": generate_hmac_key(),
                "ttl": 0, // Should be >= 1 according to schema validation
            }]),
            "zero ttl",
        ),
        (
            json!([{
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": generate_hmac_key(),
                "ttl": -1, // Should be >= 1
            }]),
            "negative ttl",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_post_request_with_headers(
                "/v1/notifications",
                request,
                vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
            )
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Request with {} should return 400",
            case_name
        );
    }
}

#[tokio::test]
async fn test_subscribe_extra_fields_rejected() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let request_with_extra_field = json!([{
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
        "ttl": Utc::now().timestamp() + 3600,
        "extra_field": "should_be_rejected", // This should cause validation to fail
    }]);

    let response = context
        .send_post_request_with_headers(
            "/v1/notifications",
            request_with_extra_field,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    // Should return 400 due to deny_unknown_fields
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_subscribe_empty_string_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        json!([{
            "topic": "", // Empty string
            "hmac_key": generate_hmac_key(),
            "ttl": Utc::now().timestamp() + 3600,
        }]),
        json!([{
            "topic": format!("topic-{}", Uuid::new_v4()),
            "hmac_key": "", // Empty string - should fail validation
            "ttl": Utc::now().timestamp() + 3600,
        }]),
        json!([{
            "topic": format!("topic-{}", Uuid::new_v4()),
            "hmac_key": "abc123", // Too short - should fail validation
            "ttl": Utc::now().timestamp() + 3600,
        }]),
    ];

    for request in test_cases {
        let response = context
            .send_post_request_with_headers(
                "/v1/notifications",
                request,
                vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
            )
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
