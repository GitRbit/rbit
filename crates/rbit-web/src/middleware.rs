//! Cross-cutting request/response processing, applied in `routes::router`.

mod auth;
mod cache;
mod errors;
mod security;

pub use auth::{LEGACY_SESSION_COOKIE, SESSION_COOKIE, session, session_token};
pub(crate) use cache::is_immutable;
pub use cache::{
    NAV_EPOCH_HEADER, PINNED_CACHE_CONTROL, PRELOAD_HEADER, PRELOAD_MAX_AGE_SECONDS, cache_pinned, decline_preload, html_caching,
    is_preload, preload_guard,
};
pub use errors::error_pages;
pub use security::{SecurityHeaders, csrf_origin_check, rate_limit, security_headers};

#[cfg(test)]
use crate::{error::WebError, themes::Theme};
#[cfg(test)]
use axum::{
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
#[cfg(test)]
use cache::{is_unsafe_to_preload, weak_etag};
#[cfg(test)]
use errors::{moved_permanently, render_error_response, split_repo_path};
#[cfg(test)]
use security::is_one_click_unsubscribe;

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        middleware::{from_fn, from_fn_with_state},
        routing::get,
    };
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
            .layer(from_fn_with_state(SecurityHeaders::default(), security_headers))
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
