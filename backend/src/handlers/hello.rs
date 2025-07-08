use axum::Json;
use serde_json::{json, Value};

pub async fn handler() -> Json<Value> {
    Json(json!({
        "message": "Hello from World Chat Backend"
    }))
}
