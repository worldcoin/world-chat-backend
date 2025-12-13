use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Adds privacy-preserving client metadata to the current trace span.
///
/// Only non-identifying information is recorded (platform, app version, OS version).
/// Future: oHTTP integration will further enhance privacy by hiding client IPs from the server.
pub async fn add_client_info_to_span(request: Request, next: Next) -> Response {
    let span = Span::current();

    for (header, key) in [
        ("client-name", "client.platform"),
        ("client-version", "client.version"),
        ("client-os-version", "client.os_version"),
    ] {
        if let Some(value) = request.headers().get(header).and_then(|v| v.to_str().ok()) {
            span.set_attribute(key, value.to_owned());
        }
    }

    next.run(request).await
}
