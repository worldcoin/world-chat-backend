use axum::Json;
use serde_json::{json, Value};

pub async fn hello() -> Json<Value> {
    Json(json!({
        "message": "Hello from Notification Service"
    }))
}
