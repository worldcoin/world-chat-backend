use std::sync::Arc;

use crate::state::EnclaveState;
use crypto_box::SecretKey;
use enclave_types::{EnclaveError, EnclaveNotificationRequest};
use hyper::{Body, Method, Request, Version};
use pontifex::http::HttpClient;
use serde::Serialize;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::info;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclaveNotificationRequest,
) -> Result<(), EnclaveError> {
    let state = state.read().await;
    let encryption_key = state.keys.private_key.clone();

    if !state.initialized {
        return Err(EnclaveError::NotInitialized);
    }

    let client = state.http_proxy_client.as_ref().unwrap();
    let braze_api_key = state.braze_api_key.clone().unwrap();
    let braze_api_endpoint = state.braze_api_url.clone().unwrap();
    let braze_api_endpoint = format!("{braze_api_endpoint}/messages/send");

    let decrypted_push_ids = request
        .subscribed_encrypted_push_ids
        .iter()
        .map(|id| decrypt_push_id(id.clone(), &encryption_key))
        .collect::<Result<Vec<String>, EnclaveError>>()?;

    send_braze_notification(
        client,
        braze_api_key,
        braze_api_endpoint,
        request.topic,
        decrypted_push_ids,
        request.encrypted_message_base64,
    )
    .await?;

    Ok(())
}

fn decrypt_push_id(
    encrypted_push_id: String,
    encryption_key: &SecretKey,
) -> Result<String, EnclaveError> {
    let encrypted_push_id = hex::decode(encrypted_push_id)
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))?;

    encryption_key
        .unseal(&encrypted_push_id)
        .map(|decrypted| hex::encode(decrypted))
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))
}

#[derive(Serialize)]
struct UserAlias {
    alias_name: String,
    alias_label: String,
}

async fn send_braze_notification(
    client: &HttpClient,
    braze_api_key: String,
    braze_api_endpoint: String,
    topic: String,
    decrypted_push_ids: Vec<String>,
    encrypted_message_base64: String,
) -> Result<(), EnclaveError> {
    let user_aliases = decrypted_push_ids
        .iter()
        .map(|id| UserAlias {
            alias_name: id.clone(),
            alias_label: "push_id".to_string(),
        })
        .collect::<Vec<UserAlias>>();
    let body = json!({
        "user_aliases": user_aliases,
        "messages": {
            "apple_push": {
                "alert": {
                    "title": "world_chat_notification",
                    "body": "world_chat_notification"
                },
                "badge": 1,
                "sound": "default",
                "mutable_content": true,
                "extra": {
                    "topic": topic,
                    "encryptedMessageBase64": encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            },
            "android_push": {
                "title": "world_chat_notification",
                "alert": "world_chat_notification",
                "priority": "high",
                "extra": {
                    "topic": topic,
                    "encryptedMessageBase64": encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            }
        }
    });
    info!("body: {:?}", body);
    let body = Body::from(body.to_string());

    let req = Request::builder()
        .method(Method::POST)
        .uri(braze_api_endpoint)
        .version(Version::HTTP_2)
        .header("Authorization", format!("Bearer {}", braze_api_key))
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))?;

    client
        .request(req)
        .await
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))?;

    Ok(())
}
