//! Resolve once after session lookup; annotate HTML before partial slicing/ETag.
use super::{CATALOG_HEADER, RequestLocale, response_for, single_header};
use axum::{
    extract::{FromRequestParts, Request},
    http::{HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use rbit_localization::Locale;

pub async fn page_locale(request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let locale = RequestLocale::from_request_parts(&mut parts, &()).await.unwrap_or_else(|never| match never {});
    let read = matches!(parts.method, Method::GET | Method::HEAD);
    let htmx = single_header(&parts.headers, "hx-request") == Some("true");
    let preload = crate::middleware::is_preload(&parts.headers);
    let document_key = single_header(&parts.headers, "rbit-document-locale");
    let default_mismatch = document_key.is_some_and(|key| key != locale.document_locale().catalog_key());
    let resolved_mismatch = document_key.is_some_and(|key| key != locale.0.locale.catalog_key());
    let response = next.run(Request::from_parts(parts, body)).await;
    let html = response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
    // Commit-pinned code fragments are language-neutral and retain existing public
    // immutability. Raw/API/static bodies do not enter the translated page contract.
    let user_content = response.headers().contains_key(header::CONTENT_DISPOSITION)
        || response.headers().get(header::CONTENT_SECURITY_POLICY).is_some_and(|value| value.as_bytes().starts_with(b"sandbox"));
    if !html || user_content || crate::middleware::is_immutable(response.headers()) {
        return response;
    }
    let selected = match response.headers().get(header::CONTENT_LANGUAGE).and_then(|value| value.to_str().ok()) {
        Some("en") => Locale::English,
        Some("es") => Locale::Spanish,
        Some("en-XA") if locale.0.locale == Locale::PseudoExpanded => Locale::PseudoExpanded,
        Some("ar-XB") if locale.0.locale == Locale::PseudoRtl => Locale::PseudoRtl,
        _ => locale.document_locale(),
    };
    let mut response = response_for(selected, response);
    let mismatch = if selected == locale.0.locale { resolved_mismatch } else { default_mismatch };
    if read && htmx && !preload && response.status().is_success() && mismatch {
        response.headers_mut().insert("hx-refresh", HeaderValue::from_static("true"));
        // Only this transitional mismatch is uncacheable; normal translated/preload
        // responses keep their existing privacy/freshness and validator behavior.
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    debug_assert!(response.headers().contains_key(CATALOG_HEADER));
    response
}
