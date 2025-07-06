pub mod http;

use axum::Router;

pub async fn build_http_router() -> Router {
    http::routes()
}
