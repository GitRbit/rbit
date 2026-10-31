//! Preload guards and cache headers for page navigation.

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Sent by the preloader (`assets/preload.js`) on every preload, so the server can tell a
/// preload from a navigation.
pub const PRELOAD_HEADER: &str = "hx-preloaded";

/// Sent by `assets/nav.js` on every htmx request: a per-page-load value that changes
/// after any state-changing request. HTML responses vary on it, so a page preloaded
/// before a form post is never served from the browser cache after it.
pub const NAV_EPOCH_HEADER: &str = "rbit-nav-epoch";

/// How long a preloaded page stays fresh in the browser cache, and (rendered into
/// `base.html`) how long the preloader treats a URL as preloaded: long enough to cover the
/// gap between seeing or hovering a link and clicking it, short enough that it can't go
/// stale. The navigation epoch makes it safe across the viewer's own changes: any form
/// post changes the cache key, so nothing preloaded before it is reused after it.
pub const PRELOAD_MAX_AGE_SECONDS: u32 = 30;

/// Paths a preload must never reach because a `GET` there has side effects, consumes a
/// secret, or is a download or Git traffic. `preload.js` (`NO_PRELOAD`) skips these links too; this is the
/// authoritative check.
pub(super) fn is_unsafe_to_preload(path: &str) -> bool {
    // The security pages and the second sign-in step issue secrets and passkey challenges;
    // (milestone 3, O) `/login/{provider}…` starts or finishes an external sign-in.
    const PREFIXES: [&str; 8] =
        ["/logout", "/settings/security", "/login/", "/email/verify/", "/password/reset/", "/api/", "/assets/", "/sw.js"];
    const EXACT: [&str; 3] = ["/healthz", "/readyz", "/metrics"];
    // `/{owner}/{repo}/{area}/…`: downloads and smart-HTTP endpoints.
    const REPO_AREAS: [&str; 5] = ["raw", "archive", "info", "git-upload-pack", "git-receive-pack"];
    let segments: Vec<&str> = path.split('/').collect();
    let repo_area = segments.get(3).copied().unwrap_or_default();
    PREFIXES.iter().any(|prefix| path.starts_with(prefix))
        || EXACT.contains(&path)
        || REPO_AREAS.contains(&repo_area)
        || (repo_area == "releases" && segments.get(4) == Some(&"download"))
        || segments.get(2).is_some_and(|repo| repo.to_ascii_lowercase().ends_with(".git"))
}

/// Whether `headers` are those of a preload (see [`PRELOAD_HEADER`]).
pub fn is_preload(headers: &HeaderMap) -> bool {
    headers.contains_key(PRELOAD_HEADER)
}

/// The answer to a preload the server declines: empty and never stored, so the click
/// that follows goes to the network as usual.
pub fn decline_preload() -> Response {
    (StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, "no-store")]).into_response()
}

/// Declines preloads of [unsafe](is_unsafe_to_preload) paths before any handler runs, so
/// hovering a link can never trigger a side effect. Handlers decline others whose `GET`
/// changes state for the viewer (viewing a discussion marks it read).
pub async fn preload_guard(request: Request, next: Next) -> Response {
    if is_preload(request.headers()) && is_unsafe_to_preload(request.uri().path()) {
        return decline_preload();
    }
    next.run(request).await
}

/// Cache headers for server-rendered HTML.
///
/// * Every page is `private` (it may show the viewer's own data) and must be
///   revalidated (`no-cache`), with a weak `ETag` so a revisit costs a `304`.
/// * A page fetched by a preload is instead fresh for
///   [`PRELOAD_MAX_AGE_SECONDS`], so the click that follows is answered from the
///   browser cache without a round trip.
/// * `Vary` keeps htmx requests (and each navigation epoch) apart from full page loads.
///
/// Handlers that set their own `Cache-Control` (e.g. `no-store` for secrets) keep it.
pub async fn html_caching(request: Request, next: Next) -> Response {
    let is_read = matches!(*request.method(), Method::GET | Method::HEAD);
    let preloaded = request.headers().contains_key(PRELOAD_HEADER);
    let if_none_match = request.headers().get(header::IF_NONE_MATCH).cloned();
    let response = next.run(request).await;

    let is_html = response.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    if !is_read || !is_html {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let has_locale_identity = parts.headers.contains_key("rbit-locale-revision");
    // English catalogs cannot override the built-in presentation. Keep the host's
    // content-addressed fragment and preload fast paths for this neutral fallback.
    let localized = parts.headers.get(header::CONTENT_LANGUAGE).is_some_and(|value| value.as_bytes() != b"en");
    // Commit identity does not include the visitor's locale or installed catalog.
    if localized && is_immutable(&parts.headers) {
        parts.headers.remove(header::CACHE_CONTROL);
    }
    if has_locale_identity {
        append_vary(&mut parts.headers, "Cookie, Accept-Language, Rbit-Locale-Revision");
    }
    // A response pinned by commit ids (`cache_pinned`) is the same in every epoch; varying
    // on it would throw a year-long cache entry away after every form post.
    let vary = if is_immutable(&parts.headers) { "HX-Request" } else { "HX-Request, Rbit-Nav-Epoch, Rbit-Plugin-Revision" };
    append_vary(&mut parts.headers, vary);
    if parts.status != StatusCode::OK || parts.headers.contains_key(header::CACHE_CONTROL) || parts.headers.contains_key(header::SET_COOKIE)
    {
        return Response::from_parts(parts, body);
    }

    // Pages are rendered into memory, so collecting the body costs nothing extra.
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to read the response body").into_response();
    };
    // The footer's render time changes on every request; leave it out of the ETag.
    let etag = weak_etag(crate::timing::without_render_time(&bytes));
    let cache_control =
        if preloaded && !localized { format!("private, max-age={PRELOAD_MAX_AGE_SECONDS}") } else { "private, no-cache".to_owned() };
    parts.headers.insert(header::CACHE_CONTROL, HeaderValue::from_str(&cache_control).unwrap_or(HeaderValue::from_static("no-cache")));
    if let Ok(value) = HeaderValue::from_str(&etag) {
        parts.headers.insert(header::ETAG, value);
    }

    if if_none_match.is_some_and(|tags| etag_matches(&tags, &etag)) {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(header::CONTENT_TYPE);
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, axum::body::Body::empty());
    }
    Response::from_parts(parts, axum::body::Body::from(bytes))
}

/// `Cache-Control` of a response fully determined by the commit ids in its URL.
pub const PINNED_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// Lets the browser keep `response` for a year without asking again, when it is fully
/// determined by commit ids in its URL (deferred-slot fragments) and the repository is
/// readable anonymously. Fragments of private repositories keep [`html_caching`]'s
/// revalidation, so a shared browser never serves one account's private fragment to the
/// next. Handlers call this; `html_caching` leaves a response with `Cache-Control` alone.
pub fn cache_pinned(mut response: Response, readable_anonymously: bool) -> Response {
    if readable_anonymously && response.status() == StatusCode::OK {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(PINNED_CACHE_CONTROL));
    }
    response
}

/// Whether `headers` mark the response as [pinned](cache_pinned): never changing at its URL.
pub(crate) fn is_immutable(headers: &HeaderMap) -> bool {
    headers
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|d| d.trim().eq_ignore_ascii_case("immutable")))
}

pub(super) fn weak_etag(body: [&[u8]; 2]) -> String {
    use std::fmt::Write;

    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for part in body {
        hash.update(part);
    }
    let digest = hash.finalize();
    let mut etag = String::from("W/\"");
    for byte in digest.iter().take(10) {
        let _ = write!(etag, "{byte:02x}");
    }
    etag.push('"');
    etag
}

/// `If-None-Match` uses weak comparison: `W/"x"` matches `"x"` and `W/"x"`.
fn etag_matches(header_value: &HeaderValue, etag: &str) -> bool {
    let bare = etag.trim_start_matches("W/");
    header_value.to_str().is_ok_and(|tags| tags.split(',').map(str::trim).any(|tag| tag == "*" || tag.trim_start_matches("W/") == bare))
}

fn append_vary(headers: &mut HeaderMap, names: &str) {
    let combined = match headers.get(header::VARY).and_then(|v| v.to_str().ok()) {
        Some(existing) if !existing.is_empty() => format!("{existing}, {names}"),
        _ => names.to_owned(),
    };
    if let Ok(value) = HeaderValue::from_str(&combined) {
        headers.insert(header::VARY, value);
    }
}
