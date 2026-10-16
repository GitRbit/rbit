//! Cross-cutting request/response processing, applied in `routes::router`.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use rustit_auth::SessionToken;

use crate::{
    AppState,
    client_ip::ClientIp,
    error::{ErrorInfo, WebError},
    extract::{CurrentSession, Viewer},
    views::{errors::ErrorPage, layout::Page},
};

pub const SESSION_COOKIE: &str = "rustit_session";

/// Self-service password changes: the session making one survives it (see
/// [`rustit_services::accounts::PasswordChangeGuard`]); every other session ends.
const PASSWORD_CHANGE_PATH: &str = "/settings/account/password";

/// Resolves the session cookie into a [`Viewer`] (and, for signed-in requests, the
/// [`CurrentSession`]) stored in the request extensions.
pub async fn session(State(state): State<AppState>, jar: CookieJar, mut request: Request, next: Next) -> Response {
    let mut viewer = Viewer::default();
    let mut current = None;
    if let Some(token) = jar.get(SESSION_COOKIE).and_then(|c| SessionToken::parse(c.value())) {
        match state.services.accounts.resume_session(&token).await {
            Ok(Some(user)) => {
                current = Some((user.id(), token));
                viewer = Viewer(Some(Arc::new(user)));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "failed to resume session"),
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
            let page = Page::new(&state, None, request.uri().path()).titled(info.title);
            let (mut parts, _) = response.into_parts();
            let rendered = ErrorPage { page, info }.into_response();
            if let Some(content_type) = rendered.headers().get(header::CONTENT_TYPE) {
                parts.headers.insert(header::CONTENT_TYPE, content_type.clone());
            }
            Response::from_parts(parts, rendered.into_body())
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
pub async fn csrf_origin_check(request: Request, next: Next) -> Response {
    let unsafe_method = is_unsafe(request.method());
    if unsafe_method && is_cross_site(request.headers()) {
        tracing::warn!(path = %request.uri().path(), "blocked cross-site request");
        return (StatusCode::FORBIDDEN, "cross-site request blocked").into_response();
    }
    next.run(request).await
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

/// Adds defensive headers to every HTML response.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
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
    response
}

/// Replaces bare error responses produced by [`crate::error::WebError`] with the full
/// error page, rendered with the current user's navigation.
pub async fn error_pages(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let viewer = request.extensions().get::<Viewer>().cloned().unwrap_or_default();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;

    let Some(info) = response.extensions().get::<ErrorInfo>().cloned() else {
        return response;
    };
    let page = Page::new(&state, viewer.user(), &path).titled(info.title);
    (info.status, ErrorPage { page, info }).into_response()
}
