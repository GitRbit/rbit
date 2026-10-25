//! Rate limits, cross-site request checks, and response security headers.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::errors::render_error_response;
use crate::{
    AppState,
    client_ip::ClientIp,
    error::{ErrorInfo, WebError},
    themes::Theme,
    views::{errors::ErrorPage, layout::Page},
};

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
pub(super) fn is_one_click_unsubscribe(path: &str) -> bool {
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

/// The configurable parts of [`security_headers`], worked out once from the config.
#[derive(Debug, Clone)]
pub struct SecurityHeaders {
    /// `Strict-Transport-Security`, when the instance is served over HTTPS.
    hsts: Option<HeaderValue>,
    /// The CSP `img-src` sources.
    img_src: &'static str,
}

impl Default for SecurityHeaders {
    /// No HSTS, external images allowed: what an `http://` instance with the default
    /// `[security]` settings sends.
    fn default() -> Self {
        Self { hsts: None, img_src: "'self' data: https:" }
    }
}

impl SecurityHeaders {
    pub fn new(config: &rbit_core::Config) -> Self {
        let security = &config.security;
        let https = config.server.public_url.get(..8).is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
        let hsts = (security.hsts && https).then(|| {
            HeaderValue::from_static(if security.hsts_include_subdomains {
                "max-age=31536000; includeSubDomains"
            } else {
                "max-age=31536000"
            })
        });
        Self { hsts, img_src: if security.external_images { "'self' data: https:" } else { "'self' data:" } }
    }
}

/// Adds defensive headers to responses that can depend on a browser session.
///
/// Default to private, revalidated caching even for raw files, downloads and errors.
/// Cookie authentication alone does not prevent a shared HTTP cache from storing them.
/// This layer wraps HTML caching so explicit page and handler cache policies win.
pub async fn security_headers(State(settings): State<SecurityHeaders>, request: Request, next: Next) -> Response {
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
            "default-src 'self'; img-src {}; style-src 'self' {styles}; script-src 'self'; \
             object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
            settings.img_src
        );
        response
            .headers_mut()
            .entry(header::CONTENT_SECURITY_POLICY)
            .or_insert(HeaderValue::from_str(&policy).expect("valid embedded style hashes"));
    }
    if !response.headers().contains_key(header::CONTENT_SECURITY_POLICY) {
        let policy = format!(
            "default-src 'self'; img-src {}; style-src 'self'; script-src 'self'; \
             object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
            settings.img_src
        );
        response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_str(&policy).expect("valid policy"));
    }
    let headers = response.headers_mut();
    if let Some(hsts) = settings.hsts {
        headers.entry(header::STRICT_TRANSPORT_SECURITY).or_insert(hsts);
    }
    let defaults = [
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
