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

/// Adds defensive headers to responses that can depend on a browser session.
///
/// Default to private, revalidated caching even for raw files, downloads and errors.
/// Cookie authentication alone does not prevent a shared HTTP cache from storing them.
/// This layer wraps HTML caching so explicit page and handler cache policies win.
///
/// htmx requests (boosted navigations, their preloads, history restores) never keep the
/// response's `<head>`: htmx strips it before swapping, and the page that asked keeps its
/// own head with the stylesheets already applied. Pages rendered for them leave the
/// inlined stylesheets out ([`crate::assets::head_styles`]), and their policy authorizes
/// no inline styles at all. Responses already vary on `HX-Request` (`html_caching`).
pub async fn security_headers(request: Request, next: Next) -> Response {
    let theme = Theme::from_headers(request.headers());
    let head_styles = !request.headers().contains_key("hx-request");
    let mut response = crate::assets::with_head_styles(head_styles, next.run(request)).await;
    // Keep style attributes and arbitrary inline styles blocked. Only our exact embedded
    // initial styles are allowed; external sheets still support theme switching.
    if response.status() == StatusCode::NOT_MODIFIED
        || response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        let mut styles = String::new();
        if head_styles {
            styles.push(' ');
            styles.push_str(&crate::assets::style_source("app.css"));
            if let Some(path) = theme.stylesheet_path() {
                styles.push(' ');
                styles.push_str(&crate::assets::style_source(&path));
            }
        }
        let policy = format!(
            "default-src 'self'; img-src 'self' data: https:; style-src 'self'{styles}; script-src 'self'; \
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

#[cfg(test)]
mod tests {
    use askama::Template;
    use axum::{Router, body::Body, middleware::from_fn, routing::get};
    use tower::ServiceExt as _;

    use super::*;

    #[derive(Template)]
    #[template(source = "{% extends \"base.html\" %}", ext = "html")]
    struct Fixture {
        page: Page,
    }

    async fn page(headers: HeaderMap) -> axum::response::Html<String> {
        let page = Page {
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".into(),
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/".into(),
            has_unread_notifications: false,
            theme: Theme::from_headers(&headers),
        };
        axum::response::Html(Fixture { page }.render().unwrap())
    }

    async fn send(headers: &[(&str, &str)]) -> (String, String) {
        let app = Router::new().route("/", get(page)).layer(from_fn(security_headers));
        let mut request = Request::builder().uri("/");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = app.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let policy = response.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap().to_owned();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (policy, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn htmx_requests_get_pages_without_inline_stylesheets() {
        let theme = "rbit-theme=base24-berlin";
        let (policy, html) = send(&[("cookie", theme)]).await;
        let full = html.len();
        assert!(html.contains("<style data-app-stylesheet>") && html.contains("<style data-theme-stylesheet"));
        assert_eq!(policy.matches("'sha256-").count(), 2);

        let (policy, html) = send(&[("cookie", theme), ("HX-Request", "true")]).await;
        assert!(!html.contains("<style"), "htmx discards <head>; its stylesheets are dead weight");
        assert!(policy.contains("style-src 'self';") && !policy.contains("'sha256-"), "{policy}");
        assert!(full - html.len() >= crate::assets::inline_css("app.css").as_str().len(), "the page shrinks by the stylesheet");
        assert!(html.contains("<!--partial:title-->") && html.contains("<!--partial:main-->"), "the parts htmx swaps remain");
    }

    #[tokio::test]
    async fn htmx_keeps_few_history_snapshots() {
        // htmx re-serialises its whole sessionStorage history cache on every navigation.
        let (_, html) = send(&[]).await;
        assert!(html.contains("\"historyCacheSize\":3,"));
    }

    #[tokio::test]
    async fn deferred_scripts_precede_the_inline_stylesheets() {
        let (_, html) = send(&[]).await;
        let styles = html.find("<style").unwrap();
        for script in ["/assets/vendor/htmx.min.js?v=", "/assets/app.bundle.js?v="] {
            assert!(html.find(script).unwrap() < styles, "the preload scanner must reach {script} before the CSS");
        }
        // theme.js can link a theme sheet, which must follow app.css in the cascade.
        assert!(html.find("/assets/theme.js?v=").unwrap() > html.rfind("</style>").unwrap());
    }
}
