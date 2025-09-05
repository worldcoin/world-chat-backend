mod common;

use http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::common::{
    create_subscription, generate_hmac_key, subscription_exists, subscription_has_deletion_request,
    TestSetup,
};

#[tokio::test]
async fn test_unsubscribe_without_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let unsubscribe_request = json!({
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
    });

    let response = context
        .send_delete_request("/v1/subscriptions", unsubscribe_request)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unsubscribe_with_invalid_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let unsubscribe_request = json!({
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
    });

    let response = context
        .send_delete_request_with_headers(
            "/v1/subscriptions",
            unsubscribe_request,
            vec![("Authorization", "Bearer invalid.jwt.encrypted_push_id")],
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unsubscribe_missing_required_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            json!({
                // Missing topic
                "hmac_key": generate_hmac_key(),
            }),
            "missing topic",
        ),
        (
            json!({
                "topic": format!("topic-{}", Uuid::new_v4()),
                // Missing hmac_key
            }),
            "missing hmac_key",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_delete_request_with_headers(
                "/v1/subscriptions",
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
async fn test_unsubscribe_invalid_field_types() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            json!({
                "topic": 12345, // Should be string
                "hmac_key": generate_hmac_key(),
            }),
            "invalid topic type",
        ),
        (
            json!({
                "topic": format!("topic-{}", Uuid::new_v4()),
                "hmac_key": 12345, // Should be string
            }),
            "invalid hmac_key type",
        ),
    ];

    for (request, case_name) in test_cases {
        let response = context
            .send_delete_request_with_headers(
                "/v1/subscriptions",
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
async fn test_unsubscribe_empty_string_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        json!({
            "topic": "", // Empty string - should fail validation
            "hmac_key": generate_hmac_key(),
        }),
        json!({
            "topic": format!("topic-{}", Uuid::new_v4()),
            "hmac_key": "", // Empty string - should fail validation
        }),
        json!({
            "topic": format!("topic-{}", Uuid::new_v4()),
            "hmac_key": "abc123", // Too short - should fail validation
        }),
    ];

    for request in test_cases {
        let response = context
            .send_delete_request_with_headers(
                "/v1/subscriptions",
                request,
                vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
            )
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn test_unsubscribe_extra_fields_rejected() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let request_with_extra_field = json!({
        "topic": format!("topic-{}", Uuid::new_v4()),
        "hmac_key": generate_hmac_key(),
        "extra_field": "should_be_rejected", // This should cause validation to fail
    });

    let response = context
        .send_delete_request_with_headers(
            "/v1/subscriptions",
            request_with_extra_field,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    // Should return 400 due to deny_unknown_fields
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_unsubscribe_nonexistent_subscription() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("nonexistent-topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    let unsubscribe_request = json!({
        "topic": topic,
        "hmac_key": hmac_key,
    });

    let response = context
        .send_delete_request_with_headers(
            "/v1/subscriptions",
            unsubscribe_request,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    // Should return 404 when subscription doesn't exist
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_unsubscribe_matching_push_id_deletes_document() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    // Create a subscription first
    create_subscription(&context, &topic, &hmac_key, &encrypted_push_id).await;
    assert!(subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);

    // Now unsubscribe with the same encrypted_push_id
    let unsubscribe_request = json!({
        "topic": topic,
        "hmac_key": hmac_key,
    });

    let response = context
        .send_delete_request_with_headers(
            "/v1/subscriptions",
            unsubscribe_request,
            vec![("Authorization", &format!("Bearer {}", encrypted_push_id))],
        )
        .await
        .expect("Failed to send request");

    // Should return 204 NO_CONTENT on successful deletion
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Subscription should be completely deleted
    assert!(!subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);
}

#[tokio::test]
async fn test_unsubscribe_nonmatching_push_id_appends_deletion_request() {
    let context = TestSetup::default().await;
    let original_encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let different_encrypted_push_id = format!("different-encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    // Create a subscription with the original encrypted_push_id
    create_subscription(&context, &topic, &hmac_key, &original_encrypted_push_id).await;
    assert!(subscription_exists(&context, &topic, &hmac_key, &original_encrypted_push_id).await);

    // Now try to unsubscribe with a different encrypted_push_id
    let unsubscribe_request = json!({
        "topic": topic,
        "hmac_key": hmac_key,
    });

    let response = context
        .send_delete_request_with_headers(
            "/v1/subscriptions",
            unsubscribe_request,
            vec![(
                "Authorization",
                &format!("Bearer {}", different_encrypted_push_id),
            )],
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Subscription should still exist
    // But should have a deletion request for the different encrypted_push_id
    assert!(subscription_exists(&context, &topic, &hmac_key, &original_encrypted_push_id).await);
    assert!(
        subscription_has_deletion_request(
            &context,
            &topic,
            &hmac_key,
            &different_encrypted_push_id
        )
        .await
    );
}
