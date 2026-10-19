//! Cross-cutting request/response processing, applied in `routes::router`.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use rbit_auth::SessionToken;

use crate::{
    AppState,
    client_ip::ClientIp,
    error::{ErrorInfo, WebError},
    extract::{CurrentSession, Viewer},
    themes::Theme,
    views::{errors::ErrorPage, layout::Page},
};

pub const SESSION_COOKIE: &str = "rbit_session";
/// The session cookie's name from before the rename to rbit, still accepted so nobody is
/// signed out by the upgrade; signing out clears it.
pub const LEGACY_SESSION_COOKIE: &str = "rustit_session";

/// The session token the request carries, under the current cookie name or the legacy one.
pub fn session_token(jar: &CookieJar) -> Option<SessionToken> {
    [SESSION_COOKIE, LEGACY_SESSION_COOKIE].into_iter().find_map(|name| jar.get(name).and_then(|c| SessionToken::parse(c.value())))
}

/// Self-service password changes: the session making one survives it (see
/// [`rbit_services::accounts::PasswordChangeGuard`]); every other session ends.
const PASSWORD_CHANGE_PATH: &str = "/settings/account/password";

/// Resolves the session cookie into a [`Viewer`] (and, for signed-in requests, the
/// [`CurrentSession`]) stored in the request extensions.
pub async fn session(State(state): State<AppState>, jar: CookieJar, mut request: Request, next: Next) -> Response {
    let mut viewer = Viewer::default();
    let mut current = None;
    if let Some(token) = session_token(&jar) {
        match state.services.accounts.resume_session(&token).await {
            Ok(Some(user)) => {
                current = Some((user.id(), token));
                viewer = Viewer(Some(Arc::new(user)));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "failed to resume session"),
        }
    }
    if let Some(user) = viewer.user() {
        match state.services.accounts.password_change_required(user.id()).await {
            Ok(true) if !allowed_during_password_change(request.method(), request.uri().path()) => {
                return ([(header::CACHE_CONTROL, "no-store")], Redirect::to("/settings/account")).into_response();
            }
            Ok(_) => {}
            Err(error) => return WebError::from(error).into_response(),
        }
    }
    request.extensions_mut().insert(viewer);

    let changes_password = request.method() == Method::POST && request.uri().path() == PASSWORD_CHANGE_PATH;
    let guard = match &current {
        Some((user, token)) if changes_password => Some(state.services.accounts.guard_password_change(*user, token)),
        _ => None,
    };
    if let Some((_, token)) = current {
        request.extensions_mut().insert(CurrentSession(token));
    }

    let response = next.run(request).await;
    if let Some(guard) = guard {
        // Settings forms redirect after success and re-render (422) after a failure.
        guard.finish(response.status().is_redirection()).await;
    }
    response
}

/// Only the password form, its submission, sign-out and presentation assets remain available.
fn allowed_during_password_change(method: &Method, path: &str) -> bool {
    (matches!(*method, Method::GET | Method::HEAD) && matches!(path, "/settings/account" | "/assets/branding.css"))
        || (*method == Method::POST && matches!(path, PASSWORD_CHANGE_PATH | "/logout"))
}

/// Throttles state-changing requests (form posts) per client address. Reads are never
/// limited here; sign-in, sign-up, two-factor codes and Git credentials have their own,
/// stricter budgets in the services.
///
/// This layer sits outside the session and error-page layers, so it renders the error
/// page itself (with anonymous chrome: the session has not been resolved yet).
pub async fn rate_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_unsafe(request.method()) {
        return next.run(request).await;
    }
    let ClientIp(ip) = ClientIp::resolve(request.extensions(), request.headers(), &state.config.security);
    match state.services.throttle.check_unsafe_request(&ip.to_canonical().to_string()) {
        Ok(()) => next.run(request).await,
        Err(error) => {
            tracing::warn!(%ip, path = %request.uri().path(), "rate limited");
            let response = WebError::from(error).into_response();
            let Some(info) = response.extensions().get::<ErrorInfo>().cloned() else {
                return response;
            };
            let page = Page::new(&state, None, request.uri().path()).with_theme(Theme::from_headers(request.headers())).titled(info.title);
            render_error_response(response, ErrorPage { page, info })
        }
    }
}

fn is_unsafe(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Rejects cross-site state-changing requests.
///
/// Browsers send `Sec-Fetch-Site` (modern) or `Origin` on unsafe methods; if either says
/// the request came from another site it is refused. Non-browser clients (git, curl)
/// send neither and authenticate without cookies, so they are unaffected.
///
/// The one exemption is the one-click unsubscribe link of notification emails
/// ([`is_one_click_unsubscribe`]).
pub async fn csrf_origin_check(request: Request, next: Next) -> Response {
    let unsafe_method = is_unsafe(request.method());
    if unsafe_method && is_cross_site(request.headers()) && !is_one_click_unsubscribe(request.uri().path()) {
        tracing::warn!(path = %request.uri().path(), "blocked cross-site request");
        return (StatusCode::FORBIDDEN, "cross-site request blocked").into_response();
    }
    next.run(request).await
}

/// Milestone 3 (N): `POST /email/unsubscribe/{token}`, which mail clients send from their
/// own origin (RFC 8058). It needs no session: the signed token is the authorisation, and
/// all it can do is turn notification emails off for the account it names.
fn is_one_click_unsubscribe(path: &str) -> bool {
    path.strip_prefix(rbit_services::notification_email::UNSUBSCRIBE_PATH).is_some_and(|token| !token.is_empty() && !token.contains('/'))
}

fn is_cross_site(headers: &HeaderMap) -> bool {
    if let Some(site) = headers.get("sec-fetch-site") {
        return !matches!(site.as_bytes(), b"same-origin" | b"none");
    }
    let (Some(origin), Some(host)) = (headers.get(header::ORIGIN), headers.get(header::HOST)) else {
        return false;
    };
    let origin_host = origin.to_str().ok().and_then(|o| o.split_once("://")).map(|(_, rest)| rest);
    origin_host != host.to_str().ok()
}

/// Adds defensive headers to responses that can depend on a browser session.
///
/// Default to private, revalidated caching even for raw files, downloads and errors.
/// Cookie authentication alone does not prevent a shared HTTP cache from storing them.
/// This layer wraps HTML caching so explicit page and handler cache policies win.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let theme = Theme::from_headers(request.headers());
    let mut response = next.run(request).await;
    // Keep style attributes and arbitrary inline styles blocked. Only our exact embedded
    // initial styles are allowed; external sheets still support theme switching.
    if response.status() == StatusCode::NOT_MODIFIED
        || response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        let mut styles = crate::assets::style_source("app.css");
        if let Some(path) = theme.stylesheet_path() {
            styles.push(' ');
            styles.push_str(&crate::assets::style_source(&path));
        }
        let policy = format!(
            "default-src 'self'; img-src 'self' data: https:; style-src 'self' {styles}; script-src 'self'; \
             object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
        );
        response
            .headers_mut()
            .entry(header::CONTENT_SECURITY_POLICY)
            .or_insert(HeaderValue::from_str(&policy).expect("valid embedded style hashes"));
    }
    let headers = response.headers_mut();
    let defaults = [
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; img-src 'self' data: https:; style-src 'self'; script-src 'self'; \
             object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::HeaderName::from_static("permissions-policy"), "camera=(), microphone=(), geolocation=(), interest-cohort=()"),
        (header::HeaderName::from_static("cross-origin-opener-policy"), "same-origin"),
    ];
    for (name, value) in defaults {
        headers.entry(name).or_insert(HeaderValue::from_static(value));
    }
    headers.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("private, no-cache"));
    response
}

/// Replaces bare error responses produced by [`crate::error::WebError`] with the full
/// error page, rendered with the current user's navigation.
///
/// A "not found" under `/{owner}/{repo}/…` where `owner/repo` is an old name of a
/// repository the viewer may read (it was renamed or transferred) becomes a redirect to
/// the same path under the current name instead; see [`moved_repository_location`].
pub async fn error_pages(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let viewer = request.extensions().get::<Viewer>().cloned().unwrap_or_default();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let theme = Theme::from_headers(request.headers());
    let response = next.run(request).await;

    let Some(info) = response.extensions().get::<ErrorInfo>().cloned() else {
        return response;
    };
    if info.status == StatusCode::NOT_FOUND
        && let Some(location) = moved_repository_location(&state, &viewer, &path, query.as_deref()).await
    {
        return moved_permanently(&method, &location);
    }
    let page = Page::new(&state, viewer.user(), &path).with_theme(theme).titled(info.title);
    render_error_response(response, ErrorPage { page, info })
}

/// Replaces the body without discarding protocol headers such as `Retry-After`.
fn render_error_response(response: Response, page: impl IntoResponse) -> Response {
    let (mut parts, _) = response.into_parts();
    let rendered = page.into_response();
    if let Some(content_type) = rendered.headers().get(header::CONTENT_TYPE) {
        parts.headers.insert(header::CONTENT_TYPE, content_type.clone());
    }
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, rendered.into_body())
}

/// `path` (and `query`) rewritten to the current name of the repository its first two
/// segments used to name, if the viewer may read that repository.
async fn moved_repository_location(state: &AppState, viewer: &Viewer, path: &str, query: Option<&str>) -> Option<String> {
    let (owner, repo, rest) = split_repo_path(path)?;
    let moved = match state.services.repositories.moved_to(viewer.user(), owner, repo).await {
        Ok(moved) => moved?,
        Err(error) => {
            tracing::error!(%error, "repository redirect lookup failed");
            return None;
        }
    };
    let mut location = format!("/{}{rest}", moved.full_name());
    if let Some(query) = query {
        location.push('?');
        location.push_str(query);
    }
    Some(location)
}

/// `/{owner}/{repo}{rest}` → `(owner, repo, rest)`, where `rest` is empty or starts with `/`.
fn split_repo_path(path: &str) -> Option<(&str, &str, &str)> {
    let (owner, after_owner) = path.strip_prefix('/')?.split_once('/')?;
    let (repo, rest) = after_owner.find('/').map_or((after_owner, ""), |slash| after_owner.split_at(slash));
    (!owner.is_empty() && !repo.is_empty()).then_some((owner, repo, rest))
}

/// `301` for reads, `308` (which keeps the method and body) for everything else. Not
/// cacheable: a new repository can take the old name at any time, and then wins.
fn moved_permanently(method: &Method, location: &str) -> Response {
    let status = if matches!(*method, Method::GET | Method::HEAD) { StatusCode::MOVED_PERMANENTLY } else { StatusCode::PERMANENT_REDIRECT };
    let Ok(location) = HeaderValue::from_str(location) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (status, [(header::LOCATION, location), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))]).into_response()
}

// ------------------------------------------------------------------ navigation caching

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
fn is_unsafe_to_preload(path: &str) -> bool {
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
    // A response pinned by commit ids (`cache_pinned`) is the same in every epoch; varying
    // on it would throw a year-long cache entry away after every form post.
    let vary = if is_immutable(&parts.headers) { "HX-Request" } else { "HX-Request, Rbit-Nav-Epoch" };
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
    let cache_control = if preloaded { format!("private, max-age={PRELOAD_MAX_AGE_SECONDS}") } else { "private, no-cache".to_owned() };
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

fn weak_etag(body: [&[u8]; 2]) -> String {
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

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, middleware::from_fn, routing::get};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn hashing_borrowed_parts_preserves_existing_validators() {
        use sha2::{Digest, Sha256};
        use std::fmt::Write;

        let bodies: &[&[u8]] = &[b"", b"abc", b"<main>hello</main>", b"\0\xff\xc3\xa9"];
        for body in bodies {
            let mut expected = String::from("W/\"");
            for byte in Sha256::digest(body).iter().take(10) {
                write!(expected, "{byte:02x}").unwrap();
            }
            expected.push('"');
            for split in 0..=body.len() {
                assert_eq!(weak_etag([&body[..split], &body[split..]]), expected);
            }
        }
        assert_eq!(weak_etag([b"a", b"bc"]), "W/\"ba7816bf8f01cfea4141\"");
    }

    #[tokio::test]
    async fn timing_changes_keep_validators_without_removing_the_response_span() {
        use crate::timing::{MARK_END, MARK_START};

        let body = |elapsed| format!("<main>same</main>{MARK_START}{elapsed}{MARK_END}<footer>end</footer>");
        let app = Router::new()
            .route("/first", get(move || async move { axum::response::Html(body("1 ms")) }))
            .route("/later", get(move || async move { axum::response::Html(body("125 ms")) }))
            .layer(from_fn(html_caching));
        let first = app.clone().oneshot(Request::builder().uri("/first").body(Body::empty()).unwrap()).await.unwrap();
        let etag = first.headers()[header::ETAG].clone();
        assert_eq!(axum::body::to_bytes(first.into_body(), usize::MAX).await.unwrap().as_ref(), body("1 ms").as_bytes());
        let later = app.clone().oneshot(Request::builder().uri("/later").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(later.headers()[header::ETAG], etag);
        let validated =
            app.oneshot(Request::builder().uri("/later").header(header::IF_NONE_MATCH, etag).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(validated.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn pinned_fragments_are_immutable_only_when_public() {
        let fragment = |public: bool| async move { cache_pinned(axum::response::Html("<span>1</span>").into_response(), public) };
        let app = Router::new()
            .route("/public", get(move || fragment(true)))
            .route("/private", get(move || fragment(false)))
            .layer(from_fn(html_caching));
        let get = |path: &str| Request::builder().uri(path).body(Body::empty()).unwrap();

        let public = app.clone().oneshot(get("/public")).await.unwrap();
        assert_eq!(public.headers()[header::CACHE_CONTROL], PINNED_CACHE_CONTROL);
        // Pinned by commit ids: the same in every navigation epoch.
        assert_eq!(public.headers()[header::VARY], "HX-Request");
        let private = app.oneshot(get("/private")).await.unwrap();
        assert_eq!(private.headers()[header::CACHE_CONTROL], "private, no-cache");
        assert!(private.headers().contains_key(header::ETAG), "revalidated");
        assert_eq!(private.headers()[header::VARY], "HX-Request, Rbit-Nav-Epoch");
    }

    fn app() -> Router {
        Router::new()
            .route("/page", get(|| async { axum::response::Html("<p>hello</p>") }))
            .route("/secret", get(|| async { ([(header::CACHE_CONTROL, "no-store")], axum::response::Html("<p>token</p>")) }))
            .route("/settings/security/two-factor", get(|| async { axum::response::Html("<p>new secret</p>") }))
            .route("/o/r/raw/main/file", get(|| async { "bytes" }))
            .route("/missing", get(|| async { (StatusCode::NOT_FOUND, axum::response::Html("<p>not found</p>")) }))
            .layer(from_fn(html_caching))
            .layer(from_fn(security_headers))
            .layer(from_fn(preload_guard))
    }

    async fn send(path: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        app().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn pages_are_private_revalidated_and_vary_on_htmx_headers() {
        let response = send("/page", &[]).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-cache");
        assert_eq!(response.headers()[header::VARY], "HX-Request, Rbit-Nav-Epoch");
        assert!(response.headers()[header::ETAG].to_str().unwrap().starts_with("W/\""));
    }

    #[tokio::test]
    async fn initial_styles_have_exact_csp_hashes_without_allowing_arbitrary_inline_css() {
        for slug in ["system", "dark", "base24-berlin"] {
            let cookie = format!("rbit-theme={slug}");
            let response = send("/page", &[("cookie", &cookie)]).await;
            let policy = response.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
            assert!(policy.contains(&crate::assets::style_source("app.css")));
            if let Some(path) = Theme::parse(slug).unwrap().stylesheet_path() {
                assert!(policy.contains(&crate::assets::style_source(&path)));
            }
            assert!(!policy.contains("unsafe-inline"));
            assert!(policy.contains("script-src 'self';"));

            // HTML caching removes Content-Type on 304. A revalidation must retain
            // the same style authorization when its headers update the cached page.
            let etag = response.headers()[header::ETAG].to_str().unwrap();
            let revalidated = send("/page", &[("cookie", &cookie), ("if-none-match", etag)]).await;
            assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
            assert_eq!(revalidated.headers()[header::CONTENT_SECURITY_POLICY], policy);
        }
    }

    #[tokio::test]
    async fn preloaded_pages_are_briefly_fresh() {
        let response = send("/page", &[("HX-Request", "true"), ("HX-Preloaded", "true")]).await;
        assert_eq!(response.headers()[header::CACHE_CONTROL], format!("private, max-age={PRELOAD_MAX_AGE_SECONDS}"));
    }

    #[test]
    fn the_preloader_shares_the_freshness_window() {
        // preload.js reads its window from <body data-preload-fresh>, rendered from the constant.
        let base = include_str!("../templates/base.html");
        assert!(base.contains("data-preload-fresh=\"{{ crate::middleware::PRELOAD_MAX_AGE_SECONDS }}\""));
        assert_eq!(PRELOAD_MAX_AGE_SECONDS, 30);
    }

    #[tokio::test]
    async fn matching_etags_get_not_modified() {
        let etag = send("/page", &[]).await.headers()[header::ETAG].to_str().unwrap().to_owned();
        let response = send("/page", &[("If-None-Match", &etag)]).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()[header::ETAG], etag.as_str());
        assert!(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().is_empty());

        let response = send("/page", &[("If-None-Match", "W/\"something-else\"")]).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn handler_cache_control_is_kept_even_for_preloads() {
        let response = send("/secret", &[("HX-Preloaded", "true")]).await;
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(response.headers().get(header::ETAG).is_none());
    }

    #[tokio::test]
    async fn raw_files_and_error_pages_are_never_shared_or_reused_without_validation() {
        for path in ["/o/r/raw/main/file", "/missing"] {
            let response = send(path, &[]).await;
            assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-cache", "{path}");
        }
    }

    #[tokio::test]
    async fn rendering_errors_preserves_retry_after_and_replaces_body_metadata() {
        let mut response = WebError::TooManyRequests { retry_after_seconds: 30 }.into_response();
        response.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from_static("1"));
        let rendered = render_error_response(response, axum::response::Html("<p>wait</p>"));
        assert_eq!(rendered.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(rendered.headers()[header::RETRY_AFTER], "30");
        assert_eq!(rendered.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert!(!rendered.headers().contains_key(header::CONTENT_LENGTH));
        assert_eq!(axum::body::to_bytes(rendered.into_body(), usize::MAX).await.unwrap(), "<p>wait</p>");
    }

    #[tokio::test]
    async fn side_effecting_paths_are_never_preloaded() {
        for path in ["/settings/security/two-factor", "/o/r/raw/main/file"] {
            let response = send(path, &[("HX-Preloaded", "true")]).await;
            assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            // Without the preload header the request goes through.
            assert_eq!(send(path, &[]).await.status(), StatusCode::OK, "{path}");
        }
    }

    #[test]
    fn unsafe_preload_paths() {
        for path in [
            "/logout",
            "/email/verify/abc",
            "/password/reset/abc",
            "/a/b/raw/main/x",
            "/a/b/archive/main.zip",
            "/a/b.git/info/refs",
            "/a/b/git-upload-pack",
            "/healthz",
            "/api/v1/user",
            // Issue passkey challenges.
            "/settings/security",
            "/login/two-factor",
            // Milestone 3 (O): start and finish external sign-in.
            "/login/github",
            "/login/corp/callback",
        ] {
            assert!(is_unsafe_to_preload(path), "{path}");
        }
        for path in [
            "/",
            "/login",
            "/explore",
            "/a/b",
            "/a/b/tree/main/src",
            "/a/b/blob/main/raw/archive.rs",
            "/a/b/commits/main",
            "/settings/profile",
            "/a/b/issues",
            "/a/b/pulls",
            // Discussions decline only signed-in preloads, in their handlers (opening one
            // marks it read); anonymous viewers have no read state.
            "/a/b/issues/1",
            "/a/b/pull/2",
        ] {
            assert!(!is_unsafe_to_preload(path), "{path}");
        }
    }

    #[test]
    fn repository_paths_split_into_owner_repo_and_rest() {
        assert_eq!(split_repo_path("/alice/demo"), Some(("alice", "demo", "")));
        assert_eq!(split_repo_path("/alice/demo/"), Some(("alice", "demo", "/")));
        assert_eq!(split_repo_path("/alice/demo/blob/main/src/lib.rs"), Some(("alice", "demo", "/blob/main/src/lib.rs")));
        assert_eq!(split_repo_path("/alice"), None);
        assert_eq!(split_repo_path("/alice/"), None);
        assert_eq!(split_repo_path("//demo"), None);
        assert_eq!(split_repo_path("alice/demo"), None);
    }

    #[test]
    fn moves_keep_the_method() {
        let read = moved_permanently(&Method::GET, "/bob/demo/tree/main?x=1");
        assert_eq!(read.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(read.headers()[header::LOCATION], "/bob/demo/tree/main?x=1");
        assert_eq!(read.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(moved_permanently(&Method::HEAD, "/bob/demo").status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(moved_permanently(&Method::POST, "/bob/demo/star").status(), StatusCode::PERMANENT_REDIRECT);
    }

    #[test]
    fn only_the_unsubscribe_link_skips_the_cross_site_check() {
        assert!(is_one_click_unsubscribe("/email/unsubscribe/2.abcdef"));
        for path in ["/email/unsubscribe/", "/email/unsubscribe", "/email/unsubscribe/2/x", "/settings/notifications", "/email/verify/x"] {
            assert!(!is_one_click_unsubscribe(path), "{path}");
        }
    }
}
