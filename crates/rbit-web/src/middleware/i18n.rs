//! One locale/catalog snapshot per HTML request, including error pages and fragments.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, Uri, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{CookieJar, cookie::Cookie};

use crate::AppState;

pub const LOCALE_COOKIE: &str = "rbit-locale";

/// Resolved by the existing session query; this middleware never queries the database.
#[derive(Clone, Debug, Default)]
pub(crate) struct AccountLocale(pub Option<String>);

pub async fn locale(State(state): State<AppState>, jar: CookieJar, request: Request, next: Next) -> Response {
    let account = request.extensions().get::<AccountLocale>().and_then(|value| value.0.as_deref());
    let cookie = jar.get(LOCALE_COOKIE).map(Cookie::value).filter(|value| value.len() <= 128);
    let accept = request.headers().get(header::ACCEPT_LANGUAGE).and_then(|value| value.to_str().ok()).filter(|value| value.len() <= 4096);
    let snapshot = state.services.locale_snapshot(account, cookie, accept);
    let revision = format!("{}:{}", snapshot.lang(), snapshot.revision());
    let language = snapshot.lang().to_owned();
    let catalog_revision = snapshot.revision();
    let read = matches!(*request.method(), Method::GET | Method::HEAD);
    let target = request.uri().clone();
    let mut response = crate::i18n::scope(snapshot, next.run(request)).await;
    let html = response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
    if !html {
        return response;
    }
    if read && catalog_revision != state.services.i18n_revision() {
        response = catalog_refresh(response, &target);
    }
    if let Ok(value) = HeaderValue::from_str(&language) {
        response.headers_mut().insert(header::CONTENT_LANGUAGE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&revision) {
        response.headers_mut().insert("rbit-locale-revision", value);
    }
    response
}

fn catalog_refresh(response: Response, target: &Uri) -> Response {
    let (parts, body) = response.into_parts();
    // Native response ownership must outlive destruction of its source, even
    // when a catalog change replaces the entire rendered representation.
    drop(body);
    let mut refresh = Redirect::temporary(&target.to_string()).into_response();
    super::plugins::copy_security_headers(&parts.headers, refresh.headers_mut());
    drop(parts);
    refresh.headers_mut().insert("hx-refresh", HeaderValue::from_static("true"));
    refresh.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    refresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct Source(Arc<AtomicBool>);
    impl http_body::Body for Source {
        type Data = axum::body::Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            std::task::Poll::Pending
        }
    }
    impl Drop for Source {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    struct Operation(Arc<AtomicBool>);
    impl Drop for Operation {
        fn drop(&mut self) {
            assert!(self.0.load(Ordering::SeqCst), "operation ownership must outlive its source");
        }
    }

    #[test]
    fn native_i18n_routes_catalog_refresh_drops_source_before_operation_and_preserves_security() {
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = Response::new(axum::body::Body::new(Source(Arc::clone(&dropped))));
        response.extensions_mut().insert(Arc::new(Operation(Arc::clone(&dropped))));
        response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("frame-ancestors 'none'"));
        let response = catalog_refresh(response, &"/-/plugins/rbit.i18n/language".parse().unwrap());
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(response.status(), axum::http::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "frame-ancestors 'none'");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()["hx-refresh"], "true");
    }
}
