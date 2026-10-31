//! One locale/catalog snapshot per HTML request, including error pages and fragments.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, header},
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
        response = Redirect::temporary(&target.to_string()).into_response();
        response.headers_mut().insert("hx-refresh", HeaderValue::from_static("true"));
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    if let Ok(value) = HeaderValue::from_str(&language) {
        response.headers_mut().insert(header::CONTENT_LANGUAGE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&revision) {
        response.headers_mut().insert("rbit-locale-revision", value);
    }
    response
}
