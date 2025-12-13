use axum::{extract::Request, middleware::Next, response::Response};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Context, KeyValue};

/// Adds privacy-preserving client metadata to the current trace span.
///
/// Only non-identifying information is recorded (platform, app version, OS version).
/// Future: oHTTP integration will further enhance privacy by hiding client IPs from the server.
pub async fn add_client_info_to_span(request: Request, next: Next) -> Response {
    // Get the OpenTelemetry context directly (bypasses tracing's Span::current())
    let cx = Context::current();
    let span = cx.span();

    for header in ["client-name", "client-version", "client-os-version"] {
        if let Some(value) = request.headers().get(header).and_then(|v| v.to_str().ok()) {
            let key = format!("http.request.headers.{header}");
            span.set_attribute(KeyValue::new(key, value.to_owned()));
        }
    }

    next.run(request).await
}
